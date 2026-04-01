use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::mascot_render;

use super::{centered_rect, BG, FG, ORANGE};

pub(super) fn render_mascot_overlay(f: &mut Frame) {
    let Some((message, dismiss_with_enter)) = mascot_render::current_overlay_message() else {
        return;
    };

    let area = centered_rect(70, 20, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ORANGE))
        .title(Span::styled(
            if dismiss_with_enter {
                " [MASCOT RENDER ERROR] "
            } else {
                " [MASCOT RENDER] "
            },
            Style::default().fg(ORANGE).bold(),
        ))
        .style(Style::default().bg(BG));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if dismiss_with_enter && inner.height >= 2 {
        let footer_area = Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(1),
            width: inner.width,
            height: 1,
        };
        let content_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: inner.height.saturating_sub(1),
        };
        let paragraph = Paragraph::new(message)
            .style(Style::default().fg(FG).bg(BG))
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, content_area);
        f.render_widget(
            Paragraph::new("ENTER:閉じる").style(Style::default().fg(ORANGE).bg(BG)),
            footer_area,
        );
        return;
    }

    let paragraph = Paragraph::new(message)
        .style(Style::default().fg(FG).bg(BG))
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, inner);
}
