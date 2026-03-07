//! アプリケーション状態と状態遷移ロジック。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;


use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tui_textarea::TextArea;

use crate::background_prefetch;
use crate::fetch::{FetchRequest, IsFetching, WavCache};
use crate::player;
use crate::tag;

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Insert,
    /// コロンコマンド入力モード（例: :tabnew）
    Command,
    /// 自動検出されたアップデートの選択ダイアログ
    UpdateAvailableDialog,
    /// qキー押下時に表示するアップデート選択ダイアログ
    QuitWithUpdateDialog,
}

/// アップデート実行方法の選択結果
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateAction {
    /// 裏でアップデート（バックグラウンドプロセスで実行）
    Background,
    /// 表でアップデート（端末にビルドログを表示）
    Foreground,
}

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
    /// "z"キー待機中（zm/zrのプレフィックス）
    pub pending_z:     bool,
    /// "g"キー待機中（gt/gTのプレフィックス）
    pub pending_g:     bool,
    /// `"`キー待機中（レジスタ指定のプレフィックス）
    pub pending_quote: bool,
    /// `"+`入力済み（クリップボードペーストのプレフィックス）
    pub pending_clipboard: bool,
    pub yank_buf:      Option<String>,
    /// 折りたたみ中かどうか（行頭spaceのある行を非表示にする）
    pub folded:        bool,
    /// fetchワーカーがAPI呼び出し中かどうか
    pub is_fetching:   IsFetching,
    /// アップデートが利用可能かどうか（バックグラウンドチェックがセットする）
    pub update_available: Arc<AtomicBool>,
    /// アップデートダイアログを一時的に却下したかどうか
    pub update_dismissed: bool,
    /// ユーザーが選択したアップデート実行方法
    pub update_action: Option<UpdateAction>,
    /// バックグラウンドprefetchタスクのハンドル（カーソル移動時にキャンセル）
    bg_prefetch_handle: Option<JoinHandle<()>>,
    /// NormalモードでESCを押した際に"q:quit"ヒントをハイライト表示する期限
    pub esc_hint_until: Option<Instant>,
    /// タブごとの (lines, cursor, folded) を保存するリスト（アクティブタブ含む全タブ）
    pub tabs:           Vec<(Vec<String>, usize, bool)>,
    /// 現在アクティブなタブのインデックス（0始まり）
    pub active_tab:     usize,
    /// コマンドモード（":tabnew" など）の入力バッファ
    pub command_buf:    String,
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
        // tabs[0] のlinesはプレースホルダー（実際のlinesはself.linesに保持される）。
        // タブ切り替え時にmem::swapでlinesを交換するため、初期値は空vecで良い。
        let tabs = vec![(vec![], 0usize, false)];
        Self {
            lines, cursor,
            textarea:      TextArea::default(),
            mode:          Mode::Normal,
            cache,
            status_msg:    String::from("ready"),
            fetch_tx, play_tx,
            visible_lines: 24,
            pending_d:     false,
            pending_z:     false,
            pending_g:     false,
            pending_quote: false,
            pending_clipboard: false,
            yank_buf:      None,
            folded:        false,
            is_fetching,
            update_available: Arc::new(AtomicBool::new(false)),
            update_dismissed: false,
            update_action: None,
            bg_prefetch_handle: None,
            esc_hint_until: None,
            tabs,
            active_tab:    0,
            command_buf:   String::new(),
        }
    }

    pub async fn init(&mut self) {
        let idx = self.cursor;
        self.fetch_and_play(idx).await;
        self.restart_background_prefetch();
    }

    // ── Normal mode ───────────────────────────────────────────────────────────

    /// 折りたたみ状態を考慮した表示行インデックスのリストを返す。
    pub fn visible_line_indices(&self) -> Vec<usize> {
        if self.folded {
            (0..self.lines.len())
                .filter(|&i| !self.lines[i].starts_with(' '))
                .collect()
        } else {
            (0..self.lines.len()).collect()
        }
    }

    /// 表示行リスト内でのカーソル位置を返す（非表示行の場合は最近傍の表示行位置）。
    pub fn vis_cursor_pos(&self) -> usize {
        nearest_vis_pos(self.cursor, &self.visible_line_indices())
    }

    /// 折りたたみ時にカーソルが非表示行にある場合、最も近い表示行に移動する。
    fn normalize_cursor_for_fold(&mut self) {
        if !self.folded { return; }
        let visible = self.visible_line_indices();
        if visible.is_empty() || visible.contains(&self.cursor) { return; }
        if let Some(&c) = visible.get(nearest_vis_pos(self.cursor, &visible)) {
            self.cursor = c;
        }
    }

    /// すべての pending プレフィックスフラグをリセットする。
    /// キーハンドラおよびアクションメソッドの冒頭で呼ぶ共通ヘルパー。
    pub fn reset_pending_prefixes(&mut self) {
        self.pending_d = false;
        self.pending_z = false;
        self.pending_g = false;
        self.pending_quote = false;
        self.pending_clipboard = false;
    }

    pub async fn move_cursor(&mut self, delta: i32) {
        self.reset_pending_prefixes();
        if self.lines.is_empty() { return; }
        let next = if self.folded {
            let visible = self.visible_line_indices();
            if visible.is_empty() { return; }
            // カーソルが非表示行にある場合は最も近い表示行の位置から動かす
            let vis_pos = nearest_vis_pos(self.cursor, &visible);
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
            self.cursor = 0;
            return;
        }
        self.lines.remove(self.cursor);
        if self.cursor >= self.lines.len() { self.cursor = self.lines.len() - 1; }
        self.fetch_and_play(self.cursor).await;
        self.restart_background_prefetch();
    }

    pub async fn paste_below(&mut self) {
        self.reset_pending_prefixes();
        let text = match &self.yank_buf { Some(t) => t.clone(), None => return };
        self.lines.insert(self.cursor + 1, text);
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
        let tail = self.lines.split_off(insert_pos);
        self.lines.extend(clip_lines);
        self.lines.extend(tail);
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
        let tail = self.lines.split_off(self.cursor);
        self.lines.extend(clip_lines);
        self.lines.extend(tail);
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

    // ── Insert mode ───────────────────────────────────────────────────────────

    /// i: 現在行を編集。現在行が空なら1つ上の行の末尾コンテキストを継承する。
    pub fn enter_insert_current(&mut self) {
        self.reset_pending_prefixes();
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
        self.reset_pending_prefixes();
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
        self.reset_pending_prefixes();
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

    /// 確定: [N]展開 → 行中途のspeaker/style変化で行分割 → lines更新 → Normalへ → 再生
    pub async fn commit_insert(&mut self) {
        self.status_msg = String::from("ready");
        self.commit_lines().await;
        self.mode       = Mode::Normal;
    }

    /// ENTERで確定: 現在行を確定し、下に空行を挿入してINSERTモードで編集開始（vim の o 相当）
    pub async fn commit_and_insert_below(&mut self) {
        self.commit_lines().await;
        self.enter_insert_below();
    }

    /// INSERTモードのバッファをlinesに書き戻し、再生・prefetchを行う内部ヘルパー。
    /// modeは変更しない。
    async fn commit_lines(&mut self) {
        let raw  = self.textarea.lines().first().cloned().unwrap_or_default();
        let text = tag::expand_id_tags(&raw);
        let split_lines = tag::split_by_ctx_change(&text);
        if self.cursor < self.lines.len() {
            // split_by_ctx_change は常に1要素以上を返す
            self.lines[self.cursor] = split_lines.first().cloned().unwrap_or_default();
            for (i, extra_line) in split_lines[1..].iter().enumerate() {
                self.lines.insert(self.cursor + 1 + i, extra_line.clone());
            }
        }
        self.lines = compress_trailing_empty(std::mem::take(&mut self.lines));
        if self.cursor >= self.lines.len() {
            self.cursor = self.lines.len().saturating_sub(1);
        }
        self.fetch_and_play(self.cursor).await;
        self.restart_background_prefetch();
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
        } else if self.mode == Mode::UpdateAvailableDialog || self.mode == Mode::QuitWithUpdateDialog {
            "[update available]"
        } else {
            &self.status_msg
        }
    }

    // ── タブ操作 ───────────────────────────────────────────────────────────────

    /// アクティブタブの現在状態をtabsスロットにswapで書き込む内部ヘルパー。
    /// クローンを避けるため、self.linesとtabs[active_tab].0を入れ替える。
    /// 呼び出し後、tabs[active_tab].0には正しいlinesが、self.linesには古いスロット値が入る。
    fn save_current_tab(&mut self) {
        if let Some((tab_lines, tab_cursor, tab_folded)) = self.tabs.get_mut(self.active_tab) {
            std::mem::swap(&mut self.lines, tab_lines);
            *tab_cursor  = self.cursor;
            *tab_folded  = self.folded;
        }
    }

    /// :tabnew: 新しい空タブを作成してそこに移動する。
    pub fn tabnew(&mut self) {
        self.reset_pending_prefixes();
        self.save_current_tab();
        // 新タブ用の空エントリを追加し、アクティブにする
        self.tabs.push((vec![], 0, false));
        self.active_tab = self.tabs.len() - 1;
        self.lines  = vec![String::new()];
        self.cursor = 0;
        self.folded = false;
        self.restart_background_prefetch();
    }

    /// gt: 次のタブに移動する（最後のタブなら最初に戻る）。
    pub fn tab_next(&mut self) {
        self.reset_pending_prefixes();
        if self.tabs.len() <= 1 { return; }
        // 現在タブをswapで保存
        self.save_current_tab();
        // 次タブのlinesをmem::takeで取り出してself.linesに設定
        self.active_tab = (self.active_tab + 1) % self.tabs.len();
        self.lines  = std::mem::take(&mut self.tabs[self.active_tab].0);
        self.cursor = self.tabs[self.active_tab].1;
        self.folded = self.tabs[self.active_tab].2;
        // 折りたたみ状態を復元した場合、カーソルが非表示行にある可能性を修正
        self.normalize_cursor_for_fold();
        self.restart_background_prefetch();
    }

    /// gT: 前のタブに移動する（最初のタブなら最後に移動する）。
    pub fn tab_prev(&mut self) {
        self.reset_pending_prefixes();
        if self.tabs.len() <= 1 { return; }
        // 現在タブをswapで保存
        self.save_current_tab();
        // 前タブのlinesをmem::takeで取り出してself.linesに設定
        self.active_tab = if self.active_tab == 0 { self.tabs.len() - 1 } else { self.active_tab - 1 };
        self.lines  = std::mem::take(&mut self.tabs[self.active_tab].0);
        self.cursor = self.tabs[self.active_tab].1;
        self.folded = self.tabs[self.active_tab].2;
        // 折りたたみ状態を復元した場合、カーソルが非表示行にある可能性を修正
        self.normalize_cursor_for_fold();
        self.restart_background_prefetch();
    }

    /// コマンドモードのバッファに入力された文字列を解釈して実行する。
    pub async fn execute_command(&mut self) {
        let cmd = self.command_buf.trim().to_string();
        match cmd.as_str() {
            "tabnew" => self.tabnew(),
            _ => {
                // 未知のコマンドはステータスメッセージで明示的に知らせる。
                self.status_msg = format!("Unknown command: {}", cmd);
            }
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

    /// 現在行のfetch完了後、表示範囲内のcacheのない行を裏で1行ずつfetchする。
    /// 前回のタスクがあればキャンセルしてから新たに起動する。
    fn restart_background_prefetch(&mut self) {
        if let Some(h) = self.bg_prefetch_handle.take() {
            h.abort();
        }
        let cursor_text = self.lines.get(self.cursor).cloned().unwrap_or_default();
        // 折りたたみ時は表示行のみをprefetch対象とする
        let target_texts: Vec<String> = if self.folded {
            let visible_indices = self.visible_line_indices();
            let visible_texts: Vec<String> = visible_indices.iter().map(|&i| self.lines[i].clone()).collect();
            let vis_cursor = nearest_vis_pos(self.cursor, &visible_indices);
            background_prefetch::compute_prefetch_targets(vis_cursor, self.visible_lines, &visible_texts)
                .into_iter()
                .map(|idx| visible_texts[idx].clone())
                .collect()
        } else {
            // 全行ではなく表示ウィンドウ内の対象行のみをcloneして渡す
            background_prefetch::compute_prefetch_targets(
                self.cursor, self.visible_lines, &self.lines,
            )
            .into_iter()
            .map(|idx| self.lines[idx].clone())
            .collect()
        };
        self.bg_prefetch_handle = Some(background_prefetch::spawn_background_prefetch(
            cursor_text,
            target_texts,
            Arc::clone(&self.cache),
            Arc::clone(&self.is_fetching),
            self.fetch_tx.clone(),
        ));
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

/// 表示行インデックスリスト内で `cursor`（実行インデックス）に最も近い位置を返す。
/// `cursor` が `visible` に含まれる場合はその位置、含まれない場合は距離が最小の位置を返す。
fn nearest_vis_pos(cursor: usize, visible: &[usize]) -> usize {
    visible.iter()
        .position(|&i| i == cursor)
        .unwrap_or_else(|| {
            visible.iter()
                .enumerate()
                .min_by_key(|(_, &i)| {
                    let diff = i as isize - cursor as isize;
                    diff.unsigned_abs()
                })
                .map(|(idx, _)| idx)
                .unwrap_or(0)
        })
}
