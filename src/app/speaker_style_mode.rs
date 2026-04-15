//! speaker/style選択オーバーレイの操作。

use crate::fetch::FetchRequest;
use crate::{mascot_render, speakers, tag};

use super::{App, Mode, SpeakerStyleFocus, SpeakerStyleState};

pub(crate) const SPEAKER_STYLE_MASCOT_MARKER: &str = " [M]";

impl App {
    pub(crate) fn speaker_style_speaker_names() -> Vec<String> {
        let (mut mascot, mut normal) = (Vec::new(), Vec::new());
        for name in &speakers::get().char_names {
            if mascot_render::speaker_has_psd(name) {
                mascot.push(name.clone());
            } else {
                normal.push(name.clone());
            }
        }
        mascot.extend(normal);
        mascot
    }

    pub(crate) fn speaker_style_speaker_items() -> Vec<String> {
        Self::speaker_style_speaker_names()
            .iter()
            .map(|name| Self::speaker_style_speaker_label(name))
            .collect()
    }

    pub(crate) fn speaker_style_speaker_label(speaker_name: &str) -> String {
        if mascot_render::speaker_has_psd(speaker_name) {
            format!("{speaker_name}{SPEAKER_STYLE_MASCOT_MARKER}")
        } else {
            speaker_name.to_string()
        }
    }

    pub(crate) fn speaker_style_styles(speaker_index: usize) -> &'static [(String, u32)] {
        let table = speakers::get();
        Self::speaker_style_speaker_names()
            .get(speaker_index)
            .and_then(|name| table.char_styles.get(name))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn speaker_style_speaker_name(speaker_index: usize) -> Option<String> {
        Self::speaker_style_speaker_names()
            .get(speaker_index)
            .cloned()
    }

    pub(crate) fn speaker_style_ctx_from_indices(
        speaker_index: usize,
        style_index: usize,
    ) -> tag::VoiceCtx {
        let table = speakers::get();
        let speaker_name = Self::speaker_style_speaker_name(speaker_index)
            .unwrap_or_else(|| table.default_char.clone());
        let styles = Self::speaker_style_styles(speaker_index);
        let (style_name, speaker_id) = styles
            .get(style_index)
            .or_else(|| styles.first())
            .cloned()
            .unwrap_or_else(|| (table.default_style.clone(), table.default_id));
        tag::VoiceCtx {
            char_name: speaker_name,
            style_name,
            speaker_id,
        }
    }

    fn speaker_style_indices_from_ctx(ctx: &tag::VoiceCtx) -> (usize, usize) {
        let speaker_index = Self::speaker_style_speaker_names()
            .iter()
            .position(|name| name == &ctx.char_name)
            .unwrap_or(0);
        let styles = Self::speaker_style_styles(speaker_index);
        let style_index = styles
            .iter()
            .position(|(_, id)| *id == ctx.speaker_id)
            .or_else(|| styles.iter().position(|(name, _)| name == &ctx.style_name))
            .unwrap_or(0);
        (speaker_index, style_index)
    }

    /// s: 現在行のspeaker/style選択オーバーレイを開く。
    pub fn enter_speaker_style_mode(&mut self) {
        self.reset_pending_prefixes();
        let original_line = self.lines.get(self.cursor).cloned().unwrap_or_default();
        let original_ctx = tag::line_head_ctx(&original_line);
        let (speaker_index, style_index) = Self::speaker_style_indices_from_ctx(&original_ctx);
        self.speaker_style_state = Some(SpeakerStyleState {
            original_line,
            previous_status_msg: self.status_msg.clone(),
            original_ctx,
            speaker_index,
            style_index,
            focus: SpeakerStyleFocus::Speaker,
        });
        self.mode = Mode::SpeakerStyle;
        self.status_msg = String::from("-- SPEAKER/STYLE --");
    }

    pub(crate) fn speaker_style_selected_ctx(&self) -> Option<tag::VoiceCtx> {
        let state = self.speaker_style_state.as_ref()?;
        Some(Self::speaker_style_ctx_from_indices(
            state.speaker_index,
            state.style_index,
        ))
    }

    pub(crate) fn speaker_style_selected_preview_line(&self) -> Option<String> {
        let state = self.speaker_style_state.as_ref()?;
        let ctx = self.speaker_style_selected_ctx()?;
        Some(tag::rewrite_line_with_ctx(&state.original_line, &ctx))
    }

    pub fn speaker_style_focus_speaker(&mut self) {
        if let Some(state) = self.speaker_style_state.as_mut() {
            state.focus = SpeakerStyleFocus::Speaker;
        }
    }

    pub fn speaker_style_focus_style(&mut self) {
        if let Some(state) = self.speaker_style_state.as_mut() {
            state.focus = SpeakerStyleFocus::Style;
        }
    }

    /// フォーカス中のspeaker/style選択を移動し、変化があった場合はプレビュー用行文字列を返す。
    pub fn speaker_style_adjust_selection(&mut self, delta: i32) -> Option<String> {
        let state = self.speaker_style_state.as_mut()?;
        let mut changed = false;

        match state.focus {
            SpeakerStyleFocus::Speaker => {
                let speaker_count = Self::speaker_style_speaker_names().len();
                if speaker_count == 0 {
                    return None;
                }
                let next = (state.speaker_index as i32 + delta).clamp(0, speaker_count as i32 - 1)
                    as usize;
                if next != state.speaker_index {
                    state.speaker_index = next;
                    state.style_index = 0;
                    changed = true;
                }
            }
            SpeakerStyleFocus::Style => {
                let style_count = Self::speaker_style_styles(state.speaker_index).len();
                if style_count == 0 {
                    return None;
                }
                let next =
                    (state.style_index as i32 + delta).clamp(0, style_count as i32 - 1) as usize;
                if next != state.style_index {
                    state.style_index = next;
                    changed = true;
                }
            }
        }

        if !changed {
            return None;
        }

        self.speaker_style_selected_preview_line()
    }

    /// 現在選択中speaker/styleのプレビューを再生する。
    pub async fn preview_speaker_style_selection(&mut self, preview_line: String) {
        let Some(ctx) = self.speaker_style_selected_ctx() else {
            return;
        };
        let body = tag::strip_known_tags(&preview_line)
            .trim_start()
            .to_string();
        let display = format!("{}{}", tag::ctx_to_explicit_prefix(&ctx), body);
        self.status_msg = format!("[preview] {display}");

        let synth_line = preview_line.trim_start().to_owned();
        if tag::parse_line(&synth_line).is_empty() {
            return;
        }

        let _ = self
            .fetch_tx
            .send(FetchRequest {
                text: synth_line,
                play_after: true,
            })
            .await;
    }

    pub fn cancel_speaker_style_mode(&mut self) {
        if let Some(state) = self.speaker_style_state.take() {
            self.status_msg = state.previous_status_msg;
        }
        self.mode = Mode::Normal;
    }

    pub fn confirm_speaker_style_mode(&mut self) {
        let Some(state) = self.speaker_style_state.take() else {
            return;
        };
        self.mode = Mode::Normal;

        let ctx = Self::speaker_style_ctx_from_indices(state.speaker_index, state.style_index);
        if ctx == state.original_ctx {
            self.status_msg = state.previous_status_msg;
            return;
        }

        let new_line = tag::rewrite_line_with_ctx(&state.original_line, &ctx);
        if self.cursor < self.line_intonations.len() {
            self.line_intonations[self.cursor] = None;
        }
        if let Some(line) = self.lines.get_mut(self.cursor) {
            *line = new_line;
        }
        self.restart_background_prefetch();
        self.status_msg = format!("[speaker/style] {}", tag::ctx_to_explicit_prefix(&ctx));
    }
}

#[cfg(test)]
#[path = "../tests/app/speaker_style_mode.rs"]
mod tests;
