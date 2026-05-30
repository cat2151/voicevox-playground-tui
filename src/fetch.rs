//! VOICEVOXへの非同期fetchワーカー。
//! キャッシュキーは行インデックスではなく行文字列。
//! 同じ文字列なら同じwavが返るため、行の移動・編集後の巻き戻しでも正しく動く。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::player::PlayRequest;
use crate::voicevox;

/// キャッシュ型エイリアス: 行文字列 → WAV bytes
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
}

impl FetchRequest {
    pub fn prefetch(text: String) -> Self {
        Self {
            text,
            play_after: false,
            summary: None,
        }
    }

    pub fn play(text: String, summary: impl Into<String>) -> Self {
        Self {
            text,
            play_after: true,
            summary: Some(summary.into()),
        }
    }

    fn voice_render_message(&self) -> String {
        self.summary
            .clone()
            .unwrap_or_else(|| String::from("voice render request中 (play予約済み)"))
    }
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

        let cached: Option<Vec<u8>> = { cache.lock().unwrap().get(&req.text).cloned() };
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
            match voicevox::synthesize_line(&req.text).await {
                Ok(wav) => {
                    {
                        cache_clone
                            .lock()
                            .unwrap()
                            .insert(req.text.clone(), wav.clone());
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
