use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::mascot_render;

use super::{centered_rect, BG, CYAN, FG, ORANGE};

fn top_overlay_rect(r: Rect, message: &str) -> Rect {
    let text_width = message
        .lines()
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0) as u16;
    let text_height = message.lines().count().max(1) as u16;
    let width = (text_width + 4).min(r.width).max(3);
    let height = (text_height + 2).min(r.height).max(3);
    let x = r.x + r.width.saturating_sub(width) / 2;
    Rect {
        x,
        y: r.y,
        width,
        height,
    }
}

pub(super) fn render_startup_overlay(f: &mut Frame) {
    if f.area().width < 3 || f.area().height < 3 {
        return;
    }
    let Some(message) = mascot_render::current_startup_overlay_message() else {
        return;
    };

    let area = top_overlay_rect(f.area(), &message);
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(CYAN))
        .title(Span::styled(
            " [STARTUP] ",
            Style::default().fg(CYAN).bold(),
        ))
        .style(Style::default().bg(BG));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let paragraph = Paragraph::new(message)
        .style(Style::default().fg(FG).bg(BG))
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, inner);
}

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
