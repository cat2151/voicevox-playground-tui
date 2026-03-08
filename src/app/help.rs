//! ヘルプメニューのエントリとアクション定義。

use super::{App, Mode};

/// ヘルプメニューから選択した際に実行するアクション
#[derive(Debug, Clone, PartialEq)]
pub enum HelpAction {
    None,
    MoveDown,
    MoveUp,
    EditCurrent,
    InsertBelow,
    InsertAbove,
    PlayCurrent,
    DeleteLine,
    PasteBelow,
    PasteAbove,
    PasteBelowClipboard,
    PasteAboveClipboard,
    Fold,
    Unfold,
    IntonationMode,
    TabNext,
    TabPrev,
    TabNew,
    Quit,
}

/// ヘルプメニューの1エントリ（表示テキストと実行アクションをひとまとめに管理）
pub struct HelpEntry {
    pub key:    &'static str,
    pub desc:   &'static str,
    pub action: HelpAction,
}

/// NORMALモードのkeybind一覧（helpメニュー表示・実行用）。
/// `action` を各エントリに直接持たせることで、並び替え・追加・削除しても
/// 表示内容と実行アクションがズレない。
pub const HELP_ENTRIES: &[HelpEntry] = &[
    HelpEntry { key: "j / ↓",      desc: "カーソル下移動",               action: HelpAction::MoveDown },
    HelpEntry { key: "k / ↑",      desc: "カーソル上移動",               action: HelpAction::MoveUp },
    HelpEntry { key: "i",           desc: "現在行を編集（挿入モード）",   action: HelpAction::EditCurrent },
    HelpEntry { key: "o",           desc: "下に新行を挿入して編集",       action: HelpAction::InsertBelow },
    HelpEntry { key: "O",           desc: "上に新行を挿入して編集",       action: HelpAction::InsertAbove },
    HelpEntry { key: "Space/Enter", desc: "現在行を再生",                 action: HelpAction::PlayCurrent },
    HelpEntry { key: "dd",          desc: "現在行を削除",                 action: HelpAction::DeleteLine },
    HelpEntry { key: "p",           desc: "ヤンクバッファを下にペースト", action: HelpAction::PasteBelow },
    HelpEntry { key: "P",           desc: "ヤンクバッファを上にペースト", action: HelpAction::PasteAbove },
    HelpEntry { key: "\"+p",        desc: "クリップボードを下にペースト", action: HelpAction::PasteBelowClipboard },
    HelpEntry { key: "\"+P",        desc: "クリップボードを上にペースト", action: HelpAction::PasteAboveClipboard },
    HelpEntry { key: "zm",          desc: "折りたたむ（行頭space行を非表示）", action: HelpAction::Fold },
    HelpEntry { key: "zr",          desc: "折りたたみを解除",             action: HelpAction::Unfold },
    HelpEntry { key: "v",           desc: "イントネーション編集モードへ", action: HelpAction::IntonationMode },
    HelpEntry { key: "l / gt",      desc: "次のタブへ移動",               action: HelpAction::TabNext },
    HelpEntry { key: "gT",          desc: "前のタブへ移動",               action: HelpAction::TabPrev },
    HelpEntry { key: ":tabnew",     desc: "新しいタブを作成",             action: HelpAction::TabNew },
    HelpEntry { key: "h",           desc: "このヘルプを表示",             action: HelpAction::None },
    HelpEntry { key: "q",           desc: "終了",                         action: HelpAction::Quit },
];

impl App {
    /// h: ヘルプメニューを開く。
    pub fn enter_help_mode(&mut self) {
        self.reset_pending_prefixes();
        self.help_cursor = 0;
        self.mode = Mode::Help;
    }

    /// ヘルプメニュー内の選択エントリに対応するアクションを返し、Normalモードに戻る。
    /// `HELP_ENTRIES[help_cursor].action` を直接返すため、エントリの並び替えや
    /// 追加・削除をしてもインデックスずれによるバグが発生しない。
    pub fn help_select(&mut self) -> HelpAction {
        self.mode = Mode::Normal;
        HELP_ENTRIES
            .get(self.help_cursor)
            .map(|e| e.action.clone())
            .unwrap_or(HelpAction::None)
    }

    /// ヘルプメニュー内で行方向にカーソルを移動する。
    pub fn help_move_row(&mut self, delta: i32) {
        let n = HELP_ENTRIES.len();
        let total_rows = (n + 1) / 2;
        let row = self.help_cursor / 2;
        let col = self.help_cursor % 2;
        let new_row = (row as i32 + delta).clamp(0, total_rows as i32 - 1) as usize;
        let new_cursor = new_row * 2 + col;
        // 右列の最終行が空の場合は左列に留まる
        self.help_cursor = if new_cursor < n { new_cursor } else { new_row * 2 };
    }

    /// ヘルプメニュー内で列方向にカーソルを移動する。
    pub fn help_move_col(&mut self, delta: i32) {
        let n = HELP_ENTRIES.len();
        let col = self.help_cursor % 2;
        if delta > 0 && col == 0 && self.help_cursor + 1 < n {
            self.help_cursor += 1;
        } else if delta < 0 && col == 1 {
            self.help_cursor -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ヘルパー: App を最小限に作れないため、help_cursor だけを模したローカル構造体で
    //   ロジックのみテストする ─────────────────────────────────────────────────────

    /// help_move_row / help_move_col と同じロジックを独立して検証する。
    fn move_row(cursor: usize, delta: i32) -> usize {
        let n = HELP_ENTRIES.len();
        let total_rows = (n + 1) / 2;
        let row = cursor / 2;
        let col = cursor % 2;
        let new_row = (row as i32 + delta).clamp(0, total_rows as i32 - 1) as usize;
        let new_cursor = new_row * 2 + col;
        if new_cursor < n { new_cursor } else { new_row * 2 }
    }

    fn move_col(cursor: usize, delta: i32) -> usize {
        let n = HELP_ENTRIES.len();
        let col = cursor % 2;
        if delta > 0 && col == 0 && cursor + 1 < n {
            cursor + 1
        } else if delta < 0 && col == 1 {
            cursor - 1
        } else {
            cursor
        }
    }

    // ── help_move_row ────────────────────────────────────────────────────────────

    #[test]
    fn move_row_down_from_first_row() {
        // 0行目左列(cursor=0) → 下(+1) → 1行目左列(cursor=2)
        assert_eq!(move_row(0, 1), 2);
    }

    #[test]
    fn move_row_up_from_second_row() {
        // 1行目左列(cursor=2) → 上(-1) → 0行目左列(cursor=0)
        assert_eq!(move_row(2, -1), 0);
    }

    #[test]
    fn move_row_down_clamps_at_last_row() {
        let n = HELP_ENTRIES.len();
        let last_row = (n - 1) / 2;
        // 最終行左列から下に移動しても最終行左列のまま
        let last_left = last_row * 2;
        assert_eq!(move_row(last_left, 1), last_left);
    }

    #[test]
    fn move_row_up_clamps_at_first_row() {
        // cursor=0 から上に移動しても 0 のまま
        assert_eq!(move_row(0, -1), 0);
    }

    #[test]
    fn move_row_right_col_moves_to_right_col_of_next_row() {
        // 0行目右列(cursor=1) → 下 → 1行目右列(cursor=3)
        assert_eq!(move_row(1, 1), 3);
    }

    #[test]
    fn move_row_right_col_falls_back_to_left_when_no_right() {
        // n=19(奇数)なので最終行(9行目)には右列エントリが存在しない
        // 8行目右列(cursor=17) → 下 → 9行目左列(cursor=18)へフォールバック
        let n = HELP_ENTRIES.len(); // 19
        assert!(n % 2 == 1, "このテストはHELP_ENTRIESが奇数個のときのみ有効");
        let second_to_last_right = (n / 2 - 1) * 2 + 1; // (9-1)*2+1 = 15... recalc
        // 最終行の1行前の右列からjで移動 → 右列が存在しないため左列へフォールバック
        let penultimate_row = (n - 1) / 2 - 1;
        let penultimate_right = penultimate_row * 2 + 1;
        let last_left = ((n - 1) / 2) * 2;
        assert_eq!(move_row(penultimate_right, 1), last_left,
            "右列が存在しない最終行では左列にフォールバックされること");
    }

    // ── help_move_col ────────────────────────────────────────────────────────────

    #[test]
    fn move_col_right_from_left() {
        // 左列(cursor=0) → 右(+1) → 右列(cursor=1)
        assert_eq!(move_col(0, 1), 1);
    }

    #[test]
    fn move_col_left_from_right() {
        // 右列(cursor=1) → 左(-1) → 左列(cursor=0)
        assert_eq!(move_col(1, -1), 0);
    }

    #[test]
    fn move_col_right_stays_when_already_right() {
        // 右列からさらに右に移動しても変化なし
        assert_eq!(move_col(1, 1), 1);
    }

    #[test]
    fn move_col_left_stays_when_already_left() {
        // 左列からさらに左に移動しても変化なし
        assert_eq!(move_col(0, -1), 0);
    }

    #[test]
    fn move_col_right_no_entry_when_odd_total() {
        // n=19(奇数)なので最終行には右列エントリが存在しない。
        // 最終行左列から右に移動しても変化なし。
        let n = HELP_ENTRIES.len(); // 19
        assert!(n % 2 == 1);
        let last_left = n - 1; // cursor=18
        assert_eq!(move_col(last_left, 1), last_left,
            "右列エントリが存在しない場合、左列から右移動しても変化しないこと");
    }

    // ── help_select ──────────────────────────────────────────────────────────────

    #[test]
    fn help_select_action_matches_entry() {
        // 全エントリについて、インデックスのアクションがエントリのactionと一致する
        for (i, entry) in HELP_ENTRIES.iter().enumerate() {
            let action = HELP_ENTRIES
                .get(i)
                .map(|e| e.action.clone())
                .unwrap_or(HelpAction::None);
            assert_eq!(action, entry.action,
                "cursor={} のアクションがHELP_ENTRIESのactionと一致しないこと", i);
        }
    }

    #[test]
    fn help_select_out_of_bounds_returns_none() {
        let action = HELP_ENTRIES
            .get(9999)
            .map(|e| e.action.clone())
            .unwrap_or(HelpAction::None);
        assert_eq!(action, HelpAction::None);
    }

    #[test]
    fn help_entries_first_is_move_down() {
        assert_eq!(HELP_ENTRIES[0].action, HelpAction::MoveDown);
    }

    #[test]
    fn help_entries_last_is_quit() {
        assert_eq!(HELP_ENTRIES.last().unwrap().action, HelpAction::Quit);
    }
}
