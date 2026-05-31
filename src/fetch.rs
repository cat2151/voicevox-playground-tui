//! VOICEVOXへの非同期fetchワーカー。
//! キャッシュキーは行インデックスではなく合成入力から生成する。
//! 通常行は行文字列、イントネーション編集済み行はqueryまたは保存済みpitchesを使う。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::player::PlayRequest;
use crate::{speakers, tag, voicevox};

/// キャッシュ型エイリアス: 合成入力キー → WAV bytes
pub type WavCache = Arc<Mutex<HashMap<String, Vec<u8>>>>;

/// フェッチ中フラグ型エイリアス
pub type IsFetching = Arc<AtomicBool>;

#[derive(Debug, Default)]
struct VoiceRenderOverlayInner {
    message: Option<VoiceRenderOverlayMessage>,
    generation: u64,
}

#[derive(Debug, Clone)]
struct VoiceRenderOverlayMessage {
    text: String,
    generation: u64,
}

#[derive(Debug, Clone, Default)]
pub struct VoiceRenderOverlay {
    inner: Arc<Mutex<VoiceRenderOverlayInner>>,
}

impl VoiceRenderOverlay {
    pub fn start(&self, message: impl Into<String>) -> u64 {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        inner.generation = inner.generation.saturating_add(1);
        let generation = inner.generation;
        inner.message = Some(VoiceRenderOverlayMessage {
            text: message.into(),
            generation,
        });
        generation
    }

    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        inner.generation = inner.generation.saturating_add(1);
        inner.message = None;
    }

    pub fn clear_if_current(&self, generation: u64) {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if inner
            .message
            .as_ref()
            .is_some_and(|message| message.generation == generation)
        {
            inner.message = None;
        }
    }

    pub fn current_message(&self) -> Option<String> {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .message
            .as_ref()
            .map(|message| message.text.clone())
    }
}

#[derive(Debug)]
pub struct FetchRequest {
    pub text: String,
    pub play_after: bool,
    pub summary: Option<String>,
    intonation: Option<IntonationPrefetch>,
}

#[derive(Debug)]
struct IntonationPrefetch {
    query: serde_json::Value,
    pitches: Vec<f64>,
    speaker_id: u32,
}

impl FetchRequest {
    pub fn prefetch(text: String) -> Self {
        Self {
            text,
            play_after: false,
            summary: None,
            intonation: None,
        }
    }

    pub fn prefetch_intonation(
        text: String,
        query: serde_json::Value,
        pitches: Vec<f64>,
        speaker_id: u32,
    ) -> Self {
        Self {
            text,
            play_after: false,
            summary: None,
            intonation: Some(IntonationPrefetch {
                query,
                pitches,
                speaker_id,
            }),
        }
    }

    pub fn play(text: String, summary: impl Into<String>) -> Self {
        Self {
            text,
            play_after: true,
            summary: Some(summary.into()),
            intonation: None,
        }
    }

    fn voice_render_message(&self) -> String {
        self.summary
            .clone()
            .unwrap_or_else(|| String::from("voice render request中 (play予約済み)"))
    }

    pub(crate) fn cache_key(&self) -> Option<String> {
        match &self.intonation {
            Some(intonation) if intonation.query.is_null() => {
                pitches_only_intonation_cache_key(&self.text, &intonation.pitches)
            }
            Some(intonation) => intonation_cache_key(intonation.speaker_id, &intonation.query),
            None => Some(self.text.clone()),
        }
    }

    async fn synthesize(&self) -> anyhow::Result<Vec<u8>> {
        match &self.intonation {
            Some(intonation) if intonation.query.is_null() => {
                let (query, speaker_id) =
                    voicevox::get_audio_query_with_pitches(&self.text, &intonation.pitches).await?;
                voicevox::synthesize_with_query(&query, speaker_id).await
            }
            Some(intonation) => {
                voicevox::synthesize_with_query(&intonation.query, intonation.speaker_id).await
            }
            None => voicevox::synthesize_line(&self.text).await,
        }
    }
}

pub fn intonation_cache_key(speaker_id: u32, query: &serde_json::Value) -> Option<String> {
    serde_json::to_string(query)
        .ok()
        .map(|query| format!("intonation:{speaker_id}:{query}"))
}

pub fn pitches_only_intonation_cache_key(text: &str, pitches: &[f64]) -> Option<String> {
    speakers::try_get()?;
    let mut segments = tag::parse_line(text);
    if segments.len() != 1 {
        return None;
    }
    let (segment_text, ctx) = segments.swap_remove(0);
    serde_json::to_string(&(ctx.speaker_id, segment_text, pitches))
        .ok()
        .map(|payload| format!("intonation:pitches:{payload}"))
}

pub fn spawn_worker(
    rx: mpsc::Receiver<FetchRequest>,
    cache: WavCache,
    play_tx: mpsc::Sender<PlayRequest>,
    is_fetching: IsFetching,
    voice_render_overlay: VoiceRenderOverlay,
) {
    tokio::spawn(worker_loop(
        rx,
        cache,
        play_tx,
        is_fetching,
        voice_render_overlay,
    ));
}

async fn worker_loop(
    mut rx: mpsc::Receiver<FetchRequest>,
    cache: WavCache,
    play_tx: mpsc::Sender<PlayRequest>,
    is_fetching: IsFetching,
    voice_render_overlay: VoiceRenderOverlay,
) {
    let mut current_handle: Option<tokio::task::JoinHandle<()>> = None;
    let mut current_is_play: bool = false;
    // 世代カウンタ: abortされたタスクが遅れてis_fetchingをリセットするのを防ぐ
    let fetch_gen = Arc::new(AtomicU64::new(0));

    while let Some(req) = rx.recv().await {
        // タスクが自然完了していた場合はis_play状態をリセットする。
        // これにより、再生fetchが完了した後にprefetchリクエストが正しく処理される。
        if let Some(h) = &current_handle {
            if h.is_finished() {
                current_handle = None;
                current_is_play = false;
            }
        }

        // play_after=true（再生リクエスト）は常に優先し既存タスクをabort。
        // play_after=false（prefetch）は既存のprefetchのみをabortし、
        // 進行中の再生fetchはabortしない。
        let should_abort = req.play_after || !current_is_play;

        if should_abort {
            if let Some(handle) = current_handle.take() {
                handle.abort();
                is_fetching.store(false, Ordering::Relaxed);
                if current_is_play {
                    voice_render_overlay.clear();
                }
                current_is_play = false;
            }
        }

        if req.text.trim().is_empty() {
            continue;
        }

        // 再生fetchが進行中の場合、prefetchはスキップ
        if !req.play_after && current_is_play {
            continue;
        }

        let Some(cache_key) = req.cache_key() else {
            continue;
        };
        let cached: Option<Vec<u8>> = { cache.lock().unwrap().get(&cache_key).cloned() };
        if let Some(wav) = cached {
            if req.play_after {
                voice_render_overlay.clear();
                let _ = play_tx
                    .send(PlayRequest {
                        wav,
                        source_text: req.text.clone(),
                    })
                    .await;
            }
            continue;
        }

        is_fetching.store(true, Ordering::Relaxed);
        let voice_render_generation = if req.play_after {
            Some(voice_render_overlay.start(req.voice_render_message()))
        } else {
            None
        };

        let gen = fetch_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let fetch_gen_clone = Arc::clone(&fetch_gen);
        let cache_clone = Arc::clone(&cache);
        let play_tx_clone = play_tx.clone();
        let is_fetching_clone = Arc::clone(&is_fetching);
        let voice_render_overlay_clone = voice_render_overlay.clone();

        current_is_play = req.play_after;
        current_handle = Some(tokio::spawn(async move {
            match req.synthesize().await {
                Ok(wav) => {
                    {
                        cache_clone.lock().unwrap().insert(cache_key, wav.clone());
                    }
                    if req.play_after {
                        let _ = play_tx_clone
                            .send(PlayRequest {
                                wav,
                                source_text: req.text.clone(),
                            })
                            .await;
                    }
                }
                Err(e) => crate::runtime_notice::set_runtime_notice(format!("[fetch error] {e}")),
            }
            // 自分が最新のタスクである場合のみis_fetchingをリセット
            if fetch_gen_clone.load(Ordering::Relaxed) == gen {
                is_fetching_clone.store(false, Ordering::Relaxed);
                if let Some(generation) = voice_render_generation {
                    voice_render_overlay_clone.clear_if_current(generation);
                }
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_render_overlay_clear_if_current_keeps_newer_message() {
        let overlay = VoiceRenderOverlay::default();
        let first = overlay.start("first");
        let second = overlay.start("second");

        overlay.clear_if_current(first);

        assert_eq!(overlay.current_message(), Some("second".to_string()));

        overlay.clear_if_current(second);

        assert_eq!(overlay.current_message(), None);
    }
}
