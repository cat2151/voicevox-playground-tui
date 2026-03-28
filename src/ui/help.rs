use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use std::collections::HashSet;

use crate::app::{App, HELP_ENTRIES};

use super::{BG, FG, DIM, YELLOW};

// ── ヘルプメニューオーバーレイ ──────────────────────────────────────────────────

/// ヘルプオーバーレイ用の中央配置Rectを計算するヘルパー。
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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

/// ヘルプメニューを画面中央にオーバーレイ表示する。
/// 2列でNORMALモードのkeybindを一覧表示し、キー入力で前方一致ハイライト/完全一致で実行、ESCで閉じる。
pub(super) fn render_help_overlay(f: &mut Frame, app: &App) {
    let area = centered_rect(80, 75, f.area());

    // 背景をクリアしてポップアップを描画
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(YELLOW))
        .title(Span::styled(" [HELP] ", Style::default().fg(YELLOW).bold()))
        .style(Style::default().bg(BG));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 2 { return; }

    // フッターのヒント行を1行確保
    let footer_area = Rect {
        x:      inner.x,
        y:      inner.y + inner.height.saturating_sub(1),
        width:  inner.width,
        height: 1,
    };
    let content_area = Rect {
        x:      inner.x,
        y:      inner.y,
        width:  inner.width,
        height: inner.height.saturating_sub(1),
    };

    // フッター
    let footer = "キーを入力してハイライト/実行  ESC:閉じる";
    f.render_widget(
        Paragraph::new(footer).style(Style::default().fg(DIM).bg(BG)),
        footer_area,
    );

    // エントリを2列で並べる
    let n = HELP_ENTRIES.len();

    // キー列の表示幅（固定）
    let key_w = 13usize;

    // 利用可能なコンテンツ幅に基づいて、desc列の最大表示幅を計算する
    // 1 行あたりの構成:
    //   key_w + 1(スペース) + left_desc + 3(セパレータ) + key_w + 1(スペース) + right_desc
    let total_fixed: u16 = (key_w + 1 + 3 + key_w + 1) as u16;
    let avail_desc_total: u16 = content_area.width.saturating_sub(total_fixed);
    // 左右それぞれに割り当てる desc の最大幅（端数は切り捨て）
    let per_col_desc_max: usize = (avail_desc_total as usize) / 2;

    // 左列・右列それぞれの自然な最大desc表示幅を計算し、ターミナル幅に合わせて上限をかける
    let natural_left_desc_w = (0..n).step_by(2)
        .map(|i| UnicodeWidthStr::width(HELP_ENTRIES[i].desc))
        .max()
        .unwrap_or(0);
    let natural_right_desc_w = (1..n).step_by(2)
        .map(|i| UnicodeWidthStr::width(HELP_ENTRIES[i].desc))
        .max()
        .unwrap_or(0);

    let max_left_desc_w = natural_left_desc_w.min(per_col_desc_max);
    let max_right_desc_w = natural_right_desc_w.min(per_col_desc_max);

    let matching: HashSet<usize> = app.help_matching_indices().into_iter().collect();

    let items: Vec<ListItem> = (0..n).step_by(2).flat_map(|row_start| {
        let left_idx  = row_start;
        let right_idx = row_start + 1;

        // 左列エントリ
        let left_selected  = matching.contains(&left_idx);
        let right_selected = right_idx < n && matching.contains(&right_idx);

        let left_entry  = &HELP_ENTRIES[left_idx];
        let right_entry = HELP_ENTRIES.get(right_idx);

        // 左列スパン
        let left_key_style = if left_selected {
            Style::default().fg(BG).bg(YELLOW).bold()
        } else {
            Style::default().fg(YELLOW)
        };
        let left_desc_style = if left_selected {
            Style::default().fg(BG).bg(YELLOW)
        } else {
            Style::default().fg(FG)
        };

        // UnicodeWidthStr で実際の表示幅を計算してスペースパディングを追加する
        let left_key_display_w  = UnicodeWidthStr::width(left_entry.key);
        let left_desc_display_w = UnicodeWidthStr::width(left_entry.desc);
        let left_key  = format!("{}{}", left_entry.key,  " ".repeat(key_w.saturating_sub(left_key_display_w)));
        let left_desc = format!("{}{}", left_entry.desc, " ".repeat(max_left_desc_w.saturating_sub(left_desc_display_w)));

        let mut spans = vec![
            Span::styled(left_key,  left_key_style),
            Span::styled(" ", Style::default().bg(BG)),
            Span::styled(left_desc, left_desc_style),
            Span::styled("   ", Style::default().bg(BG)), // 左右列の間を3桁固定
        ];

        // 右列スパン
        if let Some(right_entry) = right_entry {
            let right_key_style = if right_selected {
                Style::default().fg(BG).bg(YELLOW).bold()
            } else {
                Style::default().fg(YELLOW)
            };
            let right_desc_style = if right_selected {
                Style::default().fg(BG).bg(YELLOW)
            } else {
                Style::default().fg(FG)
            };

            let right_key  = format!("{}{}", right_entry.key,  " ".repeat(key_w.saturating_sub(UnicodeWidthStr::width(right_entry.key))));
            let right_desc = format!("{}{}", right_entry.desc, " ".repeat(max_right_desc_w.saturating_sub(UnicodeWidthStr::width(right_entry.desc))));

            spans.push(Span::styled(right_key,  right_key_style));
            spans.push(Span::styled(" ", Style::default().bg(BG)));
            spans.push(Span::styled(right_desc, right_desc_style));
        }

        vec![ListItem::new(Line::from(spans))]
    }).collect();

    let list = List::new(items).style(Style::default().bg(BG));
    f.render_widget(list, content_area);
}
