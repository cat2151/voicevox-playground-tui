//! タブ操作。

use super::App;

impl App {
    /// アクティブタブの現在状態をtabsスロットにswapで書き込む内部ヘルパー。
    /// クローンを避けるため、self.linesとtabs[active_tab].0を入れ替える。
    /// 呼び出し後、tabs[active_tab].0には正しいlinesが、self.linesには古いスロット値が入る。
    fn save_current_tab(&mut self) {
        if let Some((tab_lines, tab_intonations, tab_cursor, tab_folded)) = self.tabs.get_mut(self.active_tab) {
            std::mem::swap(&mut self.lines, tab_lines);
            std::mem::swap(&mut self.line_intonations, tab_intonations);
            *tab_cursor  = self.cursor;
            *tab_folded  = self.folded;
        }
    }

    /// :tabnew: 新しい空タブを作成してそこに移動する。
    pub fn tabnew(&mut self) {
        self.reset_pending_prefixes();
        self.save_current_tab();
        // 新タブ用の空エントリを追加し、アクティブにする
        self.tabs.push((vec![], vec![], 0, false));
        self.active_tab = self.tabs.len() - 1;
        self.lines  = vec![String::new()];
        self.line_intonations = vec![None];
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
        self.line_intonations = std::mem::take(&mut self.tabs[self.active_tab].1);
        self.cursor = self.tabs[self.active_tab].2;
        self.folded = self.tabs[self.active_tab].3;
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
        self.line_intonations = std::mem::take(&mut self.tabs[self.active_tab].1);
        self.cursor = self.tabs[self.active_tab].2;
        self.folded = self.tabs[self.active_tab].3;
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
}
