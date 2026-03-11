use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::{App, Mode};

use super::{BG, FG, DIM, YELLOW, GREEN, CYAN, CURSOR_NORMAL, CURSOR_INSERT};

pub(super) fn render_lines(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focused;

    let cursor_bg = if !focused {
        BG
    } else {
        match app.mode {
            Mode::Normal | Mode::Command | Mode::Help => CURSOR_NORMAL,
            Mode::Insert => CURSOR_INSERT,
            _ => CURSOR_NORMAL
        }
    };
    let cursor_fg = if !focused {
        DIM
    } else {
        match app.mode {
            Mode::Normal | Mode::Command | Mode::Help => FG,
            Mode::Insert => BG,
            _ => FG
        }
    };

    // リスト全体のRect（ボーダー内側）
    let inner = Rect {
        x:      area.x + 1,
        y:      area.y + 1,
        width:  area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    // 折りたたみ時は行頭spaceのある行を非表示にする
    let visible_indices = app.visible_line_indices();

    // 表示リスト内でのカーソル位置（非表示行の場合は最近傍の表示行位置）
    let visible_cursor = app.vis_cursor_pos();

    let items: Vec<ListItem> = visible_indices.iter().map(|&i| {
        let line = &app.lines[i];
        let cached_mark = if app.cache.lock().unwrap().contains_key(line.as_str()) { "♪ " } else { "  " };
        let intonation_mark = if app.line_intonations.get(i).and_then(|d| d.as_ref()).is_some() { "♬ " } else { "  " };

        // 折りたたみ時：次の行が行頭spaceなら"+"インジケータを表示する
        let fold_mark = if app.folded && app.lines.get(i + 1).map(|l| l.starts_with(' ')).unwrap_or(false) {
            "+"
        } else {
            " "
        };
        let line_num = format!("{}{:>4} ", fold_mark, i + 1);

        // Insertモードのカーソル行はtextareaが別途描画するので、プレースホルダにする
        let body = if app.mode == Mode::Insert && i == app.cursor {
            format!("{}<editing>", cached_mark)
        } else {
            format!("{}{}{}", cached_mark, intonation_mark, line)
        };

        let body_fg = if focused { FG } else { DIM };
        let text = Line::from(vec![
            Span::styled(line_num, Style::default().fg(DIM).bg(BG)),
            Span::styled(body,     Style::default().fg(body_fg).bg(BG)),
        ]);

        let style = if i == app.cursor {
            Style::default().fg(cursor_fg).bg(cursor_bg).bold()
        } else {
            Style::default().bg(BG)
        };
        ListItem::new(text).style(style)
    }).collect();

    let title = match app.mode {
        Mode::Normal | Mode::Command | Mode::Help => {
            if focused {
                Span::styled(" [NORMAL] ", Style::default().fg(GREEN).bold())
            } else {
                Span::styled(" [NORMAL] ", Style::default().fg(DIM))
            }
        }
        Mode::Insert => {
            if focused {
                Span::styled(" [INSERT] ", Style::default().fg(CYAN).bold())
            } else {
                Span::styled(" [INSERT] ", Style::default().fg(DIM))
            }
        }
        _ => {
            if focused {
                Span::styled(" [NORMAL] ", Style::default().fg(GREEN).bold())
            } else {
                Span::styled(" [NORMAL] ", Style::default().fg(DIM))
            }
        }
    };
    let border_color = if !focused {
        DIM
    } else {
        match app.mode {
            Mode::Normal | Mode::Command | Mode::Help => DIM,
            Mode::Insert => CYAN,
            _ => DIM
        }
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(title)
                .style(Style::default().bg(BG)),
        )
        .highlight_symbol(if focused { ">> " } else { "   " });

    let mut state = ListState::default();
    state.select(Some(visible_cursor));
    f.render_stateful_widget(list, area, &mut state);

    // Insertモード: カーソル行にtextareaを重ねて描画する（フォーカス中のみ）
    if focused && app.mode == Mode::Insert {
        // render_stateful_widget後のstate.offset()がratatuiの実際のスクロール位置
        let win_start = state.offset();
        // スクロール後の画面上の行位置を計算する
        if visible_cursor >= win_start {
            let row_in_inner = (visible_cursor - win_start) as u16;
            if row_in_inner < inner.height {
                // 行番号分(7文字)だけ右にオフセット
                let ta_x      = inner.x + 7;
                let ta_width  = inner.width.saturating_sub(7);
                let ta_area   = Rect { x: ta_x, y: inner.y + row_in_inner, width: ta_width, height: 1 };

                // textareaのスタイルをMonokaiに合わせる
                app.textarea.set_style(Style::default().fg(BG).bg(CURSOR_INSERT));
                app.textarea.set_cursor_style(Style::default().fg(CYAN).bg(BG).underlined());
                app.textarea.set_block(Block::default()); // ボーダーなし

                f.render_widget(&app.textarea, ta_area);
            }
        }
    }
}

pub(super) fn render_tab_bar(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focused;
    let spans: Vec<Span> = (0..app.tabs.len())
        .map(|i| {
            let label = format!(" {} ", i + 1);
            if i == app.active_tab {
                if focused {
                    Span::styled(label, Style::default().fg(BG).bg(YELLOW).bold())
                } else {
                    Span::styled(label, Style::default().fg(DIM).bg(BG))
                }
            } else {
                Span::styled(label, Style::default().fg(DIM).bg(BG))
            }
        })
        .collect();
    let line = Line::from(spans);
    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(BG)),
        area,
    );
}

pub(super) fn render_status(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focused;

    // コマンドモードは独自の表示
    if app.mode == Mode::Command {
        let cmd_display = format!(":{}", app.command_buf);
        let hint = "Enter:execute  Esc:cancel";
        let hint_width = hint.len() as u16 + 1;
        let cols = Layout::horizontal([
            Constraint::Min(0),
            Constraint::Length(hint_width),
        ]).split(area);
        f.render_widget(
            Paragraph::new(cmd_display)
                .style(Style::default().fg(if focused { YELLOW } else { DIM }).bg(BG)),
            cols[0],
        );
        f.render_widget(
            Paragraph::new(hint)
                .style(Style::default().fg(DIM).bg(BG))
                .alignment(Alignment::Right),
            cols[1],
        );
        return;
    }

    let hint = match app.mode {
        Mode::Normal => "j/k/Enter:move  i:edit  o/O:newline  dd:delete  p/P:paste  \"+p/\"+P:clip-paste  zm/zr:fold  Space:play  v:intonation  h:help  l:tab-next  q:quit",
        Mode::Insert => "^A:home  ^E:end  ^K:kill  ^W:del-word  Esc/Enter:confirm",
        Mode::Command => "",
        Mode::Intonation => "",
        Mode::Help => "",
    };
    let hint_width = hint.len() as u16 + 1;

    let cols = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(hint_width),
    ]).split(area);

    let status_color = if !focused {
        DIM
    } else {
        match app.mode {
            Mode::Normal | Mode::Command | Mode::Help => YELLOW,
            Mode::Insert => CYAN,
            _ => YELLOW
        }
    };
    f.render_widget(
        Paragraph::new(app.status_display())
            .style(Style::default().fg(status_color).bg(BG)),
        cols[0],
    );

    // NormalモードでESCが押された直後は"q:quit"をハイライト表示する（フォーカス中のみ）
    let esc_hint_active = focused
        && app.mode == Mode::Normal
        && app.esc_hint_until
            .map(|until| until > std::time::Instant::now())
            .unwrap_or(false);

    if esc_hint_active {
        const QUIT_HINT: &str = "q:quit";
        if let Some(prefix) = hint.strip_suffix(QUIT_HINT) {
            let hint_line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(DIM).bg(BG)),
                Span::styled(QUIT_HINT, Style::default().fg(YELLOW).bg(BG).bold()),
            ]);
            f.render_widget(
                Paragraph::new(hint_line)
                    .style(Style::default().fg(DIM).bg(BG))
                    .alignment(Alignment::Right),
                cols[1],
            );
        } else {
            f.render_widget(
                Paragraph::new(hint)
                    .style(Style::default().fg(DIM).bg(BG))
                    .alignment(Alignment::Right),
                cols[1],
            );
        }
    } else {
        f.render_widget(
            Paragraph::new(hint)
                .style(Style::default().fg(DIM).bg(BG))
                .alignment(Alignment::Right),
            cols[1],
        );
    }
}
