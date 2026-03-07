//! app モジュール内で共有されるユーティリティ関数。

use tui_textarea::TextArea;

pub(crate) fn compress_trailing_empty(mut lines: Vec<String>) -> Vec<String> {
    while lines.len() > 1 && lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        lines.pop();
    }
    if lines.is_empty() { lines.push(String::new()); }
    if lines.last().map(|l| !l.trim().is_empty()).unwrap_or(false) {
        lines.push(String::new());
    }
    lines
}

pub(crate) fn make_textarea(initial: String) -> TextArea<'static> {
    let mut ta = TextArea::new(vec![initial]);
    ta.move_cursor(tui_textarea::CursorMove::End);
    ta
}

/// 表示行インデックスリスト内で `cursor`（実行インデックス）に最も近い位置を返す。
/// `cursor` が `visible` に含まれる場合はその位置、含まれない場合は距離が最小の位置を返す。
pub(crate) fn nearest_vis_pos(cursor: usize, visible: &[usize]) -> usize {
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
