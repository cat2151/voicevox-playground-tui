use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use super::{
    focus_change, handle_blocking_overlay, should_exit_during_startup, should_ignore_key_event,
};

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

#[test]
fn should_exit_during_startup_only_on_ctrl_c_press() {
    let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    let plain_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
    let release_ctrl_c = Event::Key(KeyEvent {
        code: KeyCode::Char('c'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Release,
        state: KeyEventState::NONE,
    });

    assert!(should_exit_during_startup(&ctrl_c));
    assert!(!should_exit_during_startup(&plain_c));
    assert!(!should_exit_during_startup(&release_ctrl_c));
}
