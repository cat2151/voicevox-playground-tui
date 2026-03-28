//! 再生・fetch・バックグラウンドprefetch関連の内部ヘルパー。

use std::sync::Arc;

use crate::background_prefetch;
use crate::fetch::FetchRequest;
use crate::player::PlayRequest;

use super::{utils, App, IntonationLineData};

impl App {
    /// イントネーションキャッシュキーを生成する。
    /// シリアライズに失敗した場合は None を返す（キャッシュをスキップする）。
    pub(crate) fn intonation_cache_key(
        speaker_id: u32,
        query: &serde_json::Value,
    ) -> Option<String> {
        serde_json::to_string(query)
            .ok()
            .map(|q| format!("intonation:{}:{}", speaker_id, q))
    }

    /// 指定行の内容を取得し、キャッシュ済み音声または fetch/intonation 合成結果を再生する。
    /// 空行や範囲外インデックスは無視する。
    pub(super) async fn fetch_and_play(&mut self, index: usize) {
        if index >= self.lines.len() || self.lines[index].trim().is_empty() {
            return;
        }
        // 折りたたみ用の行頭spaceは音声合成に影響しないため、trim_startしてcacheキー・fetchリクエストに使う
        let text = self.lines[index].trim_start().to_owned();

        // イントネーション編集済みの場合はキャッシュを確認し、あれば即再生、なければ合成してキャッシュに保存する
        if let Some(data) = self
            .line_intonations
            .get(index)
            .and_then(|d| d.as_ref())
            .cloned()
        {
            // query が Null の場合は history.txt から復元した pitches-only 状態を示す。
            // この場合は audio_query をAPIから遅延取得し、完全なIntonationLineDataに昇格させてから再生する。
            let data = if data.query.is_null() {
                match self.resolve_pitches_only(index, &data).await {
                    Some(resolved) => resolved,
                    None => {
                        // API取得に失敗した場合は通常の合成にフォールスルー
                        let cached = { self.cache.lock().unwrap().get(&text).cloned() };
                        if let Some(wav) = cached {
                            let _ = self
                                .play_tx
                                .send(PlayRequest {
                                    wav,
                                    source_text: text.clone(),
                                })
                                .await;
                            self.status_msg = format!("[♪ cached] line {}", index + 1);
                        } else {
                            let _ = self
                                .fetch_tx
                                .send(FetchRequest {
                                    text,
                                    play_after: true,
                                })
                                .await;
                            self.status_msg = format!("[fetching...] line {}", index + 1);
                        }
                        return;
                    }
                }
            } else {
                data
            };

            if let Some(cache_key) = Self::intonation_cache_key(data.speaker_id, &data.query) {
                let cached = { self.cache.lock().unwrap().get(&cache_key).cloned() };
                if let Some(wav) = cached {
                    let _ = self
                        .play_tx
                        .send(PlayRequest {
                            wav,
                            source_text: text.clone(),
                        })
                        .await;
                    self.status_msg = format!("[♬ cached] line {}", index + 1);
                    return;
                }
            }
            self.spawn_intonation_play(data.query, data.speaker_id, text.clone());
            self.status_msg = format!("[♬ intonation] line {}", index + 1);
            return;
        }

        let cached = { self.cache.lock().unwrap().get(&text).cloned() };
        if let Some(wav) = cached {
            let _ = self
                .play_tx
                .send(PlayRequest {
                    wav,
                    source_text: text.clone(),
                })
                .await;
            self.status_msg = format!("[♪ cached] line {}", index + 1);
        } else {
            let _ = self
                .fetch_tx
                .send(FetchRequest {
                    text,
                    play_after: true,
                })
                .await;
            self.status_msg = format!("[fetching...] line {}", index + 1);
        }
    }

    /// pitches-onlyのIntonationLineData（queryがNull）に対してaudio_queryをAPIから取得し、
    /// 保存済みpitchesを適用してline_intonationsを更新する。
    /// pitches適用後にqueryから再抽出することでモーラ数との整合性を保つ。
    /// 成功した場合は解決済みのIntonationLineDataを返す。
    pub(super) async fn resolve_pitches_only(
        &mut self,
        index: usize,
        data: &IntonationLineData,
    ) -> Option<IntonationLineData> {
        let line = self.lines.get(index)?.clone();
        if line.trim().is_empty() {
            return None;
        }
        let mut segments = crate::tag::parse_line(&line);
        if segments.len() != 1 {
            return None;
        }
        let (seg_text, ctx) = segments.swap_remove(0);
        let speaker_id = ctx.speaker_id;
        match crate::voicevox::get_audio_query(&seg_text, speaker_id).await {
            Ok(mut query) => {
                // 保存済みpitchesを適用した後、queryから再抽出して長さをモーラ数に揃える
                crate::voicevox::set_mora_pitches(&mut query, &data.pitches);
                let (mora_texts, applied_pitches) = crate::voicevox::extract_mora_data(&query);
                if mora_texts.is_empty() {
                    return None;
                }
                let resolved = IntonationLineData {
                    query: query.clone(),
                    mora_texts,
                    pitches: applied_pitches,
                    speaker_id,
                };
                if index < self.line_intonations.len() {
                    self.line_intonations[index] = Some(resolved.clone());
                }
                Some(resolved)
            }
            Err(_) => None,
        }
    }

    /// イントネーションqueryを使って合成・再生するタスクを起動する。
    /// 前回のタスクがあればabortしてから新しいタスクを起動する（並列実行を防ぐ）。
    /// 合成結果はWavCacheに保存し、次回以降の再生でキャッシュから即時再生できるようにする。
    pub(super) fn spawn_intonation_play(
        &mut self,
        query: serde_json::Value,
        speaker_id: u32,
        source_text: String,
    ) {
        if let Some(h) = self.intonation_play_handle.take() {
            h.abort();
        }
        let play_tx = self.play_tx.clone();
        let cache = Arc::clone(&self.cache);
        let cache_key = Self::intonation_cache_key(speaker_id, &query);
        self.intonation_play_handle = Some(tokio::spawn(async move {
            if let Ok(wav) = crate::voicevox::synthesize_with_query(&query, speaker_id).await {
                if let Some(key) = cache_key {
                    cache.lock().unwrap().insert(key, wav.clone());
                }
                let _ = play_tx.send(PlayRequest { wav, source_text }).await;
            }
        }));
    }

    /// イントネーションキャッシュの古いエントリをすべて削除する。
    /// イントネーション確定時に呼び出し、中間的な pitch 編集で蓄積した不要エントリを解放する。
    pub(super) fn evict_intonation_cache(&mut self) {
        self.cache
            .lock()
            .unwrap()
            .retain(|k, _| !k.starts_with("intonation:"));
    }

    /// 現在のカーソル位置と折りたたみ状態に基づき、現在行のfetch完了後に
    /// 表示範囲内の未キャッシュ行を順次fetchするバックグラウンドprefetchタスクを再起動する。
    /// 既存のprefetchタスクがあれば中断してから新しいタスクを起動する。
    pub(super) fn restart_background_prefetch(&mut self) {
        if let Some(h) = self.bg_prefetch_handle.take() {
            h.abort();
        }
        // カーソル行がイントネーション編集済みの場合はイントネーション用のキャッシュキーを使う。
        // 通常の行テキストをキーとすると、イントネーション合成結果がキャッシュされないため
        // wait_for_cachedが30秒タイムアウトするまで他の行のprefetchが始まらない。
        let cursor_cache_key = {
            let intonation_key = self
                .line_intonations
                .get(self.cursor)
                .and_then(|d| d.as_ref())
                .filter(|d| !d.query.is_null())
                .and_then(|d| Self::intonation_cache_key(d.speaker_id, &d.query));
            // 折りたたみ用の行頭spaceはcacheキーから除外する
            intonation_key.unwrap_or_else(|| {
                self.lines
                    .get(self.cursor)
                    .map(|l| l.trim_start().to_owned())
                    .unwrap_or_default()
            })
        };
        // 折りたたみ時は表示行のみをprefetch対象とする
        let target_texts: Vec<String> = if self.folded {
            let visible_indices = self.visible_line_indices();
            let visible_texts: Vec<String> = visible_indices
                .iter()
                .map(|&i| self.lines[i].trim_start().to_owned())
                .collect();
            let vis_cursor = utils::nearest_vis_pos(self.cursor, &visible_indices);
            background_prefetch::compute_prefetch_targets(
                vis_cursor,
                self.visible_lines,
                &visible_texts,
            )
            .into_iter()
            .map(|idx| visible_texts[idx].clone())
            .collect()
        } else {
            // 全行ではなく表示ウィンドウ内の対象行のみをcloneして渡す
            background_prefetch::compute_prefetch_targets(
                self.cursor,
                self.visible_lines,
                &self.lines,
            )
            .into_iter()
            .map(|idx| self.lines[idx].trim_start().to_owned())
            .collect()
        };
        self.bg_prefetch_handle = Some(background_prefetch::spawn_background_prefetch(
            cursor_cache_key,
            target_texts,
            Arc::clone(&self.cache),
            Arc::clone(&self.is_fetching),
            self.fetch_tx.clone(),
        ));
    }
}

#[cfg(test)]
#[path = "../tests/app/playback.rs"]
mod tests;
