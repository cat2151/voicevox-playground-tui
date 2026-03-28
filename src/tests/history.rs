use super::*;

#[test]
fn split_pitches_suffix_no_suffix_returns_original() {
    let (text, pitches) = split_pitches_suffix("ずんだもん");
    assert_eq!(text, "ずんだもん");
    assert!(pitches.is_none());
}

#[test]
fn split_pitches_suffix_with_valid_suffix_returns_text_and_pitches() {
    let raw = "ずんだもん\t{\"pitches\":[5.9,6.0,0.0]}";
    let (text, pitches) = split_pitches_suffix(raw);
    assert_eq!(text, "ずんだもん");
    let p = pitches.unwrap();
    assert_eq!(p.len(), 3);
    assert!((p[0] - 5.9).abs() < 1e-9);
    assert!((p[1] - 6.0).abs() < 1e-9);
    assert!((p[2] - 0.0).abs() < 1e-9);
}

#[test]
fn split_pitches_suffix_tab_but_not_json_returns_original() {
    let raw = "some\ttext without pitches";
    let (text, pitches) = split_pitches_suffix(raw);
    assert_eq!(text, raw);
    assert!(pitches.is_none());
}

#[test]
fn format_with_pitches_embeds_suffix() {
    let result = format_with_pitches("ずんだもん", &[5.9, 6.0, 0.0]);
    assert!(result.starts_with("ずんだもん\t"));
    assert!(result.contains("\"pitches\""));
    // Round-trip: split should recover the original
    let (text, pitches) = split_pitches_suffix(&result);
    assert_eq!(text, "ずんだもん");
    let p = pitches.unwrap();
    assert!((p[0] - 5.9).abs() < 1e-9);
    assert!((p[1] - 6.0).abs() < 1e-9);
    assert!((p[2] - 0.0).abs() < 1e-9);
}

#[test]
fn split_pitches_suffix_empty_pitches_array() {
    let raw = "text\t{\"pitches\":[]}";
    let (text, pitches) = split_pitches_suffix(raw);
    assert_eq!(text, "text");
    let p = pitches.unwrap();
    assert!(p.is_empty());
}

#[test]
fn session_state_default_is_zeroed() {
    let s = SessionState::default();
    assert_eq!(s.active_tab, 0);
    assert!(s.tabs.is_empty());
}

#[test]
fn session_state_round_trips_through_json() {
    let state = SessionState {
        active_tab: 2,
        tabs: vec![
            TabSessionState {
                cursor: 5,
                folded: false,
            },
            TabSessionState {
                cursor: 0,
                folded: true,
            },
            TabSessionState {
                cursor: 3,
                folded: false,
            },
        ],
    };
    let json = serde_json::to_string(&state).unwrap();
    let restored: SessionState = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.active_tab, 2);
    assert_eq!(restored.tabs.len(), 3);
    assert_eq!(restored.tabs[0].cursor, 5);
    assert!(!restored.tabs[0].folded);
    assert_eq!(restored.tabs[1].cursor, 0);
    assert!(restored.tabs[1].folded);
    assert_eq!(restored.tabs[2].cursor, 3);
    assert!(!restored.tabs[2].folded);
}
