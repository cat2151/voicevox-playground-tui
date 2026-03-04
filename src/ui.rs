//! ratatuiによる描画ロジック。Monokai配色。

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::{App, Mode};

// ── Monokai パレット ───────────────────────────────────────────────────────────
const BG:           Color = Color::Rgb(39, 40, 34);
const FG:           Color = Color::Rgb(248, 248, 242);
const DIM:          Color = Color::Rgb(117, 113, 94);
const YELLOW:       Color = Color::Rgb(230, 219, 116);
const GREEN:        Color = Color::Rgb(166, 226, 46);
const CYAN:         Color = Color::Rgb(102, 217, 232);
const CURSOR_NORMAL:Color = Color::Rgb(73, 72, 62);
const CURSOR_INSERT:Color = Color::Rgb(102, 217, 232);

pub fn draw(f: &mut Frame, app: &mut App) {
    f.render_widget(Block::default().style(Style::default().bg(BG)), f.area());

    let chunks = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(f.area());

    app.visible_lines = (chunks[0].height as usize).saturating_sub(2);

    render_lines(f, app, chunks[0]);
    render_status(f, app, chunks[1]);
}

fn render_lines(f: &mut Frame, app: &mut App, area: Rect) {


    let cursor_bg = match app.mode { Mode::Normal => CURSOR_NORMAL, Mode::Insert => CURSOR_INSERT };
    let cursor_fg = match app.mode { Mode::Normal => FG,            Mode::Insert => BG            };

    // リスト全体のRect（ボーダー内側）
    let inner = Rect {
        x:      area.x + 1,
        y:      area.y + 1,
        width:  area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let win_start = self_cursor_win_start(app.cursor, inner.height as usize, app.lines.len());

    let items: Vec<ListItem> = app.lines.iter().enumerate().map(|(i, line)| {
        let cached_mark = if app.cache.lock().unwrap().contains_key(line.as_str()) { "♪ " } else { "  " };
        let line_num = format!("{:>4}  ", i + 1);

        // Insertモードのカーソル行はtextareaが別途描画するので、プレースホルダにする
        let body = if app.mode == Mode::Insert && i == app.cursor {
            format!("{}<editing>", cached_mark)
        } else {
            format!("{}{}", cached_mark, line)
        };

        let text = Line::from(vec![
            Span::styled(line_num, Style::default().fg(DIM).bg(BG)),
            Span::styled(body,     Style::default().fg(FG).bg(BG)),
        ]);

        let style = if i == app.cursor {
            Style::default().fg(cursor_fg).bg(cursor_bg).bold()
        } else {
            Style::default().bg(BG)
        };
        ListItem::new(text).style(style)
    }).collect();

    let title = match app.mode {
        Mode::Normal => Span::styled(" ずんだもん [NORMAL] ", Style::default().fg(GREEN).bold()),
        Mode::Insert => Span::styled(" ずんだもん [INSERT] ", Style::default().fg(CYAN).bold()),
    };
    let border_color = match app.mode { Mode::Normal => DIM, Mode::Insert => CYAN };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(title)
                .style(Style::default().bg(BG)),
        )
        .highlight_symbol(">> ");

    let mut state = ListState::default();
    state.select(Some(app.cursor));
    f.render_stateful_widget(list, area, &mut state);

    // Insertモード: カーソル行にtextareaを重ねて描画する
    if app.mode == Mode::Insert {
        // スクロール後の画面上の行位置を計算する
        if app.cursor >= win_start {
            let row_in_inner = (app.cursor - win_start) as u16;
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

/// ratatuiのList自動スクロールと同じwindow startを計算する
fn self_cursor_win_start(cursor: usize, height: usize, len: usize) -> usize {
    if height == 0 || len == 0 { return 0; }
    let half      = height / 2;
    let win_start = cursor.saturating_sub(half);
    let win_end   = (win_start + height).min(len);
    win_end.saturating_sub(height)
}

fn render_status(f: &mut Frame, app: &mut App, area: Rect) {
    let hint = match app.mode {
        Mode::Normal => "j/k:move  i:edit  o/O:newline  dd:delete  p/P:paste  Space/Enter:play  q:quit",
        Mode::Insert => "^A:home  ^E:end  ^K:kill  ^W:del-word  Esc/Enter:confirm",
    };
    let hint_width = hint.len() as u16 + 1;

    let cols = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(hint_width),
    ]).split(area);

    let status_color = match app.mode { Mode::Normal => YELLOW, Mode::Insert => CYAN };
    f.render_widget(
        Paragraph::new(app.status_display())
            .style(Style::default().fg(status_color).bg(BG)),
        cols[0],
    );
    f.render_widget(
        Paragraph::new(hint)
            .style(Style::default().fg(DIM).bg(BG))
            .alignment(Alignment::Right),
        cols[1],
    );
}
