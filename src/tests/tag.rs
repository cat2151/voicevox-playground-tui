use super::*;
use crate::speakers;

fn setup() {
    speakers::init_test_table();
}

#[test]
fn split_no_change_plain_text() {
    setup();
    let result = split_by_ctx_change("ずんだもん喋る");
    assert_eq!(result, vec!["ずんだもん喋る"]);
}

#[test]
fn split_no_change_prefix_tags_only() {
    setup();
    // タグが先頭にだけある場合は分割されない
    let result = split_by_ctx_change("[四国めたん]めたん喋る");
    assert_eq!(result, vec!["[四国めたん]めたん喋る"]);
}

#[test]
fn split_mid_line_char_change() {
    setup();
    // 行の途中でキャラが変わる場合は分割される
    let result = split_by_ctx_change("ずんだもん喋る[四国めたん]めたん喋る");
    assert_eq!(result, vec!["ずんだもん喋る", "[四国めたん]めたん喋る",]);
}

#[test]
fn split_mid_line_style_change() {
    setup();
    // 行の途中でスタイルが変わる場合は分割される
    let result = split_by_ctx_change("ずんだもん喋る[あまあま]あまあま喋る");
    assert_eq!(result, vec!["ずんだもん喋る", "[あまあま]あまあま喋る",]);
}

#[test]
fn split_mid_line_multiple_changes() {
    setup();
    // 複数回変わる場合は複数行に分割される
    // [ずんだもん]は先頭で現在キャラに戻り、[あまあま]はスタイル変更（バッファ空なので分割なし）
    let result = split_by_ctx_change("ずんだもん[四国めたん]めたん[ずんだもん][あまあま]あまあま");
    assert_eq!(
        result,
        vec!["ずんだもん", "[四国めたん]めたん", "[あまあま]あまあま",]
    );
}
