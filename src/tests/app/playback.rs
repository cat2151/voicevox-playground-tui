use serde_json::json;
use tokio::sync::mpsc;

use super::*;

#[tokio::test]
async fn start_startup_voice_overlay_uses_current_line_summary() {
    let mut app = App::new(vec![String::from("ずんだもん")]);
    app.cursor = 0;

    app.start_startup_voice_overlay();

    assert_eq!(
        app.voice_render_overlay_message(),
        Some(String::from(
            "[startup] line 1 render待ち / play予約: ずんだもん"
        ))
    );
}

#[test]
fn intonation_cache_key_includes_speaker_and_query_json() {
    let key = App::intonation_cache_key(42, &json!({ "accent_phrases": [] }));
    assert_eq!(
        key,
        Some(String::from(r#"intonation:42:{"accent_phrases":[]}"#))
    );
}

#[tokio::test]
async fn evict_intonation_cache_removes_only_intonation_entries() {
    let mut app = App::new(vec![String::from("hello")]);
    let mut cache = app.cache.lock().unwrap();
    cache.insert(String::from("hello"), vec![1, 2, 3]);
    cache.insert(String::from("intonation:1:{}"), vec![4, 5, 6]);
    drop(cache);

    app.evict_intonation_cache();

    let cache = app.cache.lock().unwrap();
    assert_eq!(cache.get("hello"), Some(&vec![1, 2, 3]));
    assert!(!cache.contains_key("intonation:1:{}"));
    assert_eq!(cache.len(), 1);
}

#[tokio::test]
async fn fetch_and_play_sends_voice_render_summary_for_uncached_line() {
    let mut app = App::new(vec![String::from("ずんだもん")]);
    let (tx, mut rx) = mpsc::channel(1);
    app.fetch_tx = tx;

    app.fetch_and_play(0).await;

    let req = rx.recv().await.unwrap();
    assert!(req.play_after);
    assert_eq!(
        req.summary,
        Some(String::from("line 1 render待ち / play予約: ずんだもん"))
    );
}

#[tokio::test]
async fn fetch_and_play_clears_voice_render_overlay_for_cached_line() {
    let mut app = App::new(vec![String::from("ずんだもん")]);
    app.cache
        .lock()
        .unwrap()
        .insert(String::from("ずんだもん"), vec![1, 2, 3]);
    app.start_startup_voice_overlay();

    app.fetch_and_play(0).await;

    assert_eq!(app.voice_render_overlay_message(), None);
}

#[tokio::test]
async fn fetch_and_play_keeps_overlay_visible_while_pitches_only_falls_back_to_fetch() {
    crate::speakers::init_test_table();
    let mut app = App::new(vec![String::from("ずんだもん[四国めたん]こんにちは")]);
    app.line_intonations[0] = Some(IntonationLineData {
        query: serde_json::Value::Null,
        mora_texts: Vec::new(),
        pitches: vec![5.8],
        speaker_id: 3,
    });
    let (tx, mut rx) = mpsc::channel(1);
    app.fetch_tx = tx;

    app.fetch_and_play(0).await;

    let req = rx.recv().await.unwrap();
    assert!(req.play_after);
    assert_eq!(
        app.voice_render_overlay_message(),
        Some(String::from(
            "line 1 render待ち / play予約: ずんだもん[四国めたん]こんにちは"
        ))
    );
}

#[test]
fn line_cache_key_uses_stable_pitches_only_key_for_loaded_intonation() {
    crate::speakers::init_test_table();
    let data = IntonationLineData {
        query: serde_json::Value::Null,
        mora_texts: Vec::new(),
        pitches: vec![5.8],
        speaker_id: 0,
    };

    let key = App::line_cache_key("ずんだもん", Some(&data));

    assert_eq!(key, r#"intonation:pitches:[3,"ずんだもん",[5.8]]"#);
}

#[tokio::test]
async fn fetch_and_play_uses_cached_pitches_only_intonation_without_resolving_query() {
    crate::speakers::init_test_table();
    let mut app = App::new(vec![String::from("ずんだもん")]);
    app.line_intonations[0] = Some(IntonationLineData {
        query: serde_json::Value::Null,
        mora_texts: Vec::new(),
        pitches: vec![5.8],
        speaker_id: 0,
    });
    let cache_key = App::line_cache_key("ずんだもん", app.line_intonations[0].as_ref());
    app.cache.lock().unwrap().insert(cache_key, vec![1, 2, 3]);
    let (tx, mut rx) = mpsc::channel(1);
    app.play_tx = tx;

    app.fetch_and_play(0).await;

    let request = rx.recv().await.unwrap();
    assert_eq!(request.wav, vec![1, 2, 3]);
    assert_eq!(request.source_text, "ずんだもん");
    assert_eq!(app.status_msg, "[♬ cached] line 1");
    assert!(app.line_intonations[0].as_ref().unwrap().query.is_null());
}

#[tokio::test]
async fn restart_background_prefetch_sends_pitches_only_intonation_request() {
    crate::speakers::init_test_table();
    let mut app = App::new(vec![String::from("cursor"), String::from("ずんだもん")]);
    app.cursor = 0;
    app.cache
        .lock()
        .unwrap()
        .insert(String::from("cursor"), vec![1]);
    app.line_intonations[1] = Some(IntonationLineData {
        query: serde_json::Value::Null,
        mora_texts: Vec::new(),
        pitches: vec![5.8],
        speaker_id: 0,
    });
    let (tx, mut rx) = mpsc::channel(1);
    app.fetch_tx = tx;

    app.restart_background_prefetch();

    let request = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("background prefetch request timed out")
        .expect("background prefetch channel closed");
    assert_eq!(request.text, "ずんだもん");
    assert!(!request.play_after);
    assert_eq!(
        request.cache_key(),
        Some(String::from(r#"intonation:pitches:[3,"ずんだもん",[5.8]]"#))
    );

    app.bg_prefetch_handle.take().unwrap().abort();
}
