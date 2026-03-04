//! アプリケーション状態と状態遷移ロジック。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};


use tokio::sync::mpsc;
use tui_textarea::TextArea;

use crate::fetch::{FetchRequest, IsFetching, WavCache};
use crate::player;
use crate::tag;

#[derive(Debug, Clone, PartialEq)]
pub enum Mode { Normal, Insert }

pub struct App {
    pub lines:         Vec<String>,
    pub cursor:        usize,
    pub textarea:      TextArea<'static>,
    pub mode:          Mode,
    /// キャッシュキー = 行文字列（インデックスではない）
    pub cache:         WavCache,
    pub status_msg:    String,
    pub fetch_tx:      mpsc::Sender<FetchRequest>,
    pub play_tx:       mpsc::Sender<Vec<u8>>,
    pub visible_lines: usize,
    pub pending_d:     bool,
    pub yank_buf:      Option<String>,
    /// fetchワーカーがAPI呼び出し中かどうか
    pub is_fetching:   IsFetching,
    /// 自動アップデートのためにアプリを終了すべきか
    pub should_exit_for_update: Arc<AtomicBool>,
}

impl App {
    pub fn new(lines: Vec<String>) -> Self {
        let lines = compress_trailing_empty(lines);
        let cache: WavCache = Arc::new(Mutex::new(HashMap::new()));

        let (play_tx, play_rx) = mpsc::channel::<Vec<u8>>(8);
        player::spawn_player(play_rx);

        let is_fetching: IsFetching = Arc::new(AtomicBool::new(false));
        let (fetch_tx, fetch_rx) = mpsc::channel::<FetchRequest>(64);
        crate::fetch::spawn_worker(fetch_rx, Arc::clone(&cache), play_tx.clone(), Arc::clone(&is_fetching));

        let cursor = if lines.is_empty() { 0 } else { lines.len() - 1 };
        Self {
            lines, cursor,
            textarea:      TextArea::default(),
            mode:          Mode::Normal,
            cache,
            status_msg:    String::from("ready"),
            fetch_tx, play_tx,
            visible_lines: 24,
            pending_d:     false,
            yank_buf:      None,
            is_fetching,
            should_exit_for_update: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn init(&mut self) {
        let idx = self.cursor;
        self.fetch_and_play(idx).await;
        self.prefetch_neighbors().await;
    }

    // ── Normal mode ───────────────────────────────────────────────────────────

    pub async fn move_cursor(&mut self, delta: i32) {
        self.pending_d = false;
        if self.lines.is_empty() { return; }
        let next = (self.cursor as i32 + delta)
            .clamp(0, self.lines.len() as i32 - 1) as usize;
        if next != self.cursor {
            self.cursor = next;
            self.fetch_and_play(self.cursor).await;
            self.prefetch_neighbors().await;
        }
    }

    pub async fn play_current(&mut self) {
        self.pending_d = false;
        self.fetch_and_play(self.cursor).await;
    }

    pub async fn delete_current_line(&mut self) {
        self.pending_d = false;
        self.yank_buf = Some(self.lines.get(self.cursor).cloned().unwrap_or_default());
        if self.lines.len() <= 1 {
            self.lines  = vec![String::new()];
            self.cursor = 0;
            return;
        }
        self.lines.remove(self.cursor);
        if self.cursor >= self.lines.len() { self.cursor = self.lines.len() - 1; }
        self.fetch_and_play(self.cursor).await;
        self.prefetch_neighbors().await;
    }

    pub async fn paste_below(&mut self) {
        self.pending_d = false;
        let text = match &self.yank_buf { Some(t) => t.clone(), None => return };
        self.lines.insert(self.cursor + 1, text);
        self.cursor += 1;
        self.fetch_and_play(self.cursor).await;
        self.prefetch_neighbors().await;
    }

    pub async fn paste_above(&mut self) {
        self.pending_d = false;
        let text = match &self.yank_buf { Some(t) => t.clone(), None => return };
        self.lines.insert(self.cursor, text);
        self.fetch_and_play(self.cursor).await;
        self.prefetch_neighbors().await;
    }

    // ── Insert mode ───────────────────────────────────────────────────────────

    /// i: 現在行を編集。現在行が空なら1つ上の行の末尾コンテキストを継承する。
    pub fn enter_insert_current(&mut self) {
        self.pending_d = false;
        let current = self.lines.get(self.cursor).cloned().unwrap_or_default();
        let text = if current.trim().is_empty() {
            // 空行なら1つ上の行のコンテキストを継承
            if self.cursor > 0 {
                self.lines.get(self.cursor - 1)
                    .map(|l| tag::ctx_to_prefix(&tag::tail_ctx(l)))
                    .unwrap_or_default()
            } else {
                String::new()
            }
        } else {
            current
        };
        self.textarea   = make_textarea(text);
        self.mode       = Mode::Insert;
        self.status_msg = String::from("-- INSERT --");
    }

    /// o: 現在行の下に空行を挿入。現在行の末尾コンテキストを継承。
    pub fn enter_insert_below(&mut self) {
        self.pending_d = false;
        let prefix = self.lines.get(self.cursor)
            .map(|l| tag::ctx_to_prefix(&tag::tail_ctx(l)))
            .unwrap_or_default();
        self.lines.insert(self.cursor + 1, prefix.clone());
        self.cursor    += 1;
        self.textarea   = make_textarea(prefix);
        self.mode       = Mode::Insert;
        self.status_msg = String::from("-- INSERT --");
    }

    /// O: 現在行の上に空行を挿入。1つ上の行の末尾コンテキストを継承。
    pub fn enter_insert_above(&mut self) {
        self.pending_d = false;
        let prefix = if self.cursor > 0 {
            self.lines.get(self.cursor - 1)
                .map(|l| tag::ctx_to_prefix(&tag::tail_ctx(l)))
                .unwrap_or_default()
        } else {
            String::new()
        };
        self.lines.insert(self.cursor, prefix.clone());
        self.textarea   = make_textarea(prefix);
        self.mode       = Mode::Insert;
        self.status_msg = String::from("-- INSERT --");
    }

    /// 確定: [N]展開 → lines更新 → Normalへ → 再生
    pub async fn commit_insert(&mut self) {
        let raw  = self.textarea.lines().first().cloned().unwrap_or_default();
        let text = tag::expand_id_tags(&raw);
        if self.cursor < self.lines.len() {
            self.lines[self.cursor] = text;
        }
        self.mode       = Mode::Normal;
        self.status_msg = String::from("ready");
        self.lines = compress_trailing_empty(std::mem::take(&mut self.lines));
        if self.cursor >= self.lines.len() {
            self.cursor = self.lines.len().saturating_sub(1);
        }
        self.fetch_and_play(self.cursor).await;
        self.prefetch_neighbors().await;
    }

    /// Insert中の文字変化ごとに呼ぶ（debounce prefetch）
    pub async fn on_edit_buf_changed(&mut self) {
        let raw  = self.textarea.lines().first().cloned().unwrap_or_default();
        let text = tag::expand_id_tags(&raw);  // [N]展開後のキーでfetchする
        if text.trim().is_empty() { return; }
        let _ = self.fetch_tx.send(FetchRequest { text, play_after: false }).await;
    }

    /// ステータス表示文字列: Insertモード中にfetch中なら "[fetching...]" を返す
    pub fn status_display(&self) -> &str {
        if self.mode == Mode::Insert && self.is_fetching.load(Ordering::Relaxed) {
            "[fetching...]"
        } else {
            &self.status_msg
        }
    }

    // ── 内部ヘルパー ──────────────────────────────────────────────────────────

    async fn fetch_and_play(&mut self, index: usize) {
        if index >= self.lines.len() || self.lines[index].trim().is_empty() { return; }
        let text   = self.lines[index].clone();
        let cached = { self.cache.lock().unwrap().get(&text).cloned() };
        if let Some(wav) = cached {
            let _ = self.play_tx.send(wav).await;
            self.status_msg = format!("[♪ cached] line {}", index + 1);
        } else {
            let _ = self.fetch_tx.send(FetchRequest { text, play_after: true }).await;
            self.status_msg = format!("[fetching...] line {}", index + 1);
        }
    }

    async fn prefetch_neighbors(&self) {
        let len = self.lines.len();
        if len == 0 { return; }
        let half      = self.visible_lines / 2;
        let win_start = self.cursor.saturating_sub(half);
        let win_end   = (win_start + self.visible_lines).min(len);
        let win_start = win_end.saturating_sub(self.visible_lines);
        let mut targets: Vec<usize> = Vec::new();
        for d in 1..=self.visible_lines as i32 {
            for &delta in &[d, -d] {
                let idx = self.cursor as i32 + delta;
                if idx >= win_start as i32 && idx < win_end as i32 {
                    targets.push(idx as usize);
                }
            }
        }
        targets.dedup();
        for idx in targets {
            let text = self.lines[idx].clone();
            if text.trim().is_empty() { continue; }
            if self.cache.lock().unwrap().contains_key(&text) { continue; }
            let _ = self.fetch_tx.send(FetchRequest { text, play_after: false }).await;
        }
    }
}

// ── ユーティリティ ────────────────────────────────────────────────────────────

fn compress_trailing_empty(mut lines: Vec<String>) -> Vec<String> {
    while lines.len() > 1 && lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        lines.pop();
    }
    if lines.is_empty() { lines.push(String::new()); }
    if lines.last().map(|l| !l.trim().is_empty()).unwrap_or(false) {
        lines.push(String::new());
    }
    lines
}

fn make_textarea(initial: String) -> TextArea<'static> {
    let mut ta = TextArea::new(vec![initial]);
    ta.move_cursor(tui_textarea::CursorMove::End);
    ta
}
