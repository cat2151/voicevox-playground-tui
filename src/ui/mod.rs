//! ratatuiによる描画ロジック。Monokai配色。

use ratatui::{prelude::*, widgets::Block, Frame};

use crate::app::{App, Mode};

mod help;
mod intonation;
mod lines;
mod overlay;

pub(crate) use intonation::PITCH_PER_ROW;

// ── Monokai パレット ───────────────────────────────────────────────────────────
pub(super) const BG: Color = Color::Rgb(39, 40, 34);
pub(super) const FG: Color = Color::Rgb(248, 248, 242);
pub(super) const DIM: Color = Color::Rgb(117, 113, 94);
pub(super) const YELLOW: Color = Color::Rgb(230, 219, 116);
pub(super) const GREEN: Color = Color::Rgb(166, 226, 46);
pub(super) const CYAN: Color = Color::Rgb(102, 217, 232);
pub(super) const ORANGE: Color = Color::Rgb(253, 151, 31);
pub(super) const CURSOR_NORMAL: Color = Color::Rgb(73, 72, 62);
pub(super) const CURSOR_INSERT: Color = Color::Rgb(102, 217, 232);

pub(super) fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

/// イントネーション編集の列カラー（隣接列を異なる色にする）
pub(super) fn column_color(i: usize) -> Color {
    if i.is_multiple_of(2) {
        GREEN
    } else {
        YELLOW
    }
}

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
            ])
            .split(f.area())
        } else {
            Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(f.area())
        };
        if show_tabbar {
            lines::render_tab_bar(f, app, chunks[0]);
            intonation::render_intonation_editor(f, app, chunks[1]);
            intonation::render_intonation_status(f, app, chunks[2]);
        } else {
            intonation::render_intonation_editor(f, app, chunks[0]);
            intonation::render_intonation_status(f, app, chunks[1]);
        }
        overlay::render_mascot_overlay(f);
        return;
    }

    let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(f.area());

    app.visible_lines = (chunks[0].height as usize).saturating_sub(2);
    lines::render_lines(f, app, chunks[0]);
    lines::render_status(f, app, chunks[1]);

    // ヘルプモードはノーマルレイアウトの上にオーバーレイ表示する
    if app.mode == Mode::Help {
        help::render_help_overlay(f, app);
    }

    overlay::render_mascot_overlay(f);
}
