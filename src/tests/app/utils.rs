use super::*;

// ── compress_trailing_empty ──────────────────────────────────────────────

#[test]
fn compress_empty_input_returns_single_empty_line() {
    let result = compress_trailing_empty(vec![]);
    assert_eq!(result, vec![""]);
}

#[test]
fn compress_single_non_empty_line_appends_trailing_empty() {
    let result = compress_trailing_empty(vec!["hello".to_string()]);
    assert_eq!(result, vec!["hello", ""]);
}

#[test]
fn compress_single_empty_line_unchanged() {
    // 要素が1つだけの空行は変更なし（ポップしない）
    let result = compress_trailing_empty(vec!["".to_string()]);
    assert_eq!(result, vec![""]);
}

#[test]
fn compress_multiple_trailing_empty_lines_collapsed_to_one() {
    let input = vec![
        "hello".to_string(),
        "".to_string(),
        "".to_string(),
        "".to_string(),
    ];
    let result = compress_trailing_empty(input);
    assert_eq!(result, vec!["hello", ""]);
}

#[test]
fn compress_no_trailing_empty_appends_one() {
    // 末尾が非空行ならトレイリング空行を1つ追加する
    let input = vec!["hello".to_string(), "world".to_string()];
    let result = compress_trailing_empty(input);
    assert_eq!(result, vec!["hello", "world", ""]);
}

#[test]
fn compress_preserves_internal_empty_lines() {
    let input = vec!["a".to_string(), "".to_string(), "b".to_string()];
    let result = compress_trailing_empty(input);
    // 末尾が非空なので空行が追加される
    assert_eq!(result, vec!["a", "", "b", ""]);
}

#[test]
fn compress_whitespace_only_line_is_treated_as_empty() {
    // " " (空白のみ) も空行扱いでポップされる
    let input = vec!["hello".to_string(), "  ".to_string(), "   ".to_string()];
    let result = compress_trailing_empty(input);
    assert_eq!(result, vec!["hello", ""]);
}

// ── nearest_vis_pos ───────────────────────────────────────────────────────

#[test]
fn nearest_vis_pos_empty_visible_returns_zero() {
    // visible が空のとき fallback は 0
    assert_eq!(nearest_vis_pos(3, &[]), 0);
}

#[test]
fn nearest_vis_pos_cursor_in_visible_returns_its_position() {
    // cursor=2 は visible の index 1 にある
    let visible = vec![0, 2, 4];
    assert_eq!(nearest_vis_pos(2, &visible), 1);
}

#[test]
fn nearest_vis_pos_cursor_not_in_visible_returns_nearest() {
    // cursor=3 は visible=[0,2,5] の中で 2 (diff=1) が最近傍 → index 1
    let visible = vec![0, 2, 5];
    assert_eq!(nearest_vis_pos(3, &visible), 1);
}

#[test]
fn nearest_vis_pos_cursor_before_all_visible_returns_first() {
    let visible = vec![5, 10, 15];
    // cursor=0, 最近傍は 5 → index 0
    assert_eq!(nearest_vis_pos(0, &visible), 0);
}

#[test]
fn nearest_vis_pos_cursor_after_all_visible_returns_last() {
    let visible = vec![0, 1, 2];
    // cursor=100, 最近傍は 2 → index 2
    assert_eq!(nearest_vis_pos(100, &visible), 2);
}

#[test]
fn nearest_vis_pos_single_element_always_returns_zero() {
    let visible = vec![7];
    assert_eq!(nearest_vis_pos(0, &visible), 0);
    assert_eq!(nearest_vis_pos(7, &visible), 0);
    assert_eq!(nearest_vis_pos(99, &visible), 0);
}
