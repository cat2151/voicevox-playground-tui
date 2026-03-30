use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::mascot_render;

use super::{centered_rect, BG, FG, ORANGE};

pub(super) fn render_mascot_overlay(f: &mut Frame) {
    let Some(message) = mascot_render::current_overlay_message() else {
        return;
    };

    let area = centered_rect(70, 20, f.area());
    f.render_widget(Clear, area);
    let paragraph = Paragraph::new(message)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ORANGE))
                .title(Span::styled(
                    " [MASCOT RENDER] ",
                    Style::default().fg(ORANGE).bold(),
                ))
                .style(Style::default().bg(BG)),
        )
        .style(Style::default().fg(FG).bg(BG))
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}
