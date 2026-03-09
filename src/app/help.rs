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
    pub key:           &'static str,
    /// ヘルプモードでのキー入力照合に使う正規キー文字列。
    /// 空文字列の場合は照合対象外（hjkl・カーソルキー相当で無効）。
    pub canonical_key: &'static str,
    pub desc:          &'static str,
    pub action:        HelpAction,
}

/// NORMALモードのkeybind一覧（helpメニュー表示・実行用）。
/// `action` を各エントリに直接持たせることで、並び替え・追加・削除しても
/// 表示内容と実行アクションがズレない。
/// 偶数インデックス＝左列（ナビゲーション／再生／モード系）、奇数インデックス＝右列（編集：挿入／削除／貼り付け等）。
/// `canonical_key` が空文字列のエントリはヘルプモードのキー入力では選択できない
/// （hjkl・カーソルキー相当、または複合コマンドモード操作）。
pub const HELP_ENTRIES: &[HelpEntry] = &[
    HelpEntry { key: "j / ↓",      canonical_key: "",      desc: "カーソル下移動",               action: HelpAction::MoveDown },
    HelpEntry { key: "i",           canonical_key: "i",     desc: "現在行を編集（挿入モード）",   action: HelpAction::EditCurrent },
    HelpEntry { key: "k / ↑",      canonical_key: "",      desc: "カーソル上移動",               action: HelpAction::MoveUp },
    HelpEntry { key: "O",           canonical_key: "O",     desc: "上に新行を挿入して編集",       action: HelpAction::InsertAbove },
    HelpEntry { key: "zm",          canonical_key: "zm",    desc: "折りたたむ（行頭space行を非表示）", action: HelpAction::Fold },
    HelpEntry { key: "o",           canonical_key: "o",     desc: "下に新行を挿入して編集",       action: HelpAction::InsertBelow },
    HelpEntry { key: "zr",          canonical_key: "zr",    desc: "折りたたみを解除",             action: HelpAction::Unfold },
    HelpEntry { key: "dd",          canonical_key: "dd",    desc: "現在行を削除",                 action: HelpAction::DeleteLine },
    HelpEntry { key: "l / gt",      canonical_key: "gt",    desc: "次のタブへ移動",               action: HelpAction::TabNext },
    HelpEntry { key: "P",           canonical_key: "P",     desc: "ヤンクバッファを上にペースト", action: HelpAction::PasteAbove },
    HelpEntry { key: "gT",          canonical_key: "gT",    desc: "前のタブへ移動",               action: HelpAction::TabPrev },
    HelpEntry { key: "p",           canonical_key: "p",     desc: "ヤンクバッファを下にペースト", action: HelpAction::PasteBelow },
    HelpEntry { key: "Space/Enter", canonical_key: " ",     desc: "現在行を再生",                 action: HelpAction::PlayCurrent },
    HelpEntry { key: ":tabnew",     canonical_key: ":tabnew", desc: "新しいタブを作成",             action: HelpAction::TabNew },
    HelpEntry { key: "v",           canonical_key: "v",     desc: "イントネーション編集モードへ", action: HelpAction::IntonationMode },
    HelpEntry { key: "\"+P",        canonical_key: "\"+P",  desc: "クリップボードを上にペースト", action: HelpAction::PasteAboveClipboard },
    HelpEntry { key: "q",           canonical_key: "q",     desc: "終了",                         action: HelpAction::Quit },
    HelpEntry { key: "\"+p",        canonical_key: "\"+p",  desc: "クリップボードを下にペースト", action: HelpAction::PasteBelowClipboard },
];

impl App {
    /// h: ヘルプメニューを開く。
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
            if e.canonical_key.is_empty() { continue; }
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
        HELP_ENTRIES.iter().enumerate()
            .filter(|(_, e)| !e.canonical_key.is_empty() && e.canonical_key.starts_with(self.help_key_buf.as_str()))
            .map(|(i, _)| i)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ヘルパー: App を最小限に作れないため、help_key_buf を模したローカル関数で
    //   ロジックのみテストする ─────────────────────────────────────────────────────

    /// help_matching_indices と同じロジックを独立して検証する。
    fn matching_indices(buf: &str) -> Vec<usize> {
        if buf.is_empty() { return vec![]; }
        HELP_ENTRIES.iter().enumerate()
            .filter(|(_, e)| !e.canonical_key.is_empty() && e.canonical_key.starts_with(buf))
            .map(|(i, _)| i)
            .collect()
    }

    /// help_append_key と同じロジックを独立して検証する。
    fn append_key(buf: &mut String, s: &str) -> Option<HelpAction> {
        buf.push_str(s);
        let mut exact_action: Option<HelpAction> = None;
        let mut has_prefix = false;
        for e in HELP_ENTRIES {
            if e.canonical_key.is_empty() { continue; }
            if e.canonical_key == buf.as_str() {
                exact_action = Some(e.action.clone());
                break;
            }
            if e.canonical_key.starts_with(buf.as_str()) {
                has_prefix = true;
            }
        }
        if let Some(action) = exact_action {
            buf.clear();
            return Some(action);
        }
        if !has_prefix { buf.clear(); }
        None
    }

    // ── help_matching_indices ──────────────────────────────────────────────────

    #[test]
    fn matching_empty_buf_returns_empty() {
        assert!(matching_indices("").is_empty());
    }

    #[test]
    fn matching_z_returns_zm_and_zr() {
        let indices = matching_indices("z");
        let zm_idx = HELP_ENTRIES.iter().position(|e| e.canonical_key == "zm")
            .expect("zmエントリがHELP_ENTRIESに存在すること");
        let zr_idx = HELP_ENTRIES.iter().position(|e| e.canonical_key == "zr")
            .expect("zrエントリがHELP_ENTRIESに存在すること");
        assert!(indices.contains(&zm_idx), "zmエントリがマッチすること");
        assert!(indices.contains(&zr_idx), "zrエントリがマッチすること");
    }

    #[test]
    fn matching_zm_returns_only_zm() {
        let indices = matching_indices("zm");
        let zm_idx = HELP_ENTRIES.iter().position(|e| e.canonical_key == "zm")
            .expect("zmエントリがHELP_ENTRIESに存在すること");
        assert_eq!(indices, vec![zm_idx]);
    }

    #[test]
    fn matching_g_returns_gt_and_gt_upper() {
        let indices = matching_indices("g");
        let gt_idx  = HELP_ENTRIES.iter().position(|e| e.canonical_key == "gt")
            .expect("gtエントリがHELP_ENTRIESに存在すること");
        let g_t_idx = HELP_ENTRIES.iter().position(|e| e.canonical_key == "gT")
            .expect("gTエントリがHELP_ENTRIESに存在すること");
        assert!(indices.contains(&gt_idx), "gtエントリがマッチすること");
        assert!(indices.contains(&g_t_idx), "gTエントリがマッチすること");
    }

    #[test]
    fn matching_unknown_key_returns_empty() {
        assert!(matching_indices("x").is_empty());
    }

    #[test]
    fn hjkl_canonical_keys_are_empty_or_not_matching() {
        // h, j, k は canonical_key が空なのでマッチしない
        assert!(matching_indices("h").is_empty(), "hはマッチしないこと");
        assert!(matching_indices("j").is_empty(), "jはマッチしないこと");
        assert!(matching_indices("k").is_empty(), "kはマッチしないこと");
        // l は "l / gt" エントリの canonical_key が "gt" になっているのでマッチしない
        assert!(matching_indices("l").is_empty(), "lはマッチしないこと");
    }

    // ── help_append_key ───────────────────────────────────────────────────────

    #[test]
    fn append_single_key_exact_match_returns_action() {
        let mut buf = String::new();
        let action = append_key(&mut buf, "i");
        assert_eq!(action, Some(HelpAction::EditCurrent));
        assert!(buf.is_empty(), "完全一致後バッファがクリアされること");
    }

    #[test]
    fn append_z_no_match_yet_keeps_buffer() {
        let mut buf = String::new();
        let action = append_key(&mut buf, "z");
        assert_eq!(action, None, "部分一致のみなのでアクションなし");
        assert_eq!(buf, "z", "部分一致ならバッファ保持");
    }

    #[test]
    fn append_zm_returns_fold() {
        let mut buf = String::new();
        append_key(&mut buf, "z");
        let action = append_key(&mut buf, "m");
        assert_eq!(action, Some(HelpAction::Fold));
        assert!(buf.is_empty());
    }

    #[test]
    fn append_unknown_key_clears_buffer() {
        let mut buf = String::new();
        let action = append_key(&mut buf, "x");
        assert_eq!(action, None);
        assert!(buf.is_empty(), "前方一致なしならバッファクリア");
    }

    #[test]
    fn append_quote_then_plus_then_p_returns_paste_below_clipboard() {
        let mut buf = String::new();
        append_key(&mut buf, "\"");
        append_key(&mut buf, "+");
        let action = append_key(&mut buf, "p");
        assert_eq!(action, Some(HelpAction::PasteBelowClipboard));
    }

    // ── エントリ全体の構造チェック ─────────────────────────────────────────────

    #[test]
    fn help_entries_count_is_even() {
        // 左右列が必ず対になるよう、エントリ数は偶数でなければならない
        assert_eq!(HELP_ENTRIES.len() % 2, 0,
            "HELP_ENTRIESは偶数個でなければならない（左右列の対称性を保つため）");
    }

    #[test]
    fn help_entries_first_is_move_down() {
        assert_eq!(HELP_ENTRIES[0].action, HelpAction::MoveDown);
    }

    #[test]
    fn help_entries_last_is_paste_below_clipboard() {
        assert_eq!(HELP_ENTRIES.last().unwrap().action, HelpAction::PasteBelowClipboard);
    }

    #[test]
    fn hjkl_entries_have_empty_canonical_key() {
        // j, k エントリは canonical_key が空であること（ヘルプモードで無効）
        let j_entry = HELP_ENTRIES.iter().find(|e| e.action == HelpAction::MoveDown)
            .expect("MoveDownエントリがHELP_ENTRIESに存在すること");
        let k_entry = HELP_ENTRIES.iter().find(|e| e.action == HelpAction::MoveUp)
            .expect("MoveUpエントリがHELP_ENTRIESに存在すること");
        assert!(j_entry.canonical_key.is_empty(), "jエントリのcanonical_keyは空");
        assert!(k_entry.canonical_key.is_empty(), "kエントリのcanonical_keyは空");
    }

    #[test]
    fn tab_next_canonical_key_is_gt_not_l() {
        // "l / gt" エントリの canonical_key は "gt"（lはヘルプモードで無効）
        let entry = HELP_ENTRIES.iter().find(|e| e.action == HelpAction::TabNext)
            .expect("TabNextエントリがHELP_ENTRIESに存在すること");
        assert_eq!(entry.canonical_key, "gt");
    }

    #[test]
    fn tabnew_canonical_key_is_tabnew() {
        // :tabnew エントリの canonical_key が ":tabnew" であること（ヘルプモードで入力できること）
        let entry = HELP_ENTRIES.iter().find(|e| e.action == HelpAction::TabNew)
            .expect("TabNewエントリがHELP_ENTRIESに存在すること");
        assert_eq!(entry.canonical_key, ":tabnew", ":tabnewのcanonical_keyは\":tabnew\"であること");
    }

    #[test]
    fn append_tabnew_returns_tabnew_action() {
        // `:tabnew` と順番に入力すると TabNew アクションが返ること
        let mut buf = String::new();
        for ch in [":", "t", "a", "b", "n", "e"] {
            let action = append_key(&mut buf, ch);
            assert_eq!(action, None, "'{}'入力後はまだアクションなし", ch);
        }
        let action = append_key(&mut buf, "w");
        assert_eq!(action, Some(HelpAction::TabNew), ":tabnew完全入力でTabNewが返ること");
        assert!(buf.is_empty(), "完全一致後バッファがクリアされること");
    }
}
