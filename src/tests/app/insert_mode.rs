use super::*;
use crate::app::IntonationLineData;
use crate::speakers;

fn setup() {
    speakers::init_test_table();
}

fn make_app_with_line(line: &str) -> App {
    App::new(vec![line.to_string()])
}

fn dummy_intonation() -> IntonationLineData {
    IntonationLineData {
        query: serde_json::Value::Null,
        mora_texts: vec!["ず".to_string(), "ん".to_string()],
        pitches: vec![5.9, 6.0],
        speaker_id: 3,
    }
}

/// 行頭spaceを追加しただけではイントネーションデータが消えないことを確認する
#[tokio::test]
async fn commit_insert_preserves_intonation_when_adding_leading_space() {
    setup();
    let mut app = make_app_with_line("ずんだもん");
    app.cursor = 0;
    app.line_intonations[0] = Some(dummy_intonation());

    // 行頭にspaceを追加して確定する
    app.textarea = super::super::utils::make_textarea(" ずんだもん".to_string());
    app.commit_insert().await;

    assert!(
        app.line_intonations[0].is_some(),
        "行頭spaceを追加しただけでイントネーションデータが消えてはいけない"
    );
}

/// 行頭spaceを削除しただけではイントネーションデータが消えないことを確認する
#[tokio::test]
async fn commit_insert_preserves_intonation_when_removing_leading_space() {
    setup();
    let mut app = make_app_with_line(" ずんだもん");
    app.cursor = 0;
    app.line_intonations[0] = Some(dummy_intonation());

    // 行頭のspaceを削除して確定する
    app.textarea = super::super::utils::make_textarea("ずんだもん".to_string());
    app.commit_insert().await;

    assert!(
        app.line_intonations[0].is_some(),
        "行頭spaceを削除しただけでイントネーションデータが消えてはいけない"
    );
}

/// テキスト本文が変わった場合はイントネーションデータがクリアされることを確認する
#[tokio::test]
async fn commit_insert_clears_intonation_when_text_content_changes() {
    setup();
    let mut app = make_app_with_line("ずんだもん");
    app.cursor = 0;
    app.line_intonations[0] = Some(dummy_intonation());

    // テキスト本文を変更して確定する
    app.textarea = super::super::utils::make_textarea("めたん".to_string());
    app.commit_insert().await;

    assert!(
        app.line_intonations[0].is_none(),
        "テキスト本文が変わった場合はイントネーションデータがクリアされるべき"
    );
}
