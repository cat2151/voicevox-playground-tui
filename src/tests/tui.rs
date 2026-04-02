use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use super::{focus_change, handle_blocking_overlay, should_ignore_key_event};

#[test]
fn focus_change_detects_focus_events() {
    assert_eq!(focus_change(&Event::FocusGained), Some(true));
    assert_eq!(focus_change(&Event::FocusLost), Some(false));
    assert_eq!(
        focus_change(&Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE
        ))),
        None
    );
}

#[test]
fn should_ignore_key_event_only_ignores_non_press_key_events() {
    let press = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let release = Event::Key(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Release,
        state: KeyEventState::NONE,
    });

    assert!(!should_ignore_key_event(&press));
    assert!(should_ignore_key_event(&release));
    assert!(!should_ignore_key_event(&Event::FocusGained));
}

#[test]
fn handle_blocking_overlay_returns_false_without_overlay() {
    crate::mascot_render::with_overlay_state_lock(|| {
        assert!(!handle_blocking_overlay(&Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ))));
    });
}
