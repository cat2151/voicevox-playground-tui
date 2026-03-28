use serde_json::json;

use super::*;

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
