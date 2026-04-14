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

#[test]
fn line_head_ctx_uses_first_spoken_segment() {
    setup();
    let ctx = line_head_ctx("[四国めたん]めたん[あまあま]あまあま");
    assert_eq!(ctx.char_name, "四国めたん");
    assert_eq!(ctx.style_name, "ノーマル");
    assert_eq!(ctx.speaker_id, 2);
}

#[test]
fn strip_known_tags_removes_only_recognized_voice_tags() {
    setup();
    assert_eq!(
        strip_known_tags(" [四国めたん][ノーマル]おはよう[meta]"),
        " おはよう[meta]"
    );
}

#[test]
fn rewrite_line_with_ctx_preserves_leading_space_and_unknown_tags() {
    setup();
    let ctx = VoiceCtx::default();
    assert_eq!(
        rewrite_line_with_ctx(" [四国めたん]おはよう[meta]", &ctx),
        " おはよう[meta]"
    );
}
