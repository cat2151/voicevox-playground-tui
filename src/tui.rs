//! TUI端末の初期化・イベントループ・キーハンドリング。

mod mode_handlers;

use std::io;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture,
        Event, KeyCode,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::app::{App, Mode, UpdateAction};
use crate::mascot_render;
use crate::ui;

pub async fn run(app: &mut App) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableFocusChange
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Drop時にraw mode・代替画面・マウスキャプチャを確実に復帰する
    struct TerminalGuard;
    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            let _ = disable_raw_mode();
            let _ = execute!(
                io::stdout(),
                LeaveAlternateScreen,
                DisableMouseCapture,
                DisableFocusChange
            );
        }
    }
    let _guard = TerminalGuard;

    const AUTO_SAVE_INTERVAL: Duration = Duration::from_secs(60);

    loop {
        // イントネーション編集モードのデバウンス再生チェック（100msポーリング周期）
        if app.mode == Mode::Intonation {
            app.intonation_play_if_debounced().await;
        }

        // 1分ごとにオートセーブする
        if app.last_autosave.elapsed() >= AUTO_SAVE_INTERVAL {
            let all_tab_lines = app.all_tab_lines();
            let all_tab_intonations = app.all_tab_intonations();
            let session_state = app.collect_session_state();
            let _ = crate::history::save_all(&all_tab_lines, &all_tab_intonations);
            let _ = crate::history::save_session_state(&session_state);
            app.last_autosave = Instant::now();
        }

        terminal.draw(|f| ui::draw(f, app))?;

        // アップデートが利用可能になったら自動的にアップデートを開始する
        if app.update_available.load(Ordering::Relaxed) && app.mode == Mode::Normal {
            app.update_action = Some(UpdateAction::Foreground);
            break;
        }

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        let ev = event::read()?;

        if let Some(focused) = focus_change(&ev) {
            app.focused = focused;
            continue;
        }

        // Windows は Press/Release 両方を送るため Press のみ処理する
        if should_ignore_key_event(&ev) {
            continue;
        }

        if handle_blocking_overlay(&ev) {
            continue;
        }

        if mode_handlers::handle_mode_event(app, ev).await == mode_handlers::LoopControl::Break {
            break;
        }
    }

    // _guard がDrop時に端末を復帰させる
    Ok(())
}

fn focus_change(ev: &Event) -> Option<bool> {
    match ev {
        Event::FocusGained => Some(true),
        Event::FocusLost => Some(false),
        _ => None,
    }
}

fn should_ignore_key_event(ev: &Event) -> bool {
    matches!(ev, Event::Key(key) if key.kind != event::KeyEventKind::Press)
}

fn handle_blocking_overlay(ev: &Event) -> bool {
    if !mascot_render::has_blocking_overlay_message() {
        return false;
    }

    if let Event::Key(key) = ev {
        if key.code == KeyCode::Enter {
            mascot_render::dismiss_blocking_overlay_message();
        }
    }

    true
}

#[cfg(test)]
mod tests {
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
        crate::mascot_render::dismiss_blocking_overlay_message();
        assert!(!handle_blocking_overlay(&Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ))));
    }
}
