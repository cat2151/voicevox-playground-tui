use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, SpeakerStyleFocus};

use super::{BG, CURSOR_NORMAL, DIM, FG, YELLOW};

fn centered_sized_rect(width: u16, height: u16, r: Rect) -> Rect {
    let width = width.min(r.width).max(3);
    let height = height.min(r.height).max(3);
    Rect {
        x: r.x + r.width.saturating_sub(width) / 2,
        y: r.y + r.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn render_selection_pane(
    f: &mut Frame,
    area: Rect,
    title: &str,
    items: &[String],
    selected: usize,
    focused: bool,
) {
    let border_color = if focused { YELLOW } else { DIM };
    let title_style = if focused {
        Style::default().fg(YELLOW).bold()
    } else {
        Style::default().fg(DIM)
    };
    let item_style = Style::default().fg(if focused { FG } else { DIM }).bg(BG);
    let highlight_style = if focused {
        Style::default().fg(BG).bg(YELLOW).bold()
    } else {
        Style::default().fg(FG).bg(CURSOR_NORMAL).bold()
    };
    let content: Vec<ListItem> = if items.is_empty() {
        vec![ListItem::new("(none)")]
    } else {
        items
            .iter()
            .map(|item| ListItem::new(item.as_str()))
            .collect()
    };
    let mut state = ListState::default();
    state.select(Some(selected.min(content.len().saturating_sub(1))));
    let list = List::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(Span::styled(title, title_style))
                .style(Style::default().bg(BG)),
        )
        .style(item_style)
        .highlight_style(highlight_style);
    f.render_stateful_widget(list, area, &mut state);
}

pub(super) fn render_speaker_style_overlay(f: &mut Frame, app: &App) {
    let Some(state) = app.speaker_style_state.as_ref() else {
        return;
    };

    let speaker_items = App::speaker_style_speaker_items();
    let style_items: Vec<String> = App::speaker_style_styles(state.speaker_index)
        .iter()
        .map(|(name, _)| name.clone())
        .collect();

    let speaker_width = speaker_items
        .iter()
        .map(|name| UnicodeWidthStr::width(name.as_str()))
        .max()
        .unwrap_or(0);
    let style_width = style_items
        .iter()
        .map(|name| UnicodeWidthStr::width(name.as_str()))
        .max()
        .unwrap_or(0);
    let pane_content_width = speaker_width.max(style_width).max(8) as u16 + 2;
    let footer = "[M]:mascot  h/l:focus  j/k:select  Space/p:preview  Enter:confirm  Esc:cancel";
    let desired_width = (pane_content_width + 2)
        .saturating_mul(2)
        .saturating_add(1)
        .max(UnicodeWidthStr::width(footer) as u16)
        .saturating_add(2);
    let desired_height = speaker_items.len().max(style_items.len()).max(1) as u16 + 5;
    let area = centered_sized_rect(desired_width, desired_height, f.area());
    if area.width < 12 || area.height < 6 {
        return;
    }

    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(YELLOW))
        .title(Span::styled(
            " [SPEAKER / STYLE] ",
            Style::default().fg(YELLOW).bold(),
        ))
        .style(Style::default().bg(BG));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 3 || inner.height < 3 {
        return;
    }

    let rows = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(inner);
    let cols = Layout::horizontal([
        Constraint::Ratio(1, 2),
        Constraint::Length(1),
        Constraint::Ratio(1, 2),
    ])
    .split(rows[0]);

    render_selection_pane(
        f,
        cols[0],
        " [SPEAKER] ",
        &speaker_items,
        state.speaker_index,
        state.focus == SpeakerStyleFocus::Speaker,
    );
    render_selection_pane(
        f,
        cols[2],
        " [STYLE] ",
        &style_items,
        state.style_index,
        state.focus == SpeakerStyleFocus::Style,
    );
    f.render_widget(
        Paragraph::new(footer).style(Style::default().fg(DIM).bg(BG)),
        rows[1],
    );
}
