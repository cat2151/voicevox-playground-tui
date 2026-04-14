use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use tui_textarea::{Input, Key};

use crate::app::{App, HelpAction, Mode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LoopControl {
    Continue,
    Break,
}

pub(super) async fn handle_mode_event(app: &mut App, ev: Event) -> LoopControl {
    match app.mode {
        Mode::Normal => handle_normal_mode(app, ev).await,
        Mode::Insert => handle_insert_mode(app, ev).await,
        Mode::SpeakerStyle => handle_speaker_style_mode(app, ev).await,
        Mode::Intonation => handle_intonation_mode(app, ev).await,
        Mode::Command => handle_command_mode(app, ev).await,
        Mode::Help => handle_help_mode(app, ev).await,
    }
}

async fn handle_normal_mode(app: &mut App, ev: Event) -> LoopControl {
    if let Event::Key(key) = ev {
        match key.code {
            KeyCode::Char('q') => return LoopControl::Break,
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Enter => {
                let count = app.take_count();
                app.move_cursor(count as i32).await;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let count = app.take_count();
                app.move_cursor(-(count as i32)).await;
            }
            KeyCode::Char('i') => app.enter_insert_current(),
            KeyCode::Char('o') => app.enter_insert_below(),
            KeyCode::Char('O') => app.enter_insert_above(),
            KeyCode::Char(' ') => app.play_current().await,
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
            KeyCode::Char('m') if app.pending_z => app.fold(),
            KeyCode::Char('r') if app.pending_z => app.unfold(),
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
            KeyCode::Char('t') if app.pending_g => app.tab_next(),
            KeyCode::Char('T') if app.pending_g => app.tab_prev(),
            KeyCode::Char('s') => app.enter_speaker_style_mode(),
            KeyCode::Char('v') => app.enter_intonation_mode().await,
            KeyCode::Char('h') | KeyCode::Left => app.enter_help_mode(),
            KeyCode::Char('l') | KeyCode::Right => app.tab_next(),
            KeyCode::Char(':') => {
                app.reset_pending_prefixes();
                app.command_buf = String::new();
                app.mode = Mode::Command;
            }
            KeyCode::Esc => {
                const ESC_HINT_DURATION_MS: u64 = 1500;
                app.reset_pending_prefixes();
                app.esc_hint_until =
                    Some(Instant::now() + Duration::from_millis(ESC_HINT_DURATION_MS));
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                if app.count_buf.len() < 6 {
                    app.count_buf.push(c);
                }
            }
            _ => app.reset_pending_prefixes(),
        }
    }

    LoopControl::Continue
}

async fn handle_insert_mode(app: &mut App, ev: Event) -> LoopControl {
    if let Event::Key(key) = &ev {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                app.commit_insert().await;
                return LoopControl::Continue;
            }
            (KeyCode::Enter, KeyModifiers::NONE) => {
                app.commit_and_insert_below().await;
                return LoopControl::Continue;
            }
            (KeyCode::Enter, _) => return LoopControl::Continue,
            _ => {}
        }
    }

    let input: Input = ev.into();
    if input.key == Key::Enter {
        return LoopControl::Continue;
    }

    if app.textarea.input(input) {
        app.on_edit_buf_changed().await;
    }

    LoopControl::Continue
}

async fn handle_speaker_style_mode(app: &mut App, ev: Event) -> LoopControl {
    if let Event::Key(key) = ev {
        match key.code {
            KeyCode::Esc => app.cancel_speaker_style_mode(),
            KeyCode::Enter => app.confirm_speaker_style_mode(),
            KeyCode::Char('h') | KeyCode::Left => app.speaker_style_focus_speaker(),
            KeyCode::Char('l') | KeyCode::Right => app.speaker_style_focus_style(),
            KeyCode::Char(' ') | KeyCode::Char('p') => {
                if let Some(preview_line) = app.speaker_style_selected_preview_line() {
                    app.preview_speaker_style_selection(preview_line).await;
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(preview_line) = app.speaker_style_adjust_selection(1) {
                    app.preview_speaker_style_selection(preview_line).await;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(preview_line) = app.speaker_style_adjust_selection(-1) {
                    app.preview_speaker_style_selection(preview_line).await;
                }
            }
            _ => {}
        }
    }

    LoopControl::Continue
}

async fn handle_intonation_mode(app: &mut App, ev: Event) -> LoopControl {
    match ev {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left),
            column,
            row,
            ..
        }) => {
            app.intonation_handle_mouse_down(column, row).await;
        }
        Event::Key(key) => {
            let num_active = !app.intonation_num_buf.is_empty();
            match key.code {
                KeyCode::Esc => {
                    if num_active {
                        app.intonation_num_buf.clear();
                    } else {
                        app.intonation_confirm().await;
                    }
                }
                KeyCode::Enter => {
                    if num_active {
                        app.intonation_confirm_num_input().await;
                    } else {
                        app.intonation_confirm().await;
                    }
                }
                KeyCode::Backspace if num_active => {
                    app.intonation_num_buf.pop();
                }
                KeyCode::Char(' ') => {
                    app.intonation_play_now().await;
                }
                KeyCode::Char('i') if !num_active => {
                    app.intonation_reset_to_initial().await;
                }
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    app.intonation_num_buf.push(c);
                }
                KeyCode::Char('.') if !app.intonation_num_buf.contains('.') => {
                    if app.intonation_num_buf.is_empty() {
                        app.intonation_num_buf.push_str("0.");
                    } else {
                        app.intonation_num_buf.push('.');
                    }
                }
                KeyCode::Char('h') | KeyCode::Left if !num_active => {
                    app.intonation_move_cursor(-1);
                }
                KeyCode::Char('l') | KeyCode::Right if !num_active => {
                    app.intonation_move_cursor(1);
                }
                KeyCode::Char('k') | KeyCode::Up if !num_active => {
                    app.intonation_adjust_current_pitch(0.1);
                }
                KeyCode::Char('j') | KeyCode::Down if !num_active => {
                    app.intonation_adjust_current_pitch(-0.1);
                }
                KeyCode::Char(c) if c.is_ascii_lowercase() && !num_active => {
                    let mora_idx = (c as usize) - ('a' as usize);
                    app.intonation_adjust_pitch(mora_idx, 0.1);
                }
                KeyCode::Char(c) if c.is_ascii_uppercase() && !num_active => {
                    let mora_idx = (c as usize) - ('A' as usize);
                    app.intonation_adjust_pitch(mora_idx, -0.1);
                }
                _ => {}
            }
        }
        _ => {}
    }

    LoopControl::Continue
}

async fn handle_command_mode(app: &mut App, ev: Event) -> LoopControl {
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
            (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                app.command_buf.push(c);
            }
            _ => {}
        }
    }

    LoopControl::Continue
}

async fn handle_help_mode(app: &mut App, ev: Event) -> LoopControl {
    if let Event::Key(key) = ev {
        match key.code {
            KeyCode::Esc => {
                app.help_key_buf.clear();
                app.mode = Mode::Normal;
            }
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Enter => {
                app.help_key_buf.clear();
                app.mode = Mode::Normal;
                let count = app.take_count();
                app.move_cursor(count as i32).await;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.help_key_buf.clear();
                app.mode = Mode::Normal;
                let count = app.take_count();
                app.move_cursor(-(count as i32)).await;
            }
            KeyCode::Char('l') | KeyCode::Right => {
                app.help_key_buf.clear();
                app.mode = Mode::Normal;
                app.tab_next();
            }
            KeyCode::Char('h') | KeyCode::Left => {
                app.help_key_buf.clear();
                app.mode = Mode::Normal;
            }
            _ => {
                let maybe_action = match key.code {
                    KeyCode::Char(' ') => app.help_append_key(" "),
                    KeyCode::Char(c) => app.help_append_key(&c.to_string()),
                    _ => None,
                };
                if let Some(action) = maybe_action {
                    match action {
                        HelpAction::MoveDown => app.move_cursor(1).await,
                        HelpAction::MoveUp => app.move_cursor(-1).await,
                        HelpAction::EditCurrent => app.enter_insert_current(),
                        HelpAction::InsertBelow => app.enter_insert_below(),
                        HelpAction::InsertAbove => app.enter_insert_above(),
                        HelpAction::PlayCurrent => app.play_current().await,
                        HelpAction::DeleteLine => app.delete_current_line().await,
                        HelpAction::PasteBelow => app.paste_below().await,
                        HelpAction::PasteAbove => app.paste_above().await,
                        HelpAction::PasteBelowClipboard => app.paste_below_from_clipboard().await,
                        HelpAction::PasteAboveClipboard => app.paste_above_from_clipboard().await,
                        HelpAction::Fold => app.fold(),
                        HelpAction::Unfold => app.unfold(),
                        HelpAction::SpeakerStyleMode => app.enter_speaker_style_mode(),
                        HelpAction::IntonationMode => app.enter_intonation_mode().await,
                        HelpAction::TabNext => app.tab_next(),
                        HelpAction::TabPrev => app.tab_prev(),
                        HelpAction::TabNew => app.tabnew(),
                        HelpAction::Quit => return LoopControl::Break,
                        HelpAction::None => {}
                    }
                }
            }
        }
    }

    LoopControl::Continue
}
