use super::*;

fn make_query_with_pitches(texts: &[&str], pitches: &[f64]) -> serde_json::Value {
    let moras: Vec<serde_json::Value> = texts
        .iter()
        .zip(pitches.iter())
        .map(|(&t, &p)| serde_json::json!({ "text": t, "pitch": p }))
        .collect();
    serde_json::json!({ "accent_phrases": [{ "moras": moras }] })
}

#[test]
fn extract_mora_data_returns_texts_and_pitches() {
    let query = make_query_with_pitches(&["ず", "ん", "だ"], &[5.87, 6.0, 0.0]);
    let (texts, pitches) = extract_mora_data(&query);
    assert_eq!(texts, vec!["ず", "ん", "だ"]);
    assert_eq!(pitches, vec![5.87, 6.0, 0.0]);
}

#[test]
fn extract_mora_data_empty_query_returns_empty() {
    let query = serde_json::json!({ "accent_phrases": [] });
    let (texts, pitches) = extract_mora_data(&query);
    assert!(texts.is_empty());
    assert!(pitches.is_empty());
}

#[test]
fn set_mora_pitches_updates_values_in_query() {
    let mut query = make_query_with_pitches(&["ず", "ん", "だ"], &[5.87, 6.0, 0.0]);
    set_mora_pitches(&mut query, &[1.1, 2.2, 3.3]);
    let (_, pitches) = extract_mora_data(&query);
    assert!((pitches[0] - 1.1).abs() < 1e-9);
    assert!((pitches[1] - 2.2).abs() < 1e-9);
    assert!((pitches[2] - 3.3).abs() < 1e-9);
}

#[test]
fn set_mora_pitches_partial_update_leaves_rest_unchanged() {
    let mut query = make_query_with_pitches(&["ず", "ん", "だ"], &[1.0, 2.0, 3.0]);
    // Only 2 new pitches: only the first 2 should change
    set_mora_pitches(&mut query, &[9.0, 8.0]);
    let (_, pitches) = extract_mora_data(&query);
    assert!((pitches[0] - 9.0).abs() < 1e-9);
    assert!((pitches[1] - 8.0).abs() < 1e-9);
    assert!((pitches[2] - 3.0).abs() < 1e-9);
}
