use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use tokio::sync::mpsc;

use super::{
    focus_change, handle_blocking_overlay, handle_runtime_startup, handle_startup_load,
    mode_handlers::handle_mode_event, should_exit_during_startup, should_ignore_key_event,
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

#[test]
fn handle_startup_load_returns_error_when_loader_fails() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut app = crate::app::App::new(vec![String::new()]);
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(Err(anyhow::anyhow!("boom"))).unwrap();
        let mut startup_rx = Some(rx);

        let err = handle_startup_load(&mut app, &mut startup_rx).unwrap_err();

        assert_eq!(err.to_string(), "startup error");
        assert_eq!(err.source().unwrap().to_string(), "boom");
        assert!(startup_rx.is_none());
    });
}

#[test]
fn handle_startup_load_returns_error_when_loader_disconnects() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut app = crate::app::App::new(vec![String::new()]);
        let (tx, rx) = mpsc::unbounded_channel();
        drop(tx);
        let mut startup_rx = Some(rx);

        let err = handle_startup_load(&mut app, &mut startup_rx).unwrap_err();

        assert_eq!(
            err.to_string(),
            "startup error: history loader disconnected"
        );
        assert!(startup_rx.is_none());
    });
}

#[test]
fn handle_runtime_startup_updates_status_on_progress_event() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut app = crate::app::App::new(vec![String::new()]);
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(crate::startup::RuntimeStartupEvent::Status(
            "[startup] checking VOICEVOX...".to_string(),
        ))
        .unwrap();
        let mut runtime_startup_rx = Some(rx);

        let finished = handle_runtime_startup(&mut app, &mut runtime_startup_rx).unwrap();

        assert!(!finished);
        assert_eq!(app.status_msg, "[startup] checking VOICEVOX...");
        assert!(runtime_startup_rx.is_some());
    });
}

#[test]
fn handle_runtime_startup_returns_true_when_loader_succeeds() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut app = crate::app::App::new(vec![String::new()]);
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(crate::startup::RuntimeStartupEvent::Ready(Ok(())))
            .unwrap();
        let mut runtime_startup_rx = Some(rx);

        let finished = handle_runtime_startup(&mut app, &mut runtime_startup_rx).unwrap();

        assert!(finished);
        assert!(runtime_startup_rx.is_none());
    });
}

#[test]
fn handle_runtime_startup_returns_error_when_loader_fails() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut app = crate::app::App::new(vec![String::new()]);
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(crate::startup::RuntimeStartupEvent::Ready(Err(
            anyhow::anyhow!("boom"),
        )))
        .unwrap();
        let mut runtime_startup_rx = Some(rx);

        let err = handle_runtime_startup(&mut app, &mut runtime_startup_rx).unwrap_err();

        assert_eq!(err.to_string(), "startup error");
        assert_eq!(err.source().unwrap().to_string(), "boom");
        assert!(runtime_startup_rx.is_none());
    });
}

#[test]
fn handle_runtime_startup_returns_error_when_loader_disconnects() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut app = crate::app::App::new(vec![String::new()]);
        let (tx, rx) = mpsc::unbounded_channel();
        drop(tx);
        let mut runtime_startup_rx = Some(rx);

        let err = handle_runtime_startup(&mut app, &mut runtime_startup_rx).unwrap_err();

        assert_eq!(
            err.to_string(),
            "startup error: runtime loader disconnected"
        );
        assert!(runtime_startup_rx.is_none());
    });
}

fn make_speaker_style_app() -> (crate::app::App, mpsc::Receiver<crate::fetch::FetchRequest>) {
    crate::speakers::init_test_table();
    let mut app = crate::app::App::new(vec!["[四国めたん]こんにちは".to_string()]);
    app.cursor = 0;
    let (tx, rx) = mpsc::channel(8);
    app.fetch_tx = tx;
    app.enter_speaker_style_mode();
    (app, rx)
}

#[tokio::test]
async fn handle_mode_event_space_previews_in_speaker_style_mode() {
    let (mut app, mut rx) = make_speaker_style_app();

    handle_mode_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)),
    )
    .await;

    let req = rx
        .recv()
        .await
        .expect("Spaceでspeaker/styleプレビューのfetchが送られること");
    assert_eq!(req.text, "[四国めたん]こんにちは");
    assert!(req.play_after);
    assert_eq!(app.mode, crate::app::Mode::SpeakerStyle);
}

#[tokio::test]
async fn handle_mode_event_question_mark_enters_help_mode() {
    let mut app = crate::app::App::new(vec![String::new()]);

    handle_mode_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)),
    )
    .await;

    assert_eq!(app.mode, crate::app::Mode::Help);
}

#[tokio::test]
async fn handle_mode_event_h_does_not_enter_help_mode() {
    let mut app = crate::app::App::new(vec![String::new()]);

    handle_mode_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)),
    )
    .await;

    assert_eq!(app.mode, crate::app::Mode::Normal);
}

#[tokio::test]
async fn handle_mode_event_p_previews_in_speaker_style_mode() {
    let (mut app, mut rx) = make_speaker_style_app();

    handle_mode_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE)),
    )
    .await;

    let req = rx
        .recv()
        .await
        .expect("pでspeaker/styleプレビューのfetchが送られること");
    assert_eq!(req.text, "[四国めたん]こんにちは");
    assert!(req.play_after);
    assert_eq!(app.mode, crate::app::Mode::SpeakerStyle);
}
