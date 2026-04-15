use super::*;
use crate::app::{IntonationLineData, Mode, SpeakerStyleFocus};
use crate::speakers;

fn setup() {
    speakers::init_test_table();
}

fn make_app_with_line(line: &str) -> App {
    let mut app = App::new(vec![line.to_string()]);
    app.cursor = 0;
    app
}

#[tokio::test]
async fn enter_speaker_style_mode_uses_current_line_context() {
    crate::mascot_render::with_overlay_state_lock(|| {
        setup();
        crate::mascot_render::set_loaded_psd_file_names_for_test(&[]);
        let mut app = make_app_with_line("[四国めたん]こんにちは");

        app.enter_speaker_style_mode();

        let state = app
            .speaker_style_state
            .as_ref()
            .expect("speaker/style状態が初期化されること");
        assert_eq!(app.mode, Mode::SpeakerStyle);
        assert_eq!(state.original_ctx.char_name, "四国めたん");
        assert_eq!(state.original_ctx.style_name, "ノーマル");
        assert_eq!(state.focus, SpeakerStyleFocus::Speaker);
        assert_eq!(state.speaker_index, 1);
    });
}

#[tokio::test]
async fn speaker_change_resets_style_to_first_style_and_returns_preview_line() {
    crate::mascot_render::with_overlay_state_lock(|| {
        setup();
        crate::mascot_render::set_loaded_psd_file_names_for_test(&[]);
        let mut app = make_app_with_line("[あまあま]おはよう");
        app.enter_speaker_style_mode();

        let preview_line = app
            .speaker_style_adjust_selection(1)
            .expect("speaker変更時にプレビュー行が返ること");

        let state = app
            .speaker_style_state
            .as_ref()
            .expect("speaker/style状態が維持されること");
        assert_eq!(state.speaker_index, 1);
        assert_eq!(state.style_index, 0, "speaker変更時はstyleが先頭に戻る");
        let ctx = app
            .speaker_style_selected_ctx()
            .expect("選択中ctxを取得できること");
        assert_eq!(ctx.char_name, "四国めたん");
        assert_eq!(ctx.style_name, "ノーマル");
        assert_eq!(preview_line, "[四国めたん]おはよう");
    });
}

#[tokio::test]
async fn confirm_without_change_keeps_original_line_representation() {
    crate::mascot_render::with_overlay_state_lock(|| {
        setup();
        crate::mascot_render::set_loaded_psd_file_names_for_test(&[]);
        let mut app = make_app_with_line("[3]おはよう");
        app.status_msg = String::from("ready");
        app.enter_speaker_style_mode();

        app.confirm_speaker_style_mode();

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.lines[0], "[3]おはよう");
        assert_eq!(app.status_msg, "ready");
    });
}

#[tokio::test]
async fn confirm_change_rewrites_line_and_clears_intonation() {
    crate::mascot_render::with_overlay_state_lock(|| {
        setup();
        crate::mascot_render::set_loaded_psd_file_names_for_test(&[]);
        let mut app = make_app_with_line(" [四国めたん]おはよう[meta]");
        app.line_intonations[0] = Some(IntonationLineData {
            query: serde_json::Value::Null,
            mora_texts: vec!["お".to_string()],
            pitches: vec![5.0],
            speaker_id: 2,
        });
        app.enter_speaker_style_mode();
        let _ = app.speaker_style_adjust_selection(-1);

        app.confirm_speaker_style_mode();

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.lines[0], " おはよう[meta]");
        assert!(
            app.line_intonations[0].is_none(),
            "speaker/style変更時はイントネーション編集データを破棄する"
        );
    });
}

#[tokio::test]
async fn cancel_restores_previous_status_and_keeps_line() {
    crate::mascot_render::with_overlay_state_lock(|| {
        setup();
        crate::mascot_render::set_loaded_psd_file_names_for_test(&[]);
        let mut app = make_app_with_line("おはよう");
        app.status_msg = String::from("before");
        app.enter_speaker_style_mode();
        let original = app.lines[0].clone();

        app.cancel_speaker_style_mode();

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.status_msg, "before");
        assert_eq!(app.lines[0], original);
    });
}

#[test]
fn speaker_items_mark_only_mascot_capable_speakers() {
    crate::mascot_render::with_overlay_state_lock(|| {
        setup();
        crate::mascot_render::set_loaded_psd_file_names_for_test(&["assets\\ずんだもん立ち絵.PSD"]);

        assert_eq!(
            App::speaker_style_speaker_items(),
            vec![
                format!("ずんだもん{SPEAKER_STYLE_MASCOT_MARKER}"),
                "四国めたん".to_string(),
            ]
        );
    });
}

#[tokio::test]
async fn mascot_speakers_are_grouped_first_in_overlay_order() {
    crate::mascot_render::with_overlay_state_lock(|| {
        setup();
        crate::mascot_render::set_loaded_psd_file_names_for_test(&["四国めたん.psd"]);
        let mut app = make_app_with_line("[四国めたん]こんにちは");

        app.enter_speaker_style_mode();

        let state = app
            .speaker_style_state
            .as_ref()
            .expect("speaker/style状態が初期化されること");
        assert_eq!(state.speaker_index, 0);
        assert_eq!(
            App::speaker_style_speaker_items(),
            vec![
                format!("四国めたん{SPEAKER_STYLE_MASCOT_MARKER}"),
                "ずんだもん".to_string(),
            ]
        );
    });
}
