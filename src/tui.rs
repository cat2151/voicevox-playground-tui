//! TUI端末の初期化・イベントループ・キーハンドリング。

use std::io;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
            EnableMouseCapture, DisableMouseCapture},
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
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend      = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        // イントネーション編集モードのデバウンス再生チェック（100msポーリング周期）
        if app.mode == Mode::Intonation {
            app.intonation_play_if_debounced().await;
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
                            if app.update_available.load(Ordering::Relaxed) {
                                app.update_action = Some(UpdateAction::Foreground);
                            }
                            break;
                        }
                        KeyCode::Char('j') | KeyCode::Down  => app.move_cursor(1).await,
                        KeyCode::Char('k') | KeyCode::Up    => app.move_cursor(-1).await,
                        KeyCode::Char('i') => app.enter_insert_current(),
                        KeyCode::Char('o') => app.enter_insert_below(),
                        KeyCode::Char('O') => app.enter_insert_above(),
                        KeyCode::Enter | KeyCode::Char(' ') => app.play_current().await,
                        KeyCode::Char('p') if app.pending_clipboard => app.paste_below_from_clipboard().await,
                        KeyCode::Char('P') if app.pending_clipboard => app.paste_above_from_clipboard().await,
                        KeyCode::Char('p') => app.paste_below().await,
                        KeyCode::Char('P') => app.paste_above().await,
                        KeyCode::Char('"') => {
                            app.reset_pending_prefixes();
                            app.pending_quote = true;
                        }
                        KeyCode::Char('+') if app.pending_quote => {
                            app.pending_quote = false;
                            app.pending_clipboard = true;
                        }
                        KeyCode::Char('z') => {
                            app.reset_pending_prefixes();
                            app.pending_z = true;
                        }
                        KeyCode::Char('m') if app.pending_z => {
                            app.fold();
                        }
                        KeyCode::Char('r') if app.pending_z => {
                            app.unfold();
                        }
                        KeyCode::Char('d') => {
                            if app.pending_d {
                                app.delete_current_line().await;
                            } else {
                                app.reset_pending_prefixes();
                                app.pending_d = true;
                            }
                        }
                        KeyCode::Char('g') => {
                            app.reset_pending_prefixes();
                            app.pending_g = true;
                        }
                        KeyCode::Char('t') if app.pending_g => {
                            app.tab_next();
                        }
                        KeyCode::Char('T') if app.pending_g => {
                            app.tab_prev();
                        }
                        KeyCode::Char('v') => app.enter_intonation_mode().await,
                        KeyCode::Char(':') => {
                            app.reset_pending_prefixes();
                            app.command_buf = String::new();
                            app.mode = Mode::Command;
                        }
                        KeyCode::Esc => {
                            // NormalモードでESCを押したら"q:quit"ヒントをハイライト表示する
                            const ESC_HINT_DURATION_MS: u64 = 1500;
                            app.reset_pending_prefixes();
                            app.esc_hint_until = Some(Instant::now() + Duration::from_millis(ESC_HINT_DURATION_MS));
                        }
                        _ => { app.reset_pending_prefixes(); }
                    }
                }
            }
            Mode::Insert => {
                if let Event::Key(key) = &ev {
                    match (key.code, key.modifiers) {
                        // Esc で確定してNormalモードへ
                        (KeyCode::Esc, _) => {
                            app.commit_insert().await;
                            continue;
                        }
                        // Enter で確定し、次の空行をINSERTモードで編集（vimのo相当）
                        (KeyCode::Enter, KeyModifiers::NONE) => {
                            app.commit_and_insert_below().await;
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
            Mode::Intonation => {
                match ev {
                    // マウスクリック: pitch設定
                    Event::Mouse(MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column, row, .. }) => {
                        app.intonation_handle_mouse_down(column, row).await;
                    }
                    Event::Key(key) => {
                    let num_active = !app.intonation_num_buf.is_empty();
                    match key.code {
                        // Esc: 数値入力中ならキャンセル、そうでなければイントネーション確定してNormalへ
                        KeyCode::Esc => {
                            if num_active {
                                app.intonation_num_buf.clear();
                            } else {
                                app.intonation_confirm().await;
                            }
                        }
                        // Enter: 数値入力中なら確定、そうでなければイントネーション確定してNormalへ
                        KeyCode::Enter => {
                            if num_active {
                                app.intonation_confirm_num_input().await;
                            } else {
                                app.intonation_confirm().await;
                            }
                        }
                        // Backspace: 数値バッファを1文字削除
                        KeyCode::Backspace if num_active => {
                            app.intonation_num_buf.pop();
                        }
                        // 数字: 数値入力バッファに追記
                        KeyCode::Char(c) if c.is_ascii_digit() => {
                            app.intonation_num_buf.push(c);
                        }
                        // '.': 小数点（重複不可。バッファ空の場合は "0." として開始）
                        KeyCode::Char('.') if !app.intonation_num_buf.contains('.') => {
                            if app.intonation_num_buf.is_empty() {
                                app.intonation_num_buf.push_str("0.");
                            } else {
                                app.intonation_num_buf.push('.');
                            }
                        }
                        // a-z: 対応モーラのpitchを+0.1（数値入力中は無効）
                        KeyCode::Char(c) if c.is_ascii_lowercase() && !num_active => {
                            let mora_idx = (c as usize) - ('a' as usize);
                            app.intonation_adjust_pitch(mora_idx, 0.1);
                        }
                        // A-Z: 対応モーラのpitchを-0.1（数値入力中は無効）
                        KeyCode::Char(c) if c.is_ascii_uppercase() && !num_active => {
                            let mora_idx = (c as usize) - ('A' as usize);
                            app.intonation_adjust_pitch(mora_idx, -0.1);
                        }
                        _ => {}
                    }
                    }
                    _ => {}
                }
            }
            Mode::Command => {
                if let Event::Key(key) = ev {
                    match (key.code, key.modifiers) {
                        (KeyCode::Enter, _) => {
                            app.execute_command().await;
                            app.command_buf = String::new();
                            app.mode = Mode::Normal;
                        }
                        (KeyCode::Esc, _) => {
                            app.command_buf = String::new();
                            app.mode = Mode::Normal;
                        }
                        (KeyCode::Backspace, _) => {
                            app.command_buf.pop();
                        }
                        (KeyCode::Char(c), KeyModifiers::NONE)
                        | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                            app.command_buf.push(c);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}
