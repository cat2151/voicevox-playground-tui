//! Insertモードの操作。

use std::sync::atomic::Ordering;

use crate::fetch::FetchRequest;
use crate::tag;

use super::{App, Mode};

impl App {
    /// i: 現在行を編集。現在行が空なら1つ上の行の末尾コンテキストを継承する。
    pub fn enter_insert_current(&mut self) {
        self.reset_pending_prefixes();
        let current = self.lines.get(self.cursor).cloned().unwrap_or_default();
        let text = if current.trim().is_empty() {
            // 空行なら1つ上の行のコンテキストを継承
            if self.cursor > 0 {
                self.lines
                    .get(self.cursor - 1)
                    .map(|l| tag::ctx_to_prefix(&tag::tail_ctx(l)))
                    .unwrap_or_default()
            } else {
                String::new()
            }
        } else {
            current
        };
        self.textarea = super::utils::make_textarea(text);
        self.mode = Mode::Insert;
        self.status_msg = String::from("-- INSERT --");
    }

    /// o: 現在行の下に空行を挿入。現在行の末尾コンテキストを継承。
    pub fn enter_insert_below(&mut self) {
        self.reset_pending_prefixes();
        let prefix = self
            .lines
            .get(self.cursor)
            .map(|l| tag::ctx_to_prefix(&tag::tail_ctx(l)))
            .unwrap_or_default();
        self.lines.insert(self.cursor + 1, prefix.clone());
        self.line_intonations.insert(self.cursor + 1, None);
        self.cursor += 1;
        self.textarea = super::utils::make_textarea(prefix);
        self.mode = Mode::Insert;
        self.status_msg = String::from("-- INSERT --");
    }

    /// O: 現在行の上に空行を挿入。1つ上の行の末尾コンテキストを継承。
    pub fn enter_insert_above(&mut self) {
        self.reset_pending_prefixes();
        let prefix = if self.cursor > 0 {
            self.lines
                .get(self.cursor - 1)
                .map(|l| tag::ctx_to_prefix(&tag::tail_ctx(l)))
                .unwrap_or_default()
        } else {
            String::new()
        };
        self.lines.insert(self.cursor, prefix.clone());
        self.line_intonations.insert(self.cursor, None);
        self.textarea = super::utils::make_textarea(prefix);
        self.mode = Mode::Insert;
        self.status_msg = String::from("-- INSERT --");
    }

    /// 確定: [N]展開 → 行中途のspeaker/style変化で行分割 → lines更新 → Normalへ → 再生
    pub async fn commit_insert(&mut self) {
        self.status_msg = String::from("ready");
        self.commit_lines().await;
        self.mode = Mode::Normal;
    }

    /// ENTERで確定: 現在行を確定し、下に空行を挿入してINSERTモードで編集開始（vim の o 相当）
    pub async fn commit_and_insert_below(&mut self) {
        self.commit_lines().await;
        self.enter_insert_below();
    }

    /// INSERTモードのバッファをlinesに書き戻し、再生・prefetchを行う内部ヘルパー。
    /// modeは変更しない。
    async fn commit_lines(&mut self) {
        let raw = self.textarea.lines().first().cloned().unwrap_or_default();
        let text = tag::expand_id_tags(&raw);
        let split_lines = tag::split_by_ctx_change(&text);
        if self.cursor < self.lines.len() {
            // split_by_ctx_change は常に1要素以上を返す
            // テキストが変わった場合のみ現在行のイントネーションをクリアする。
            // 折りたたみ用の行頭spaceは音声合成に影響しないため、trim_startして比較する。
            if let Some(first_line) = split_lines.first() {
                if self.lines[self.cursor].trim_start() != first_line.trim_start() {
                    self.line_intonations[self.cursor] = None;
                }
                self.lines[self.cursor] = first_line.clone();
            }
            for (i, extra_line) in split_lines[1..].iter().enumerate() {
                self.lines.insert(self.cursor + 1 + i, extra_line.clone());
                self.line_intonations.insert(self.cursor + 1 + i, None);
            }
        }
        self.lines = super::utils::compress_trailing_empty(std::mem::take(&mut self.lines));
        // line_intonations の長さを lines に合わせる（末尾の空行は常にイントネーションなし）
        self.line_intonations.resize(self.lines.len(), None);
        if self.cursor >= self.lines.len() {
            self.cursor = self.lines.len().saturating_sub(1);
        }
        self.fetch_and_play(self.cursor).await;
        self.restart_background_prefetch();
    }

    /// Insert中の文字変化ごとに呼ぶ（debounce prefetch）
    pub async fn on_edit_buf_changed(&mut self) {
        let raw = self.textarea.lines().first().cloned().unwrap_or_default();
        // [N]展開後、折りたたみ用の行頭spaceを除いたキーでfetchする
        let text = tag::expand_id_tags(&raw).trim_start().to_owned();
        if text.trim().is_empty() {
            return;
        }
        let _ = self
            .fetch_tx
            .send(FetchRequest {
                text,
                play_after: false,
            })
            .await;
    }

    /// ステータス表示文字列: Insertモード中にfetch中なら "[fetching...]" を返す
    pub fn status_display(&self) -> &str {
        if self.mode == Mode::Insert && self.is_fetching.load(Ordering::Relaxed) {
            "[fetching...]"
        } else {
            &self.status_msg
        }
    }
}

#[cfg(test)]
#[path = "../tests/app/insert_mode.rs"]
mod tests;
