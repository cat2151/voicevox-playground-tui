use super::*;

// ── ヘルパー: App を最小限に作れないため、help_key_buf を模したローカル関数で
//   ロジックのみテストする ─────────────────────────────────────────────────────

/// help_matching_indices と同じロジックを独立して検証する。
fn matching_indices(buf: &str) -> Vec<usize> {
    if buf.is_empty() {
        return vec![];
    }
    HELP_ENTRIES
        .iter()
        .enumerate()
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
        if e.canonical_key.is_empty() {
            continue;
        }
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
    if !has_prefix {
        buf.clear();
    }
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
    let zm_idx = HELP_ENTRIES
        .iter()
        .position(|e| e.canonical_key == "zm")
        .expect("zmエントリがHELP_ENTRIESに存在すること");
    let zr_idx = HELP_ENTRIES
        .iter()
        .position(|e| e.canonical_key == "zr")
        .expect("zrエントリがHELP_ENTRIESに存在すること");
    assert!(indices.contains(&zm_idx), "zmエントリがマッチすること");
    assert!(indices.contains(&zr_idx), "zrエントリがマッチすること");
}

#[test]
fn matching_zm_returns_only_zm() {
    let indices = matching_indices("zm");
    let zm_idx = HELP_ENTRIES
        .iter()
        .position(|e| e.canonical_key == "zm")
        .expect("zmエントリがHELP_ENTRIESに存在すること");
    assert_eq!(indices, vec![zm_idx]);
}

#[test]
fn matching_g_returns_gt_and_gt_upper() {
    let indices = matching_indices("g");
    let gt_idx = HELP_ENTRIES
        .iter()
        .position(|e| e.canonical_key == "gt")
        .expect("gtエントリがHELP_ENTRIESに存在すること");
    let g_t_idx = HELP_ENTRIES
        .iter()
        .position(|e| e.canonical_key == "gT")
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
    assert_eq!(
        HELP_ENTRIES.len() % 2,
        0,
        "HELP_ENTRIESは偶数個でなければならない（左右列の対称性を保つため）"
    );
}

#[test]
fn help_entries_first_is_move_down() {
    assert_eq!(HELP_ENTRIES[0].action, HelpAction::MoveDown);
}

#[test]
fn help_entries_last_is_paste_below_clipboard() {
    assert_eq!(
        HELP_ENTRIES.last().unwrap().action,
        HelpAction::PasteBelowClipboard
    );
}

#[test]
fn hjkl_entries_have_empty_canonical_key() {
    // j, k エントリは canonical_key が空であること（ヘルプモードで無効）
    let j_entry = HELP_ENTRIES
        .iter()
        .find(|e| e.action == HelpAction::MoveDown)
        .expect("MoveDownエントリがHELP_ENTRIESに存在すること");
    let k_entry = HELP_ENTRIES
        .iter()
        .find(|e| e.action == HelpAction::MoveUp)
        .expect("MoveUpエントリがHELP_ENTRIESに存在すること");
    assert!(
        j_entry.canonical_key.is_empty(),
        "jエントリのcanonical_keyは空"
    );
    assert!(
        k_entry.canonical_key.is_empty(),
        "kエントリのcanonical_keyは空"
    );
}

#[test]
fn n_jk_entry_is_in_left_column() {
    // "n j/k" は左列（偶数インデックス）に配置されること（qの上に表示するため）
    let idx = HELP_ENTRIES
        .iter()
        .position(|e| e.key == "n j/k")
        .expect("n j/kエントリがHELP_ENTRIESに存在すること");
    assert_eq!(
        idx % 2,
        0,
        "n j/kエントリは左列（偶数インデックス）に表示されること"
    );
}

#[test]
fn tab_next_canonical_key_is_gt_not_l() {
    // "l / gt" エントリの canonical_key は "gt"（lはヘルプモードで無効）
    let entry = HELP_ENTRIES
        .iter()
        .find(|e| e.action == HelpAction::TabNext)
        .expect("TabNextエントリがHELP_ENTRIESに存在すること");
    assert_eq!(entry.canonical_key, "gt");
}

#[test]
fn tabnew_canonical_key_is_tabnew() {
    // :tabnew エントリの canonical_key が ":tabnew" であること（ヘルプモードで入力できること）
    let entry = HELP_ENTRIES
        .iter()
        .find(|e| e.action == HelpAction::TabNew)
        .expect("TabNewエントリがHELP_ENTRIESに存在すること");
    assert_eq!(
        entry.canonical_key, ":tabnew",
        ":tabnewのcanonical_keyは\":tabnew\"であること"
    );
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
    assert_eq!(
        action,
        Some(HelpAction::TabNew),
        ":tabnew完全入力でTabNewが返ること"
    );
    assert!(buf.is_empty(), "完全一致後バッファがクリアされること");
}
