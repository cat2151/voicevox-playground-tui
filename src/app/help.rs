//! ヘルプメニューのエントリとアクション定義。

use super::{App, Mode};

/// ヘルプメニューの1エントリ
pub struct HelpEntry {
    pub key:  &'static str,
    pub desc: &'static str,
}

/// NORMALモードのkeybind一覧（helpメニュー表示・実行用）
pub const HELP_ENTRIES: &[HelpEntry] = &[
    HelpEntry { key: "j / ↓",      desc: "カーソル下移動" },
    HelpEntry { key: "k / ↑",      desc: "カーソル上移動" },
    HelpEntry { key: "i",           desc: "現在行を編集（挿入モード）" },
    HelpEntry { key: "o",           desc: "下に新行を挿入して編集" },
    HelpEntry { key: "O",           desc: "上に新行を挿入して編集" },
    HelpEntry { key: "Space/Enter", desc: "現在行を再生" },
    HelpEntry { key: "dd",          desc: "現在行を削除" },
    HelpEntry { key: "p",           desc: "ヤンクバッファを下にペースト" },
    HelpEntry { key: "P",           desc: "ヤンクバッファを上にペースト" },
    HelpEntry { key: "\"+p",        desc: "クリップボードを下にペースト" },
    HelpEntry { key: "\"+P",        desc: "クリップボードを上にペースト" },
    HelpEntry { key: "zm",          desc: "折りたたむ（行頭space行を非表示）" },
    HelpEntry { key: "zr",          desc: "折りたたみを解除" },
    HelpEntry { key: "v",           desc: "イントネーション編集モードへ" },
    HelpEntry { key: "l / gt",      desc: "次のタブへ移動" },
    HelpEntry { key: "gT",          desc: "前のタブへ移動" },
    HelpEntry { key: ":tabnew",     desc: "新しいタブを作成" },
    HelpEntry { key: "h",           desc: "このヘルプを表示" },
    HelpEntry { key: "q",           desc: "終了" },
];

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

impl App {
    /// h: ヘルプメニューを開く。
    pub fn enter_help_mode(&mut self) {
        self.reset_pending_prefixes();
        self.help_cursor = 0;
        self.mode = Mode::Help;
    }

    /// ヘルプメニュー内の選択エントリに対応するアクションを返し、Normalモードに戻る。
    pub fn help_select(&mut self) -> HelpAction {
        self.mode = Mode::Normal;
        match self.help_cursor {
            0  => HelpAction::MoveDown,
            1  => HelpAction::MoveUp,
            2  => HelpAction::EditCurrent,
            3  => HelpAction::InsertBelow,
            4  => HelpAction::InsertAbove,
            5  => HelpAction::PlayCurrent,
            6  => HelpAction::DeleteLine,
            7  => HelpAction::PasteBelow,
            8  => HelpAction::PasteAbove,
            9  => HelpAction::PasteBelowClipboard,
            10 => HelpAction::PasteAboveClipboard,
            11 => HelpAction::Fold,
            12 => HelpAction::Unfold,
            13 => HelpAction::IntonationMode,
            14 => HelpAction::TabNext,
            15 => HelpAction::TabPrev,
            16 => HelpAction::TabNew,
            17 => HelpAction::None,   // h: ヘルプ表示（メニューを閉じるだけ）
            18 => HelpAction::Quit,
            _  => HelpAction::None,
        }
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
