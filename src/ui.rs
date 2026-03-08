//! ratatuiによる描画ロジック。Monokai配色。

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Mode, HELP_ENTRIES};

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

/// イントネーション編集の列カラー（隣接列を異なる色にする）
fn column_color(i: usize) -> Color {
    if i % 2 == 0 { GREEN } else { YELLOW }
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

    // ヘルプモードはノーマルレイアウトの上にオーバーレイ表示する
    if app.mode == Mode::Help {
        render_help_overlay(f, app);
    }

}

fn render_lines(f: &mut Frame, app: &mut App, area: Rect) {

    let cursor_bg = match app.mode {
        Mode::Normal | Mode::Command | Mode::Help => CURSOR_NORMAL,
        Mode::Insert => CURSOR_INSERT,
        _ => CURSOR_NORMAL
    };
    let cursor_fg = match app.mode {
        Mode::Normal | Mode::Command | Mode::Help => FG,
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
        Mode::Normal | Mode::Command | Mode::Help => Span::styled(" [NORMAL] ", Style::default().fg(GREEN).bold()),
        Mode::Insert => Span::styled(" [INSERT] ", Style::default().fg(CYAN).bold()),
        _ => Span::styled(" [NORMAL] ", Style::default().fg(GREEN).bold()),
    };
    let border_color = match app.mode {
        Mode::Normal | Mode::Command | Mode::Help => DIM,
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
        Mode::Normal => "j/k:move  i:edit  o/O:newline  dd:delete  p/P:paste  \"+p/\"+P:clip-paste  zm/zr:fold  Space/Enter:play  v:intonation  h:help  l:tab-next  q:quit",
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

    let status_color = match app.mode {
        Mode::Normal | Mode::Command | Mode::Help => YELLOW,
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
fn render_intonation_editor(f: &mut Frame, app: &mut App, area: Rect) {
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
fn render_intonation_status(f: &mut Frame, app: &App, area: Rect) {
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
/// 2列でNORMALモードのkeybindを一覧表示し、hjklで移動、Space/Enterで実行、ESCで閉じる。
fn render_help_overlay(f: &mut Frame, app: &App) {
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

    let matching = app.help_matching_indices();

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
