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
    SpeakerStyleMode,
    IntonationMode,
    TabNext,
    TabPrev,
    TabNew,
    Quit,
}

/// ヘルプメニューの1エントリ（表示テキストと実行アクションをひとまとめに管理）
pub struct HelpEntry {
    pub key: &'static str,
    /// ヘルプモードでのキー入力照合に使う正規キー文字列。
    /// 空文字列の場合は照合対象外（hjkl・カーソルキー相当で無効）。
    pub canonical_key: &'static str,
    pub desc: &'static str,
    pub action: HelpAction,
}

/// NORMALモードのkeybind一覧（helpメニュー表示・実行用）。
/// `action` を各エントリに直接持たせることで、並び替え・追加・削除しても
/// 表示内容と実行アクションがズレない。
/// 偶数インデックス＝左列（ナビゲーション／再生系を優先）、奇数インデックス＝右列（編集：挿入／削除／貼り付け等）。
/// `canonical_key` が空文字列のエントリはヘルプモードのキー入力では選択できない
/// （hjkl・カーソルキー相当、または複合コマンドモード操作）。
pub const HELP_ENTRIES: &[HelpEntry] = &[
    HelpEntry {
        key: "j / ↓",
        canonical_key: "",
        desc: "カーソル下移動",
        action: HelpAction::MoveDown,
    },
    HelpEntry {
        key: "i",
        canonical_key: "i",
        desc: "現在行を編集（挿入モード）",
        action: HelpAction::EditCurrent,
    },
    HelpEntry {
        key: "k / ↑",
        canonical_key: "",
        desc: "カーソル上移動",
        action: HelpAction::MoveUp,
    },
    HelpEntry {
        key: "O",
        canonical_key: "O",
        desc: "上に新行を挿入して編集",
        action: HelpAction::InsertAbove,
    },
    HelpEntry {
        key: "zm",
        canonical_key: "zm",
        desc: "折りたたむ（行頭space行を非表示）",
        action: HelpAction::Fold,
    },
    HelpEntry {
        key: "o",
        canonical_key: "o",
        desc: "下に新行を挿入して編集",
        action: HelpAction::InsertBelow,
    },
    HelpEntry {
        key: "zr",
        canonical_key: "zr",
        desc: "折りたたみを解除",
        action: HelpAction::Unfold,
    },
    HelpEntry {
        key: "dd",
        canonical_key: "dd",
        desc: "現在行を削除",
        action: HelpAction::DeleteLine,
    },
    HelpEntry {
        key: "l / gt",
        canonical_key: "gt",
        desc: "次のタブへ移動",
        action: HelpAction::TabNext,
    },
    HelpEntry {
        key: "P",
        canonical_key: "P",
        desc: "ヤンクバッファを上にペースト",
        action: HelpAction::PasteAbove,
    },
    HelpEntry {
        key: "gT",
        canonical_key: "gT",
        desc: "前のタブへ移動",
        action: HelpAction::TabPrev,
    },
    HelpEntry {
        key: "p",
        canonical_key: "p",
        desc: "ヤンクバッファを下にペースト",
        action: HelpAction::PasteBelow,
    },
    HelpEntry {
        key: "Space",
        canonical_key: " ",
        desc: "現在行を再生",
        action: HelpAction::PlayCurrent,
    },
    HelpEntry {
        key: ":tabnew",
        canonical_key: ":tabnew",
        desc: "新しいタブを作成",
        action: HelpAction::TabNew,
    },
    HelpEntry {
        key: "Enter",
        canonical_key: "",
        desc: "下へ移動（j と同じ）",
        action: HelpAction::None,
    },
    HelpEntry {
        key: "v",
        canonical_key: "v",
        desc: "イントネーション編集モードへ",
        action: HelpAction::IntonationMode,
    },
    HelpEntry {
        key: "s",
        canonical_key: "s",
        desc: "speaker/styleを変更",
        action: HelpAction::SpeakerStyleMode,
    },
    HelpEntry {
        key: "\"+P",
        canonical_key: "\"+P",
        desc: "クリップボードを上にペースト",
        action: HelpAction::PasteAboveClipboard,
    },
    HelpEntry {
        key: "n j/k",
        canonical_key: "",
        desc: "n行分移動（例: 5j）",
        action: HelpAction::None,
    },
    HelpEntry {
        key: "?",
        canonical_key: "",
        desc: "ヘルプメニューを開く",
        action: HelpAction::None,
    },
    HelpEntry {
        key: "q",
        canonical_key: "q",
        desc: "終了",
        action: HelpAction::Quit,
    },
    HelpEntry {
        key: "\"+p",
        canonical_key: "\"+p",
        desc: "クリップボードを下にペースト",
        action: HelpAction::PasteBelowClipboard,
    },
];

impl App {
    /// ?: ヘルプメニューを開く。
    pub fn enter_help_mode(&mut self) {
        self.reset_pending_prefixes();
        self.help_key_buf.clear();
        self.mode = Mode::Help;
    }

    /// ヘルプモードでキー文字列をバッファに追記し、完全一致するエントリのアクションを返す。
    /// - 完全一致した場合: Normalモードに戻り `Some(action)` を返す（バッファはクリア）。
    /// - 前方一致あり: バッファを保持したまま `None` を返す（複数ハイライト表示用）。
    /// - 前方一致なし: バッファをクリアして `None` を返す。
    pub fn help_append_key(&mut self, s: &str) -> Option<HelpAction> {
        self.help_key_buf.push_str(s);
        // 1パスで完全一致と前方一致を同時に判定する
        let mut exact_action: Option<HelpAction> = None;
        let mut has_prefix = false;
        for e in HELP_ENTRIES {
            if e.canonical_key.is_empty() {
                continue;
            }
            if e.canonical_key == self.help_key_buf.as_str() {
                exact_action = Some(e.action.clone());
                break; // 完全一致が見つかったら前方一致の走査は不要
            }
            if e.canonical_key.starts_with(self.help_key_buf.as_str()) {
                has_prefix = true;
            }
        }
        if let Some(action) = exact_action {
            self.help_key_buf.clear();
            self.mode = Mode::Normal;
            return Some(action);
        }
        // 前方一致がなければバッファをリセット
        if !has_prefix {
            self.help_key_buf.clear();
        }
        None
    }

    /// help_key_buf の内容と前方一致する（canonical_key が空でない）エントリのインデックスを返す。
    /// バッファが空の場合は空ベクタを返す。
    pub fn help_matching_indices(&self) -> Vec<usize> {
        if self.help_key_buf.is_empty() {
            return vec![];
        }
        HELP_ENTRIES
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                !e.canonical_key.is_empty()
                    && e.canonical_key.starts_with(self.help_key_buf.as_str())
            })
            .map(|(i, _)| i)
            .collect()
    }
}

#[cfg(test)]
#[path = "../tests/app/help.rs"]
mod tests;
