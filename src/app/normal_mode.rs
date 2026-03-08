//! Normalモードの操作。

use super::App;

impl App {
    pub async fn move_cursor(&mut self, delta: i32) {
        self.reset_pending_prefixes();
        if self.lines.is_empty() { return; }
        let next = if self.folded {
            let visible = self.visible_line_indices();
            if visible.is_empty() { return; }
            // カーソルが非表示行にある場合は最も近い表示行の位置から動かす
            let vis_pos = super::utils::nearest_vis_pos(self.cursor, &visible);
            let next_vis = (vis_pos as i32 + delta)
                .clamp(0, visible.len() as i32 - 1) as usize;
            visible[next_vis]
        } else {
            (self.cursor as i32 + delta)
                .clamp(0, self.lines.len() as i32 - 1) as usize
        };
        if next != self.cursor {
            self.cursor = next;
            self.fetch_and_play(self.cursor).await;
            self.restart_background_prefetch();
        }
    }

    /// zm: 折りたたみ。行頭に半角spaceのある行を非表示にする。
    pub fn fold(&mut self) {
        self.reset_pending_prefixes();
        self.folded = true;
        // カーソルが非表示行にある場合、直前の表示行に移動する
        let visible = self.visible_line_indices();
        if !visible.is_empty() && !visible.contains(&self.cursor) {
            let new_cursor = visible.iter().rev().find(|&&i| i < self.cursor)
                .or_else(|| visible.first())
                .copied();
            if let Some(c) = new_cursor {
                self.cursor = c;
            }
        }
    }

    /// zr: 折りたたみを開く。すべての行を表示する。
    pub fn unfold(&mut self) {
        self.reset_pending_prefixes();
        self.folded = false;
    }

    pub async fn play_current(&mut self) {
        self.reset_pending_prefixes();
        self.fetch_and_play(self.cursor).await;
    }

    pub async fn delete_current_line(&mut self) {
        self.reset_pending_prefixes();
        self.yank_buf = Some(self.lines.get(self.cursor).cloned().unwrap_or_default());
        if self.lines.len() <= 1 {
            self.lines  = vec![String::new()];
            self.line_intonations = vec![None];
            self.cursor = 0;
            return;
        }
        self.lines.remove(self.cursor);
        self.line_intonations.remove(self.cursor);
        if self.cursor >= self.lines.len() { self.cursor = self.lines.len() - 1; }
        self.fetch_and_play(self.cursor).await;
        self.restart_background_prefetch();
    }

    pub async fn paste_below(&mut self) {
        self.reset_pending_prefixes();
        let text = match &self.yank_buf { Some(t) => t.clone(), None => return };
        self.lines.insert(self.cursor + 1, text);
        self.line_intonations.insert(self.cursor + 1, None);
        self.cursor += 1;
        // 折りたたみ時、カーソルが非表示行（行頭space）になる場合は最も近い表示行へ移動する
        self.normalize_cursor_for_fold();
        self.fetch_and_play(self.cursor).await;
        self.restart_background_prefetch();
    }

    pub async fn paste_above(&mut self) {
        self.reset_pending_prefixes();
        let text = match &self.yank_buf { Some(t) => t.clone(), None => return };
        self.lines.insert(self.cursor, text);
        self.line_intonations.insert(self.cursor, None);
        // 折りたたみ時、カーソルが非表示行（行頭space）になる場合は最も近い表示行へ移動する
        self.normalize_cursor_for_fold();
        self.fetch_and_play(self.cursor).await;
        self.restart_background_prefetch();
    }

    /// "+p: システムクリップボードの内容を現在行の下に貼り付ける。
    pub async fn paste_below_from_clipboard(&mut self) {
        self.reset_pending_prefixes();
        let clip_lines = match self.read_clipboard_lines() { Ok(l) => l, Err(()) => return };
        if clip_lines.is_empty() { return; }
        let insert_pos = self.cursor + 1;
        let clip_count = clip_lines.len();
        let tail = self.lines.split_off(insert_pos);
        let tail_intonations = self.line_intonations.split_off(insert_pos);
        self.lines.extend(clip_lines);
        self.line_intonations.extend(vec![None; clip_count]);
        self.lines.extend(tail);
        self.line_intonations.extend(tail_intonations);
        self.cursor = insert_pos;
        self.normalize_cursor_for_fold();
        self.fetch_and_play(self.cursor).await;
        self.restart_background_prefetch();
    }

    /// "+P: システムクリップボードの内容を現在行の上に貼り付ける。
    pub async fn paste_above_from_clipboard(&mut self) {
        self.reset_pending_prefixes();
        let clip_lines = match self.read_clipboard_lines() { Ok(l) => l, Err(()) => return };
        if clip_lines.is_empty() { return; }
        let clip_count = clip_lines.len();
        let tail = self.lines.split_off(self.cursor);
        let tail_intonations = self.line_intonations.split_off(self.cursor);
        self.lines.extend(clip_lines);
        self.line_intonations.extend(vec![None; clip_count]);
        self.lines.extend(tail);
        self.line_intonations.extend(tail_intonations);
        self.normalize_cursor_for_fold();
        self.fetch_and_play(self.cursor).await;
        self.restart_background_prefetch();
    }

    /// システムクリップボードからテキストを読み込み、行に分割して返す。
    /// 失敗した場合は `status_msg` にエラーメッセージを設定して `Err(())` を返す。
    fn read_clipboard_lines(&mut self) -> Result<Vec<String>, ()> {
        let mut cb = match arboard::Clipboard::new() {
            Ok(c) => c,
            Err(e) => {
                self.status_msg = format!("[clipboard] init failed: {}", e);
                return Err(());
            }
        };
        let text = match cb.get_text() {
            Ok(t) => t,
            Err(e) => {
                self.status_msg = format!("[clipboard] read failed: {}", e);
                return Err(());
            }
        };
        Ok(text.lines().map(|l| l.to_string()).collect())
    }
}
