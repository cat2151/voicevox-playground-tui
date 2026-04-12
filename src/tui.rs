//! TUI端末の初期化・イベントループ・キーハンドリング。

mod mode_handlers;

use std::io;
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
use tokio::sync::mpsc;

use crate::app::{App, Mode};
use crate::mascot_render;
use crate::startup::{LoadedHistoryResult, RuntimeStartupEvent};
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitDisposition {
    PersistState,
    SkipPersistState,
}

pub async fn run(
    app: &mut App,
    mut history_rx: Option<mpsc::UnboundedReceiver<LoadedHistoryResult>>,
    mut runtime_startup_rx: Option<mpsc::UnboundedReceiver<RuntimeStartupEvent>>,
) -> Result<ExitDisposition> {
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
    let mut history_pending = history_rx.is_some();
    let mut runtime_startup_pending = runtime_startup_rx.is_some();
    let mut needs_init = !history_pending && !runtime_startup_pending;

    loop {
        let startup_pending = history_pending || runtime_startup_pending;

        // イントネーション編集モードのデバウンス再生チェック（100msポーリング周期）
        if app.mode == Mode::Intonation {
            app.intonation_play_if_debounced().await;
        }

        // 1分ごとにオートセーブする
        if !startup_pending && app.last_autosave.elapsed() >= AUTO_SAVE_INTERVAL {
            let all_tab_lines = app.all_tab_lines();
            let all_tab_intonations = app.all_tab_intonations();
            let session_state = app.collect_session_state();
            let _ = crate::history::save_all(&all_tab_lines, &all_tab_intonations);
            let _ = crate::history::save_session_state(&session_state);
            app.last_autosave = Instant::now();
        }

        terminal.draw(|f| ui::draw(f, app))?;

        if history_pending && handle_startup_load(app, &mut history_rx)? {
            history_pending = false;
            needs_init = !runtime_startup_pending;
        }

        if runtime_startup_pending && handle_runtime_startup(app, &mut runtime_startup_rx)? {
            runtime_startup_pending = false;
            needs_init = !history_pending;
        }

        if !history_pending && !runtime_startup_pending && needs_init {
            app.status_msg = String::from("ready");
            app.init().await;
            needs_init = false;
        }

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        let ev = event::read()?;

        if startup_pending {
            if should_exit_during_startup(&ev) {
                return Ok(ExitDisposition::SkipPersistState);
            }
            continue;
        }

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
            return Ok(ExitDisposition::PersistState);
        }
    }
}

fn handle_startup_load(
    app: &mut App,
    history_rx: &mut Option<mpsc::UnboundedReceiver<LoadedHistoryResult>>,
) -> Result<bool> {
    let Some(rx) = history_rx.as_mut() else {
        return Ok(true);
    };

    match rx.try_recv() {
        Ok(Ok(loaded)) => {
            app.apply_loaded_history(
                loaded.all_lines,
                loaded.all_intonations,
                &loaded.session_state,
            );
            *history_rx = None;
            Ok(true)
        }
        Ok(Err(err)) => {
            *history_rx = None;
            Err(err.context("startup error"))
        }
        Err(mpsc::error::TryRecvError::Empty) => Ok(false),
        Err(mpsc::error::TryRecvError::Disconnected) => {
            *history_rx = None;
            Err(anyhow::anyhow!(
                "startup error: history loader disconnected"
            ))
        }
    }
}

fn handle_runtime_startup(
    app: &mut App,
    runtime_startup_rx: &mut Option<mpsc::UnboundedReceiver<RuntimeStartupEvent>>,
) -> Result<bool> {
    let Some(rx) = runtime_startup_rx.as_mut() else {
        return Ok(true);
    };

    match rx.try_recv() {
        Ok(RuntimeStartupEvent::Status(status)) => {
            app.status_msg = status;
            Ok(false)
        }
        Ok(RuntimeStartupEvent::Ready(Ok(()))) => {
            *runtime_startup_rx = None;
            Ok(true)
        }
        Ok(RuntimeStartupEvent::Ready(Err(err))) => {
            *runtime_startup_rx = None;
            Err(err.context("startup error"))
        }
        Err(mpsc::error::TryRecvError::Empty) => Ok(false),
        Err(mpsc::error::TryRecvError::Disconnected) => {
            *runtime_startup_rx = None;
            Err(anyhow::anyhow!(
                "startup error: runtime loader disconnected"
            ))
        }
    }
}

fn should_exit_during_startup(ev: &Event) -> bool {
    matches!(
        ev,
        Event::Key(key)
            if key.kind == event::KeyEventKind::Press
                && key.code == KeyCode::Char('c')
                && key.modifiers.contains(event::KeyModifiers::CONTROL)
    )
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
#[path = "tests/tui.rs"]
mod tests;
