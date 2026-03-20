//! タグ解析・speaker_id解決モジュール。1行スコープで完結する。
//!
//! タグ記法:
//!   [キャラ名]   → そのキャラのデフォルトstyle
//!   [スタイル名] → 現在キャラの指定style
//!   [N]         → style_id = N を直接指定（例: [1]=ずんだもんあまあま、[3]=ずんだもんノーマル）

use crate::speakers;

// ── VoiceCtx ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct VoiceCtx {
    pub char_name: String,
    pub style_name: String,
    pub speaker_id: u32, // VOICEVOXのspeakerパラメータ = style_id
}

impl Default for VoiceCtx {
    fn default() -> Self {
        let t = speakers::get();
        Self {
            char_name: t.default_char.clone(),
            style_name: t.default_style.clone(),
            speaker_id: t.default_id,
        }
    }
}

impl VoiceCtx {
    /// キャラ変更: そのキャラのAPIレスポンス上の先頭styleに切り替える
    pub fn apply_char(&mut self, char_name: &str) {
        let t = speakers::get();
        if let Some(styles) = t.char_styles.get(char_name) {
            if let Some((style_name, id)) = styles.first() {
                self.char_name = char_name.to_string();
                self.style_name = style_name.clone();
                self.speaker_id = *id;
            }
        }
    }

    /// スタイル変更: 現在キャラのまま指定styleに切り替える
    pub fn apply_style(&mut self, style_name: &str) {
        let t = speakers::get();
        if let Some(id) = t.resolve_by_name(&self.char_name, style_name) {
            self.style_name = style_name.to_string();
            self.speaker_id = id;
        }
    }

    /// [N] 記法: style_id = N を直接指定
    pub fn apply_id(&mut self, id: u32) {
        let t = speakers::get();
        if let Some((char_name, style_name, _)) = t.resolve_by_id(id) {
            self.char_name = char_name;
            self.style_name = style_name;
            self.speaker_id = id;
        }
    }
}

// ── タグ分類ヘルパー ──────────────────────────────────────────────────────────

/// タグ内容を分類して処理する
enum TagKind {
    CharName(String),
    StyleName(String),
    StyleId(u32),
    Unknown(String),
}

fn classify_tag(tag: &str) -> TagKind {
    let t = speakers::get();
    if t.char_names.iter().any(|n| n == tag) {
        return TagKind::CharName(tag.to_string());
    }
    if t.style_names.iter().any(|n| n == tag) {
        return TagKind::StyleName(tag.to_string());
    }
    if let Ok(id) = tag.parse::<u32>() {
        if t.by_style_id.contains_key(&id) {
            return TagKind::StyleId(id);
        }
    }
    TagKind::Unknown(tag.to_string())
}

// ── パーサ ───────────────────────────────────────────────────────────────────

/// 1行を (text, VoiceCtx) セグメントのVecに分解する。
pub fn parse_line(line: &str) -> Vec<(String, VoiceCtx)> {
    let mut result = Vec::new();
    let mut ctx = VoiceCtx::default();
    let mut buf = String::new();
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '[' {
            buf.push(c);
            continue;
        }

        let mut inner = String::new();
        for ch in chars.by_ref() {
            if ch == ']' {
                break;
            }
            inner.push(ch);
        }

        match classify_tag(inner.trim()) {
            TagKind::Unknown(raw) => {
                buf.push('[');
                buf.push_str(&raw);
                buf.push(']');
            }
            kind => {
                // タグ前のbufをフラッシュ
                let text = buf.trim().to_string();
                if !text.is_empty() {
                    result.push((text, ctx.clone()));
                }
                buf.clear();
                match kind {
                    TagKind::CharName(n) => ctx.apply_char(&n),
                    TagKind::StyleName(n) => ctx.apply_style(&n),
                    TagKind::StyleId(id) => ctx.apply_id(id),
                    TagKind::Unknown(_) => unreachable!(),
                }
            }
        }
    }

    let text = buf.trim().to_string();
    if !text.is_empty() {
        result.push((text, ctx));
    }
    result
}

/// 行の末尾コンテキストを返す（o/O の次行初期値として使う）
pub fn tail_ctx(line: &str) -> VoiceCtx {
    let mut ctx = VoiceCtx::default();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '[' {
            continue;
        }
        let mut inner = String::new();
        for ch in chars.by_ref() {
            if ch == ']' {
                break;
            }
            inner.push(ch);
        }
        match classify_tag(inner.trim()) {
            TagKind::CharName(n) => ctx.apply_char(&n),
            TagKind::StyleName(n) => ctx.apply_style(&n),
            TagKind::StyleId(id) => ctx.apply_id(id),
            TagKind::Unknown(_) => {}
        }
    }
    ctx
}

/// VoiceCtxを行頭タグ文字列に変換する（デフォルトは出力しない）
pub fn ctx_to_prefix(ctx: &VoiceCtx) -> String {
    let t = speakers::get();
    let mut s = String::new();
    if ctx.char_name != t.default_char {
        s.push('[');
        s.push_str(&ctx.char_name);
        s.push(']');
    }
    if ctx.style_name != t.default_style {
        s.push('[');
        s.push_str(&ctx.style_name);
        s.push(']');
    }
    s
}

/// 行の途中でspeaker/styleが変わる場合、変わる箇所で行を分割して返す。
/// 変わらない場合は元の行をそのまま1要素のVecで返す（原文字列を保持する）。
///
/// 例: `ずんだもん喋る[四国めたん]めたん喋る`
///   → `["ずんだもん喋る", "[四国めたん]めたん喋る"]`
pub fn split_by_ctx_change(line: &str) -> Vec<String> {
    let segments = parse_line(line);
    if segments.len() <= 1 {
        // 分割不要の場合は元の文字列をそのまま返す（再構築による差異を避ける）
        return vec![line.to_string()];
    }
    segments
        .into_iter()
        .map(|(text, ctx)| {
            let prefix = ctx_to_prefix(&ctx);
            format!("{}{}", prefix, text)
        })
        .collect()
}

/// `[N]` タグを可読なキャラ名・スタイル名タグに展開する（commit_insert時に呼ぶ）。
///
/// 変換ルール:
///   - `[N]` → そのstyle_idに対応する `[キャラ名][スタイル名]` に展開する
///   - 展開後、その断面でのキャラ名が直前のキャラ名と同じなら `[キャラ名]` を省略する
///   - デフォルト（ずんだもんノーマル等）と同じなら両方省略する
///
/// 例: ずんだもんが現在キャラの場合、`[1]` → `[あまあま]`（キャラ名は同じなので省略）
///     四国めたんが現在キャラの場合、`[1]` → `[ずんだもん][あまあま]`
pub fn expand_id_tags(line: &str) -> String {
    let t = speakers::get();
    let mut out = String::new();
    let mut current_char = t.default_char.clone();
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '[' {
            out.push(c);
            continue;
        }

        let mut inner = String::new();
        for ch in chars.by_ref() {
            if ch == ']' {
                break;
            }
            inner.push(ch);
        }
        let tag = inner.trim();

        // 数値タグかどうか判定
        if let Ok(id) = tag.parse::<u32>() {
            if let Some((char_name, style_name)) = t.by_style_id.get(&id) {
                // [キャラ名] は直前のキャラと違う場合のみ出力
                if char_name != &current_char {
                    out.push('[');
                    out.push_str(char_name);
                    out.push(']');
                    current_char = char_name.clone();
                }
                // [スタイル名] はデフォルトスタイルでない場合のみ出力
                // ただしキャラが変わった場合はそのキャラのデフォルトstyleと比較する
                let char_default_style = t
                    .char_styles
                    .get(char_name)
                    .and_then(|v| v.first())
                    .map(|(s, _)| s.as_str())
                    .unwrap_or("");
                if style_name != char_default_style {
                    out.push('[');
                    out.push_str(style_name);
                    out.push(']');
                }
                continue;
            }
        }

        // 非数値タグ: キャラ名タグなら current_char を更新する
        if t.char_names.iter().any(|n| n == tag) {
            current_char = tag.to_string();
        }

        // そのまま出力
        out.push('[');
        out.push_str(&inner);
        out.push(']');
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speakers;

    fn setup() {
        speakers::init_test_table();
    }

    #[test]
    fn split_no_change_plain_text() {
        setup();
        let result = split_by_ctx_change("ずんだもん喋る");
        assert_eq!(result, vec!["ずんだもん喋る"]);
    }

    #[test]
    fn split_no_change_prefix_tags_only() {
        setup();
        // タグが先頭にだけある場合は分割されない
        let result = split_by_ctx_change("[四国めたん]めたん喋る");
        assert_eq!(result, vec!["[四国めたん]めたん喋る"]);
    }

    #[test]
    fn split_mid_line_char_change() {
        setup();
        // 行の途中でキャラが変わる場合は分割される
        let result = split_by_ctx_change("ずんだもん喋る[四国めたん]めたん喋る");
        assert_eq!(result, vec!["ずんだもん喋る", "[四国めたん]めたん喋る",]);
    }

    #[test]
    fn split_mid_line_style_change() {
        setup();
        // 行の途中でスタイルが変わる場合は分割される
        let result = split_by_ctx_change("ずんだもん喋る[あまあま]あまあま喋る");
        assert_eq!(result, vec!["ずんだもん喋る", "[あまあま]あまあま喋る",]);
    }

    #[test]
    fn split_mid_line_multiple_changes() {
        setup();
        // 複数回変わる場合は複数行に分割される
        // [ずんだもん]は先頭で現在キャラに戻り、[あまあま]はスタイル変更（バッファ空なので分割なし）
        let result =
            split_by_ctx_change("ずんだもん[四国めたん]めたん[ずんだもん][あまあま]あまあま");
        assert_eq!(
            result,
            vec!["ずんだもん", "[四国めたん]めたん", "[あまあま]あまあま",]
        );
    }
}
