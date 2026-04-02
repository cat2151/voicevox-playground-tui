//! タブ操作。

use super::App;

impl App {
    /// 全タブのlines（終了時保存用）を返す。
    /// アクティブタブのlinesはself.linesから、他のタブはtabsスロットから取得する。
    /// self.tabsは全タブ（アクティブ含む）1エントリを持つため、self.tabs.len()が正しいサイズとなる。
    pub fn all_tab_lines(&self) -> Vec<Vec<String>> {
        let mut result = vec![Vec::new(); self.tabs.len()];
        for (i, (tab_lines, _, _, _)) in self.tabs.iter().enumerate() {
            if i == self.active_tab {
                result[i] = self.lines.clone();
            } else {
                result[i] = tab_lines.clone();
            }
        }
        result
    }

    /// 全タブのline_intonations（終了時保存用）を返す。
    /// アクティブタブのline_intonationsはself.line_intonationsから、他のタブはtabsスロットから取得する。
    pub fn all_tab_intonations(&self) -> Vec<Vec<Option<super::IntonationLineData>>> {
        let mut result = vec![Vec::new(); self.tabs.len()];
        for (i, (_, tab_intonations, _, _)) in self.tabs.iter().enumerate() {
            if i == self.active_tab {
                result[i] = self.line_intonations.clone();
            } else {
                result[i] = tab_intonations.clone();
            }
        }
        result
    }

    /// アクティブタブの現在状態をtabsスロットにswapで書き込む内部ヘルパー。
    /// クローンを避けるため、self.linesとtabs[active_tab].0、self.line_intonationsとtabs[active_tab].1を入れ替える。
    /// 呼び出し後、tabs[active_tab].0/1には正しいlines/line_intonationsが、self.lines/self.line_intonationsには古いスロット値が入る。
    fn save_current_tab(&mut self) {
        if let Some((tab_lines, tab_intonations, tab_cursor, tab_folded)) =
            self.tabs.get_mut(self.active_tab)
        {
            std::mem::swap(&mut self.lines, tab_lines);
            std::mem::swap(&mut self.line_intonations, tab_intonations);
            *tab_cursor = self.cursor;
            *tab_folded = self.folded;
        }
    }

    /// :tabnew: 新しい空タブを作成してそこに移動する。
    pub fn tabnew(&mut self) {
        self.reset_pending_prefixes();
        self.save_current_tab();
        // 新タブ用の空エントリを追加し、アクティブにする
        self.tabs.push((vec![], vec![], 0, false));
        self.active_tab = self.tabs.len() - 1;
        self.lines = vec![String::new()];
        self.line_intonations = vec![None];
        self.cursor = 0;
        self.folded = false;
        self.restart_background_prefetch();
    }

    /// gt: 次のタブに移動する（最後のタブなら最初に戻る）。
    pub fn tab_next(&mut self) {
        self.reset_pending_prefixes();
        if self.tabs.len() <= 1 {
            return;
        }
        // 現在タブをswapで保存
        self.save_current_tab();
        // 次タブのlinesをmem::takeで取り出してself.linesに設定
        self.active_tab = (self.active_tab + 1) % self.tabs.len();
        self.lines = std::mem::take(&mut self.tabs[self.active_tab].0);
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
        if self.tabs.len() <= 1 {
            return;
        }
        // 現在タブをswapで保存
        self.save_current_tab();
        // 前タブのlinesをmem::takeで取り出してself.linesに設定
        self.active_tab = if self.active_tab == 0 {
            self.tabs.len() - 1
        } else {
            self.active_tab - 1
        };
        self.lines = std::mem::take(&mut self.tabs[self.active_tab].0);
        self.line_intonations = std::mem::take(&mut self.tabs[self.active_tab].1);
        self.cursor = self.tabs[self.active_tab].2;
        self.folded = self.tabs[self.active_tab].3;
        // 折りたたみ状態を復元した場合、カーソルが非表示行にある可能性を修正
        self.normalize_cursor_for_fold();
        self.restart_background_prefetch();
    }

    /// 指定インデックスのタブに直接移動する（インデックスが範囲外の場合は何もしない）。
    pub fn switch_to_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        if index == self.active_tab {
            return;
        }
        self.save_current_tab();
        self.active_tab = index;
        self.lines = std::mem::take(&mut self.tabs[self.active_tab].0);
        self.line_intonations = std::mem::take(&mut self.tabs[self.active_tab].1);
        self.cursor = self.tabs[self.active_tab].2;
        self.folded = self.tabs[self.active_tab].3;
        self.normalize_cursor_for_fold();
        self.restart_background_prefetch();
    }

    /// 現在のアプリ状態からセッション状態を収集して返す。
    /// 各タブのカーソル位置・折りたたみ状態をすべて含む。
    pub fn collect_session_state(&self) -> crate::history::SessionState {
        let num_tabs = self.tabs.len();
        let mut tab_states = Vec::with_capacity(num_tabs);
        for i in 0..num_tabs {
            let (cursor, folded) = if i == self.active_tab {
                (self.cursor, self.folded)
            } else {
                (self.tabs[i].2, self.tabs[i].3)
            };
            tab_states.push(crate::history::TabSessionState { cursor, folded });
        }
        crate::history::SessionState {
            active_tab: self.active_tab,
            tabs: tab_states,
        }
    }

    /// 保存済みセッション状態をアプリに適用する。
    /// 各タブのカーソル位置・折りたたみ状態を復元し、アクティブタブに切り替える。
    pub fn restore_session_state(&mut self, state: &crate::history::SessionState) {
        let num_tabs = self.tabs.len();
        // まずタブ0（現在アクティブ）のカーソル/折りたたみを適用する
        if let Some(tab_state) = state.tabs.first() {
            let max_cursor = self.lines.len().saturating_sub(1);
            self.cursor = tab_state.cursor.min(max_cursor);
            self.folded = tab_state.folded;
            self.normalize_cursor_for_fold();
        }
        // タブ1以降の状態を適用する
        for (i, tab_state) in state.tabs.iter().enumerate().skip(1) {
            if i >= num_tabs {
                break;
            }
            let max_cursor = self.tabs[i].0.len().saturating_sub(1);
            self.tabs[i].2 = tab_state.cursor.min(max_cursor);
            self.tabs[i].3 = tab_state.folded;
        }
        // 保存済みアクティブタブに切り替える（範囲外はクランプ）
        if num_tabs > 0 {
            let target = state.active_tab.min(num_tabs - 1);
            if target != 0 {
                self.switch_to_tab(target);
            }
        }
    }

    /// 起動後にバックグラウンドで読み込まれた履歴・セッション状態を適用する。
    pub fn apply_loaded_history(
        &mut self,
        all_lines: super::AllTabLines,
        all_intonations: super::AllTabIntonations,
        session_state: &crate::history::SessionState,
    ) {
        if let Some(handle) = self.bg_prefetch_handle.take() {
            handle.abort();
        }
        if let Some(handle) = self.intonation_play_handle.take() {
            handle.abort();
        }

        let mut all_lines = all_lines;
        if all_lines.is_empty() {
            all_lines.push(vec![String::new()]);
        }
        let mut all_intonations = all_intonations;

        self.lines = super::utils::compress_trailing_empty(all_lines.remove(0));
        let first_intonations = if all_intonations.is_empty() {
            None
        } else {
            Some(all_intonations.remove(0))
        };
        self.line_intonations = normalize_loaded_intonations(self.lines.len(), first_intonations);
        self.cursor = self.lines.len().saturating_sub(1);
        self.folded = false;
        self.tabs = vec![(vec![], vec![], 0usize, false)];
        self.active_tab = 0;
        self.mode = super::Mode::Normal;
        self.textarea = tui_textarea::TextArea::default();
        self.reset_pending_prefixes();
        self.command_buf.clear();
        self.help_key_buf.clear();
        self.intonation_speaker_id = 0;
        self.intonation_mora_texts.clear();
        self.intonation_pitches.clear();
        self.intonation_initial_pitches.clear();
        self.intonation_query = serde_json::Value::Null;
        self.intonation_cursor = 0;
        self.intonation_num_buf.clear();
        self.intonation_debounce = None;
        self.intonation_graph_x = 0;
        self.intonation_graph_y = 0;
        self.intonation_graph_h = 0;
        self.intonation_graph_pitch_top = 0.0;
        self.intonation_mora_col_x.clear();
        self.intonation_mora_col_w.clear();
        self.esc_hint_until = None;

        for (i, extra_lines) in all_lines.into_iter().enumerate() {
            let extra_lines = super::utils::compress_trailing_empty(extra_lines);
            let extra_intonations =
                normalize_loaded_intonations(extra_lines.len(), all_intonations.get(i).cloned());
            let extra_cursor = extra_lines.len().saturating_sub(1);
            self.tabs
                .push((extra_lines, extra_intonations, extra_cursor, false));
        }

        self.restore_session_state(session_state);
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

fn normalize_loaded_intonations(
    line_len: usize,
    loaded: Option<super::LineIntonations>,
) -> super::LineIntonations {
    let mut normalized = vec![None; line_len];
    if let Some(loaded) = loaded {
        for (i, slot) in loaded.into_iter().enumerate() {
            if i < normalized.len() {
                normalized[i] = slot;
            }
        }
    }
    normalized
}

#[cfg(test)]
#[path = "../tests/app/tab_ops.rs"]
mod tests;
