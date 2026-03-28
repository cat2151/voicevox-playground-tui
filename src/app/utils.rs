//! app モジュール内で共有されるユーティリティ関数。

use tui_textarea::TextArea;

pub(super) fn compress_trailing_empty(mut lines: Vec<String>) -> Vec<String> {
    while lines.len() > 1 && lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        lines.pop();
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    if lines.last().map(|l| !l.trim().is_empty()).unwrap_or(false) {
        lines.push(String::new());
    }
    lines
}

pub(super) fn make_textarea(initial: String) -> TextArea<'static> {
    let mut ta = TextArea::new(vec![initial]);
    ta.move_cursor(tui_textarea::CursorMove::End);
    ta
}

/// 表示行インデックスリスト内で `cursor`（実行インデックス）に最も近い位置を返す。
/// `cursor` が `visible` に含まれる場合はその位置、含まれない場合は距離が最小の位置を返す。
pub(super) fn nearest_vis_pos(cursor: usize, visible: &[usize]) -> usize {
    visible
        .iter()
        .position(|&i| i == cursor)
        .unwrap_or_else(|| {
            visible
                .iter()
                .enumerate()
                .min_by_key(|(_, &i)| {
                    let diff = i as isize - cursor as isize;
                    diff.unsigned_abs()
                })
                .map(|(idx, _)| idx)
                .unwrap_or(0)
        })
}

#[cfg(test)]
mod tests {
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
}
