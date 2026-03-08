//! ratatuiによる描画ロジック。Monokai配色。

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Mode};

// ── Monokai パレット ───────────────────────────────────────────────────────────
const BG:           Color = Color::Rgb(39, 40, 34);
const FG:           Color = Color::Rgb(248, 248, 242);
const DIM:          Color = Color::Rgb(117, 113, 94);
const YELLOW:       Color = Color::Rgb(230, 219, 116);
const GREEN:        Color = Color::Rgb(166, 226, 46);
const CYAN:         Color = Color::Rgb(102, 217, 232);
const ORANGE:       Color = Color::Rgb(253, 151, 31);
const CURSOR_NORMAL:Color = Color::Rgb(73, 72, 62);
const CURSOR_INSERT:Color = Color::Rgb(102, 217, 232);

pub fn draw(f: &mut Frame, app: &mut App) {
    f.render_widget(Block::default().style(Style::default().bg(BG)), f.area());

    // イントネーション編集モードは専用レイアウト
    if app.mode == Mode::Intonation {
        let show_tabbar = app.tabs.len() > 1;
        let chunks = if show_tabbar {
            Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(1),
            ]).split(f.area())
        } else {
            Layout::vertical([
                Constraint::Min(3),
                Constraint::Length(1),
            ]).split(f.area())
        };
        if show_tabbar {
            render_tab_bar(f, app, chunks[0]);
            render_intonation_editor(f, app, chunks[1]);
            render_intonation_status(f, app, chunks[2]);
        } else {
            render_intonation_editor(f, app, chunks[0]);
            render_intonation_status(f, app, chunks[1]);
        }
        return;
    }

    let show_tabbar = app.tabs.len() > 1;

    let chunks = if show_tabbar {
        Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(f.area())
    } else {
        Layout::vertical([
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(f.area())
    };

    if show_tabbar {
        app.visible_lines = (chunks[1].height as usize).saturating_sub(2);
        render_tab_bar(f, app, chunks[0]);
        render_lines(f, app, chunks[1]);
        render_status(f, app, chunks[2]);
    } else {
        app.visible_lines = (chunks[0].height as usize).saturating_sub(2);
        render_lines(f, app, chunks[0]);
        render_status(f, app, chunks[1]);
    }

    // (アップデートダイアログは廃止)
}

fn render_lines(f: &mut Frame, app: &mut App, area: Rect) {

    let cursor_bg = match app.mode {
        Mode::Normal | Mode::Command => CURSOR_NORMAL,
        Mode::Insert => CURSOR_INSERT,
        _ => CURSOR_NORMAL
    };
    let cursor_fg = match app.mode {
        Mode::Normal | Mode::Command => FG,
        Mode::Insert => BG,
        _ => FG
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
        let intonation_mark = if app.intonation_cache.contains(line.as_str()) { "♬ " } else { "  " };

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
        Mode::Normal | Mode::Command => Span::styled(" [NORMAL] ", Style::default().fg(GREEN).bold()),
        Mode::Insert => Span::styled(" [INSERT] ", Style::default().fg(CYAN).bold()),
        _ => Span::styled(" [NORMAL] ", Style::default().fg(GREEN).bold()),
    };
    let border_color = match app.mode {
        Mode::Normal | Mode::Command => DIM,
        Mode::Insert => CYAN,
        _ => DIM
    };

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
    state.select(Some(visible_cursor));
    f.render_stateful_widget(list, area, &mut state);

    // Insertモード: カーソル行にtextareaを重ねて描画する
    if app.mode == Mode::Insert {
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

fn render_tab_bar(f: &mut Frame, app: &App, area: Rect) {
    let spans: Vec<Span> = (0..app.tabs.len())
        .map(|i| {
            let label = format!(" {} ", i + 1);
            if i == app.active_tab {
                Span::styled(label, Style::default().fg(BG).bg(YELLOW).bold())
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

fn render_status(f: &mut Frame, app: &mut App, area: Rect) {
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
                .style(Style::default().fg(YELLOW).bg(BG)),
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
        Mode::Normal => "j/k:move  i:edit  o/O:newline  dd:delete  p/P:paste  \"+p/\"+P:clip-paste  zm/zr:fold  Space/Enter:play  v:intonation  q:quit",
        Mode::Insert => "^A:home  ^E:end  ^K:kill  ^W:del-word  Esc/Enter:confirm",
        Mode::Command => "",
        Mode::Intonation => "",
    };
    let hint_width = hint.len() as u16 + 1;

    let cols = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(hint_width),
    ]).split(area);

    let status_color = match app.mode {
        Mode::Normal | Mode::Command => YELLOW,
        Mode::Insert => CYAN,
        _ => YELLOW
    };
    f.render_widget(
        Paragraph::new(app.status_display())
            .style(Style::default().fg(status_color).bg(BG)),
        cols[0],
    );

    // NormalモードでESCが押された直後は"q:quit"をハイライト表示する
    let esc_hint_active = app.mode == Mode::Normal
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

// ── イントネーション編集モード ──────────────────────────────────────────────────

/// イントネーション編集モードのメイン画面を描画する。
/// レイアウト（ブロック内）:
///   1行目: モードラベル
///   2行目: 現在行のテキスト
///   3行目: モーラ一覧（space区切り、選択モーラをハイライト）
///   4行目: pitch一覧（小数1桁、選択モーラをハイライト）
///   5行目: 数値直接入力バッファ（入力中のみ）
fn render_intonation_editor(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ORANGE))
        .title(Span::styled(" [INTONATION] ", Style::default().fg(ORANGE).bold()))
        .style(Style::default().bg(BG));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height == 0 { return; }

    let rows = Layout::vertical([
        Constraint::Length(1), // モードラベル
        Constraint::Length(1), // 本文
        Constraint::Length(1), // モーラ一覧
        Constraint::Length(1), // pitch一覧
        Constraint::Min(0),    // 数値入力バッファ（余白兼用）
    ]).split(inner);

    // 1行目: モードラベル
    f.render_widget(
        Paragraph::new("イントネーション編集モード  (a-z:+0.1  A-Z:-0.1  0-9:直接入力  Esc/Enter:確定)")
            .style(Style::default().fg(ORANGE).bold()),
        rows[0],
    );

    // 2行目: 現在行のテキスト
    let line_text = app.lines.get(app.cursor).cloned().unwrap_or_default();
    f.render_widget(
        Paragraph::new(line_text).style(Style::default().fg(FG)),
        rows[1],
    );

    // 3行目: モーラ一覧
    let mora_spans: Vec<Span> = app.intonation_mora_texts.iter().enumerate()
        .flat_map(|(i, text)| {
            let style = if i == app.intonation_cursor {
                Style::default().fg(BG).bg(CYAN).bold()
            } else {
                Style::default().fg(FG)
            };
            let sep = if i + 1 < app.intonation_mora_texts.len() { " " } else { "" };
            let label = format!("{}{}", text, sep);
            [Span::styled(label, style)]
        })
        .collect();
    f.render_widget(
        Paragraph::new(Line::from(mora_spans)).style(Style::default().bg(BG)),
        rows[2],
    );

    // 4行目: pitch一覧
    let pitch_spans: Vec<Span> = app.intonation_pitches.iter().enumerate()
        .flat_map(|(i, &pitch)| {
            let style = if i == app.intonation_cursor {
                Style::default().fg(BG).bg(YELLOW).bold()
            } else {
                Style::default().fg(GREEN)
            };
            let sep = if i + 1 < app.intonation_pitches.len() { " " } else { "" };
            let label = format!("{:.1}{}", pitch, sep);
            [Span::styled(label, style)]
        })
        .collect();
    f.render_widget(
        Paragraph::new(Line::from(pitch_spans)).style(Style::default().bg(BG)),
        rows[3],
    );

    // 5行目: 数値直接入力バッファ（入力中のみ表示）
    if !app.intonation_num_buf.is_empty() {
        let display = format!("pitch直接入力: {}_", app.intonation_num_buf);
        f.render_widget(
            Paragraph::new(display).style(Style::default().fg(CYAN).bold()),
            rows[4],
        );
    }
}

/// イントネーション編集モードのステータスバーを描画する。
fn render_intonation_status(f: &mut Frame, app: &App, area: Rect) {
    let hint = "a-z:mora pitch+0.1  A-Z:pitch-0.1  0-9:直接入力  Esc/Enter:確定してNormalへ";
    let hint_width = UnicodeWidthStr::width(hint) as u16 + 1;
    let cols = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(hint_width),
    ]).split(area);
    f.render_widget(
        Paragraph::new(app.status_display()).style(Style::default().fg(ORANGE).bg(BG)),
        cols[0],
    );
    f.render_widget(
        Paragraph::new(hint)
            .style(Style::default().fg(DIM).bg(BG))
            .alignment(Alignment::Right),
        cols[1],
    );
}
