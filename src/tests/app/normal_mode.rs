use super::*;
use crate::app::IntonationLineData;

fn make_app() -> App {
    App::new(vec!["a".to_string(), "b".to_string(), "c".to_string()])
}

fn dummy_intonation() -> IntonationLineData {
    IntonationLineData {
        query: serde_json::json!({ "accent_phrases": [] }),
        mora_texts: vec!["び".to_string()],
        pitches: vec![6.0],
        speaker_id: 3,
    }
}

#[tokio::test]
async fn take_count_empty_buf_returns_one() {
    let mut app = make_app();
    assert_eq!(app.take_count(), 1);
}

#[tokio::test]
async fn take_count_single_digit_returns_it() {
    let mut app = make_app();
    app.count_buf = "5".to_string();
    assert_eq!(app.take_count(), 5);
    assert!(app.count_buf.is_empty());
}

#[tokio::test]
async fn take_count_multi_digit_returns_parsed_value() {
    let mut app = make_app();
    app.count_buf = "10".to_string();
    assert_eq!(app.take_count(), 10);
    assert!(app.count_buf.is_empty());
}

#[tokio::test]
async fn take_count_zero_returns_one() {
    let mut app = make_app();
    app.count_buf = "0".to_string();
    assert_eq!(app.take_count(), 1);
}

#[tokio::test]
async fn delete_trailing_empty_line_keeps_it_deleted() {
    let mut app = App::new(vec!["a".to_string(), String::new()]);
    app.cursor = 1;

    app.delete_current_line().await;

    assert_eq!(app.lines, vec!["a"]);
}

#[tokio::test]
async fn delete_then_paste_below_restores_intonation() {
    let mut app = make_app();
    app.cursor = 1;
    app.line_intonations[1] = Some(dummy_intonation());

    app.delete_current_line().await;
    app.paste_below().await;

    assert_eq!(app.lines, vec!["a", "c", "b"]);
    let restored = app.line_intonations[2]
        .as_ref()
        .expect("p should restore the yanked intonation");
    assert_eq!(restored.pitches, vec![6.0]);
    assert_eq!(restored.speaker_id, 3);
}

#[tokio::test]
async fn delete_then_paste_above_restores_intonation() {
    let mut app = make_app();
    app.cursor = 1;
    app.line_intonations[1] = Some(dummy_intonation());

    app.delete_current_line().await;
    app.paste_above().await;

    assert_eq!(app.lines, vec!["a", "b", "c"]);
    let restored = app.line_intonations[1]
        .as_ref()
        .expect("P should restore the yanked intonation");
    assert_eq!(restored.pitches, vec![6.0]);
    assert_eq!(restored.speaker_id, 3);
}
