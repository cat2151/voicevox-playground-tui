//! TUI端末の初期化・イベントループ・キーハンドリング。

use std::io;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tui_textarea::{Input, Key};

use crate::app::{App, Mode, UpdateAction};
use crate::ui;

pub async fn run(app: &mut App) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend      = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // アップデートが利用可能になったらダイアログを表示する
        if app.update_available.load(Ordering::Relaxed)
            && !app.update_dismissed
            && app.mode == Mode::Normal
        {
            app.mode = Mode::UpdateAvailableDialog;
        }

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        let ev = event::read()?;

        // Windows は Press/Release 両方を送るため Press のみ処理する
        if let Event::Key(key) = &ev {
            if key.kind != event::KeyEventKind::Press {
                continue;
            }
        }

        match app.mode {
            Mode::Normal => {
                if let Event::Key(key) = ev {
                    match key.code {
                        KeyCode::Char('q') => {
                            // アップデートが利用可能で未却下の場合はダイアログを表示
                            if app.update_available.load(Ordering::Relaxed) && !app.update_dismissed {
                                app.mode = Mode::QuitWithUpdateDialog;
                            } else {
                                break;
                            }
                        }
                        KeyCode::Char('j') | KeyCode::Down  => app.move_cursor(1).await,
                        KeyCode::Char('k') | KeyCode::Up    => app.move_cursor(-1).await,
                        KeyCode::Char('i') => app.enter_insert_current(),
                        KeyCode::Char('o') => app.enter_insert_below(),
                        KeyCode::Char('O') => app.enter_insert_above(),
                        KeyCode::Enter | KeyCode::Char(' ') => app.play_current().await,
                        KeyCode::Char('p') => app.paste_below().await,
                        KeyCode::Char('P') => app.paste_above().await,
                        KeyCode::Char('d') => {
                            if app.pending_d {
                                app.delete_current_line().await;
                            } else {
                                app.pending_d = true;
                            }
                        }
                        _ => { app.pending_d = false; }
                    }
                }
            }
            Mode::Insert => {
                if let Event::Key(key) = &ev {
                    match (key.code, key.modifiers) {
                        // Esc / Enter で確定
                        (KeyCode::Esc, _) |
                        (KeyCode::Enter, KeyModifiers::NONE) => {
                            app.commit_insert().await;
                            continue;
                        }
                        // Enter+修飾キーはtextareaに渡さない（改行防止）
                        (KeyCode::Enter, _) => continue,
                        _ => {}
                    }
                }
                // それ以外はtui-textareaにそのまま渡す（Emacsキーバインド込み）
                let input: Input = ev.into();
                // 改行系キーは弾く（シングルライン強制）
                if input.key == Key::Enter { continue; }
                let changed = app.textarea.input(input);
                if changed {
                    app.on_edit_buf_changed().await;
                }
            }
            Mode::UpdateAvailableDialog => {
                if let Event::Key(key) = ev {
                    match key.code {
                        KeyCode::Char('b') => {
                            app.update_action = Some(UpdateAction::Background);
                            break;
                        }
                        KeyCode::Char('f') => {
                            app.update_action = Some(UpdateAction::Foreground);
                            break;
                        }
                        KeyCode::Esc => {
                            // ダイアログを却下して通常操作に戻る
                            app.update_dismissed = true;
                            app.mode = Mode::Normal;
                        }
                        _ => {}
                    }
                }
            }
            Mode::QuitWithUpdateDialog => {
                if let Event::Key(key) = ev {
                    match key.code {
                        KeyCode::Char('q') => {
                            // アップデートせず終了
                            break;
                        }
                        KeyCode::Char('f') => {
                            app.update_action = Some(UpdateAction::Foreground);
                            break;
                        }
                        KeyCode::Esc => {
                            // 終了をキャンセルして通常操作に戻る
                            app.update_dismissed = true;
                            app.mode = Mode::Normal;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
