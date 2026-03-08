use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;

use super::{BG, FG, DIM, ORANGE, CYAN, column_color};

// ── イントネーション編集モード ──────────────────────────────────────────────────

/// 1行あたりのpitch変化量（0.1 pitch = 1行）
pub(crate) const PITCH_PER_ROW: f64 = 0.1;
/// グラフ上端から一番高いpitchバーまでの空白行数
const TOP_MARGIN_ROWS: u16 = 5;

/// イントネーション編集モードのメイン画面を描画する。
/// レイアウト（ブロック内）:
///   1行目: モードラベル
///   2行目: 現在行のテキスト
///   3行目: モーラ一覧（space区切り、選択モーラをハイライト）
///   4行目: pitch一覧（小数1桁、選択モーラをハイライト）
///   5行目: 数値直接入力バッファ（常に確保、空のときは空白）
///   残り:  擬似折れ線グラフ（0.1 = 1行、上端は最高pitchからTOP_MARGIN_ROWS行上、範囲外はグレーアウト）
pub(super) fn render_intonation_editor(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ORANGE))
        .title(Span::styled(" [INTONATION] ", Style::default().fg(ORANGE).bold()))
        .style(Style::default().bg(BG));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height == 0 { return; }

    let rows = Layout::vertical([
        Constraint::Length(1), // 0: モードラベル
        Constraint::Length(1), // 1: 本文
        Constraint::Length(1), // 2: モーラ一覧
        Constraint::Length(1), // 3: pitch一覧
        Constraint::Length(1), // 4: 数値入力バッファ（常に確保）
        Constraint::Min(0),    // 5: 擬似折れ線グラフ
    ]).split(inner);

    // 1行目: モードラベル
    f.render_widget(
        Paragraph::new("イントネーション編集モード  (a-z:+0.1  A-Z:-0.1  0-9:直接入力  マウスクリック:pitch設定  Esc/Enter:確定)")
            .style(Style::default().fg(ORANGE).bold()),
        rows[0],
    );

    // 2行目: 現在行のテキスト
    let line_text = app.lines.get(app.cursor).cloned().unwrap_or_default();
    f.render_widget(
        Paragraph::new(line_text).style(Style::default().fg(FG)),
        rows[1],
    );

    // 3行目: モーラ一覧（各列を4ターミナル列幅に統一）
    let mora_spans: Vec<Span> = app.intonation_mora_texts.iter().enumerate()
        .flat_map(|(i, text)| {
            let col = column_color(i);
            let style = if i == app.intonation_cursor {
                Style::default().fg(BG).bg(col).bold()
            } else {
                Style::default().fg(col)
            };
            let text_w = UnicodeWidthStr::width(text.as_str());
            let padding = " ".repeat(4usize.saturating_sub(text_w));
            let label = format!("{}{}", text, padding);
            [Span::styled(label, style)]
        })
        .collect();
    f.render_widget(
        Paragraph::new(Line::from(mora_spans)).style(Style::default().bg(BG)),
        rows[2],
    );

    // 4行目: pitch一覧（各列を4ターミナル列幅に統一）
    let pitch_spans: Vec<Span> = app.intonation_pitches.iter().enumerate()
        .flat_map(|(i, &pitch)| {
            let col = column_color(i);
            let style = if i == app.intonation_cursor {
                Style::default().fg(BG).bg(col).bold()
            } else {
                Style::default().fg(col)
            };
            let s = format!("{:.1}", pitch);
            let label = format!("{:<4}", s);
            [Span::styled(label, style)]
        })
        .collect();
    f.render_widget(
        Paragraph::new(Line::from(pitch_spans)).style(Style::default().bg(BG)),
        rows[3],
    );

    // 5行目: 数値直接入力バッファ（入力中のみ内容を表示）
    if !app.intonation_num_buf.is_empty() {
        let display = format!("pitch直接入力: {}_", app.intonation_num_buf);
        f.render_widget(
            Paragraph::new(display).style(Style::default().fg(CYAN).bold()),
            rows[4],
        );
    }

    // 6行目以降: 擬似折れ線グラフ
    render_intonation_graph(f, app, rows[5]);
}

/// イントネーション擬似折れ線グラフを描画する。
/// - PITCH_PER_ROW pitch = 1行
/// - 上端は最高pitchからTOP_MARGIN_ROWS行上（画面が狭い場合はgraph_h-1行上に縮小）
/// - 範囲外のモーラはグレーアウト表示
/// - グラフ情報をAppに保存してマウスイベント処理で使用する
fn render_intonation_graph(f: &mut Frame, app: &mut App, area: Rect) {
    let graph_h = area.height;
    if graph_h == 0 || app.intonation_pitches.is_empty() {
        app.intonation_graph_h = 0;
        return;
    }

    let n = app.intonation_pitches.len();
    let intonation_cursor = app.intonation_cursor;

    // pitch範囲の計算（一番高いpitchからTOP_MARGIN_ROWS行上を上端とする）
    // 画面が狭い場合（graph_h <= TOP_MARGIN_ROWS）はmarginをgraph_h-1に縮小して
    // 最高pitchが必ず表示範囲内に入るよう保証する
    let max_p = app.intonation_pitches.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let max_p = if !max_p.is_finite() { 0.0 } else { max_p };
    let margin = TOP_MARGIN_ROWS.min(graph_h.saturating_sub(1));
    let pitch_top    = max_p + margin as f64 * PITCH_PER_ROW;
    let pitch_bottom = (pitch_top - (graph_h as f64 - 1.0) * PITCH_PER_ROW).max(0.0);

    // モーラ列の幅と開始x座標を計算（全列を4ターミナル列幅に統一）
    let mut col_x: Vec<u16> = Vec::with_capacity(n);
    let mut col_w: Vec<u16> = Vec::with_capacity(n);
    let mut cx = area.x;
    for _ in 0..n {
        let w: u16 = 4;
        col_x.push(cx);
        col_w.push(w);
        cx += w;
    }

    // pitch_top を整数単位（0.1刻み）に変換して整数演算で比較する
    let pitch_top_unit    = (pitch_top    * 10.0).round() as i64;
    let pitch_bottom_unit = (pitch_bottom * 10.0).round() as i64;

    // Appにグラフ情報を保存（マウスイベント処理用）— cloneなしでmove代入
    app.intonation_graph_x         = area.x;
    app.intonation_graph_y         = area.y;
    app.intonation_graph_h         = graph_h;
    app.intonation_graph_pitch_top = pitch_top;
    app.intonation_mora_col_x      = col_x;
    app.intonation_mora_col_w      = col_w;

    // グラフの各行を描画（app.intonation_pitches と app.intonation_mora_col_w を参照）
    let mut graph_lines: Vec<Line> = Vec::with_capacity(graph_h as usize);
    for r in 0..graph_h {
        let spans: Vec<Span> = app.intonation_pitches.iter()
            .zip(&app.intonation_mora_col_w)
            .enumerate()
            .map(|(i, (&p, &col_w))| {
            let w        = col_w as usize;
            let p_unit   = (p * 10.0).round() as i64;
            let mora_row = pitch_top_unit - p_unit; // このモーラのマーカー行

            let is_out  = p_unit > pitch_top_unit || p_unit < pitch_bottom_unit;
            let is_here = mora_row == r as i64;
            let is_sel  = i == intonation_cursor;
            let col     = column_color(i);

            let (marker, style) = if is_out {
                // 範囲外モーラ: グレーアウト（現在行にかかわらず薄い点を表示）
                (format!("{:<width$}", ".", width = w), Style::default().fg(DIM))
            } else if is_here && is_sel {
                // 選択中モーラのマーカー
                (format!("{:<width$}", "*", width = w), Style::default().fg(BG).bg(col).bold())
            } else if is_here {
                // 非選択モーラのマーカー
                (format!("{:<width$}", "*", width = w), Style::default().fg(col))
            } else {
                // 空白
                (" ".repeat(w), Style::default())
            };

            // ピッチ行よりも下（マーカー未到達）に縦線（茎）を描画
            let (marker, style) = if !is_out && !is_here && mora_row >= 0 && r as i64 > mora_row {
                if is_sel {
                    (format!("{:<width$}", "|", width = w), Style::default().fg(BG).bg(col).bold())
                } else {
                    (format!("{:<width$}", "|", width = w), Style::default().fg(DIM))
                }
            } else {
                (marker, style)
            };

            Span::styled(marker, style)
        }).collect();

        graph_lines.push(Line::from(spans));
    }

    f.render_widget(
        Paragraph::new(graph_lines).style(Style::default().bg(BG)),
        area,
    );
}

/// イントネーション編集モードのステータスバーを描画する。
pub(super) fn render_intonation_status(f: &mut Frame, app: &App, area: Rect) {
    let hint = "h/← l/→:モーラ選択  k/↑:pitch+0.1 j/↓:pitch-0.1  a-z:+0.1  A-Z:-0.1  0-9:直接入力  マウスクリック:pitch設定  Esc/Enter:確定してNormalへ";
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
