use super::test_support::{with_data_root_env, with_local_data_dir_env, with_temp_request_log_dir};
use super::*;
use crate::speakers;
use mascot_render_client::{preview_mouth_flap_timeline_request, PREVIEW_MOUTH_FLAP_FPS};
use mascot_render_protocol::{ChangeCharacterRequest, MotionTimelineKind};
use std::ffi::OsString;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn mascot_char_name_for_plain_line_uses_default_character() {
    speakers::init_test_table();
    assert_eq!(
        mascot_char_name_for_line("こんにちは"),
        Some("ずんだもん".to_string())
    );
}

#[test]
fn mascot_char_name_for_mixed_characters_returns_none() {
    speakers::init_test_table();
    assert_eq!(
        mascot_char_name_for_line("ずんだもん[四国めたん]めたん"),
        None
    );
}

#[test]
fn wav_duration_ms_reads_pcm_length() {
    let mut wav = vec![0u8; 44];
    wav[0..4].copy_from_slice(b"RIFF");
    wav[8..12].copy_from_slice(b"WAVE");
    wav[28..32].copy_from_slice(&16_000u32.to_le_bytes());
    wav[40..44].copy_from_slice(&1_600u32.to_le_bytes());
    assert_eq!(wav_duration_ms(&wav), Some(100));
}

#[test]
fn mascot_char_name_for_explicit_character_tag_uses_tagged_character() {
    speakers::init_test_table();
    assert_eq!(
        mascot_char_name_for_line("[四国めたん]こんにちは"),
        Some("四国めたん".to_string())
    );
}

#[test]
fn default_mascot_data_root_uses_local_data_dir() {
    assert_eq!(
        default_mascot_data_root(),
        dirs::data_local_dir().map(|base| base.join("mascot-render-server"))
    );
}

#[test]
fn mascot_data_root_resolves_relative_env_under_local_data_dir() {
    let relative_path = PathBuf::from("voicevox-playground-tui").join("logs");
    with_data_root_env(Some(OsString::from(&relative_path)), || {
        assert_eq!(
            mascot_data_root(),
            dirs::data_local_dir().map(|base| base.join(&relative_path))
        );
    });
}

#[test]
fn mascot_log_path_uses_app_logs_dir() {
    with_temp_request_log_dir(|log_dir| {
        assert_eq!(mascot_log_path(), Some(log_dir.join("request.log")));
    });
}

#[test]
fn with_temp_request_log_dir_cleans_up_base_dir_after_panic() {
    let mut base_dir = None;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_temp_request_log_dir(|log_dir| {
            let history_dir = log_dir.parent().expect("log dir should have a parent");
            base_dir = history_dir.parent().map(Path::to_path_buf);
            panic!("expected panic");
        });
    }));

    assert!(result.is_err());
    let base_dir = base_dir.expect("base dir should be captured");
    assert!(!base_dir.exists());
}

#[test]
fn init_data_root_env_populates_default_root_when_env_is_unset() {
    with_data_root_env(None, || {
        init_data_root_env();
        assert_eq!(
            std::env::var_os(DATA_ROOT_ENV).map(PathBuf::from),
            default_mascot_data_root()
        );
    });
}

#[test]
fn sync_character_change_sends_current_speaker_as_character_name() {
    with_overlay_state_lock(|| {
        with_temp_request_log_dir(|dir| {
            let address = SocketAddr::from(([127, 0, 0, 1], 62152));
            let mut called_with = None;

            let result = sync_character_change(address, Some("四国めたん"), |speaker| {
                called_with = Some(speaker.to_string());
                Ok(())
            });

            assert!(result);
            assert_eq!(called_with.as_deref(), Some("四国めたん"));

            let log = fs::read_to_string(dir.join("request.log")).unwrap();
            assert!(log.contains("POST /change-character HTTP/1.1"));
            assert!(log.contains(r#""character_name": "四国めたん""#));
            assert!(log.contains("四国めたん へのcharacter変更request を送信しました。"));
        });
    });
}

#[test]
fn sync_character_change_failure_sets_blocking_overlay_and_stops_timeline() {
    with_overlay_state_lock(|| {
        with_temp_request_log_dir(|dir| {
            let address = SocketAddr::from(([127, 0, 0, 1], 62152));

            let result = sync_character_change(address, Some("四国めたん"), |_| {
                Err(anyhow::anyhow!("change-character failed"))
            });

            assert!(!result);

            let (message, dismiss_with_enter) = current_overlay_message().unwrap();
            assert!(dismiss_with_enter);
            assert!(message.contains("POST /change-character HTTP/1.1"));
            assert!(message.contains(r#""character_name": "四国めたん""#));
            assert!(message.contains("change-character failed"));

            let log = fs::read_to_string(dir.join("request.log")).unwrap();
            assert!(log.contains("POST /change-character HTTP/1.1"));
            assert!(log.contains(r#""character_name": "四国めたん""#));
            assert!(log.contains("change-character failed"));

            dismiss_blocking_overlay_message();
        });
    });
}

#[test]
fn motion_timeline_request_serializes_mouth_flap_kind() {
    let body = serde_json::to_value(preview_mouth_flap_timeline_request()).unwrap();

    assert_eq!(body["steps"][0]["kind"], "mouth_flap");
}

#[test]
fn motion_timeline_request_uses_preview_mouth_flap_timing() {
    let request = motion_timeline_request(1_234);
    let preview_request = preview_mouth_flap_timeline_request();

    assert!(!preview_request.steps.is_empty());
    assert_eq!(request.steps.len(), preview_request.steps.len());
    assert!(matches!(
        request.steps[0].kind,
        MotionTimelineKind::MouthFlap
    ));
    assert_ne!(
        request.steps[0].duration_ms,
        preview_request.steps[0].duration_ms
    );
    assert_eq!(request.steps[0].duration_ms, 1_234);
    assert_eq!(request.steps[0].fps, PREVIEW_MOUTH_FLAP_FPS);
    assert_eq!(request.steps[0].kind, preview_request.steps[0].kind);
    assert_eq!(request.steps[0].fps, preview_request.steps[0].fps);
    if request.steps.len() > 1 {
        assert_eq!(&request.steps[1..], &preview_request.steps[1..]);
    }
}

#[test]
fn format_mascot_request_without_body_omits_json_sections() {
    let address = SocketAddr::from(([127, 0, 0, 1], 62152));

    let request = format_mascot_request("POST", "/show", address, None);

    assert!(request.contains("header:"));
    assert!(request.contains("  POST /show HTTP/1.1"));
    assert!(request.contains("  Host: 127.0.0.1:62152"));
    assert!(request.contains("  Connection: close"));
    assert!(request.contains("  Content-Length: 0"));
    assert!(!request.contains("Content-Type: application/json"));
    assert!(!request.contains("body:"));
}

#[test]
fn format_mascot_json_request_pretty_prints_headers_and_body() {
    let address = SocketAddr::from(([127, 0, 0, 1], 62152));
    let body = ChangeCharacterRequest {
        character_name: "四国めたん".to_string(),
    };

    let request = format_mascot_json_request("POST", "/change-character", address, &body);

    let compact_body = serde_json::to_vec(&body).unwrap();
    assert!(request.contains("header:"));
    assert!(request.contains("  POST /change-character HTTP/1.1"));
    assert!(request.contains("  Host: 127.0.0.1:62152"));
    assert!(request.contains(&format!("  Content-Length: {}", compact_body.len())));
    assert!(request.contains("  Content-Type: application/json"));
    assert!(request.contains("body:"));
    assert!(request.contains("  {"));
    assert!(request.contains(r#"    "character_name": "四国めたん""#));
    assert!(request.contains("  }"));
}

#[test]
fn format_mascot_request_uses_brackets_for_ipv6_host_header() {
    let address = SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 62152));

    let request = format_mascot_request("POST", "/show", address, None);

    assert!(request.contains("  Host: [::1]:62152"));
}

#[test]
fn current_log_timestamp_uses_human_readable_datetime_format() {
    let timestamp = current_log_timestamp();

    assert!(chrono::DateTime::parse_from_str(&timestamp, "%Y-%m-%d %H:%M:%S%:z").is_ok());
}

#[test]
fn format_mascot_log_message_prefixes_timestamp_and_category() {
    let message = format_mascot_log_message("port 62152 に 表示request を送信しました。");

    let (timestamp, rest) = message
        .strip_prefix('[')
        .and_then(|message| message.split_once("] [mascot-render] "))
        .unwrap();
    assert!(chrono::DateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S%:z").is_ok());
    assert_eq!(rest, "port 62152 に 表示request を送信しました。");
}

#[test]
fn blocking_overlay_message_stays_visible_until_dismissed() {
    crate::mascot_render::with_overlay_state_lock(|| {
        set_blocking_overlay_message("request failed".to_string());

        assert_eq!(
            current_overlay_message(),
            Some(("request failed".to_string(), true))
        );
        assert!(has_blocking_overlay_message());

        dismiss_blocking_overlay_message();

        assert_eq!(current_overlay_message(), None);
        assert!(!has_blocking_overlay_message());
    });
}

#[test]
fn non_blocking_overlay_does_not_replace_blocking_overlay() {
    crate::mascot_render::with_overlay_state_lock(|| {
        set_blocking_overlay_message("request failed".to_string());
        set_overlay_message("temporary info".to_string());

        assert_eq!(
            current_overlay_message(),
            Some(("request failed".to_string(), true))
        );

        dismiss_blocking_overlay_message();
        assert_eq!(current_overlay_message(), None);
    });
}

#[test]
fn clear_overlay_message_keeps_blocking_overlay_until_dismissed() {
    crate::mascot_render::with_overlay_state_lock(|| {
        set_blocking_overlay_message("request failed".to_string());
        clear_overlay_message();

        assert_eq!(
            current_overlay_message(),
            Some(("request failed".to_string(), true))
        );

        dismiss_blocking_overlay_message();
        assert_eq!(current_overlay_message(), None);
    });
}

#[test]
fn log_mascot_request_result_shows_blocking_overlay_on_error() {
    crate::mascot_render::with_overlay_state_lock(|| {
        with_temp_request_log_dir(|dir| {
            let address = SocketAddr::from(([127, 0, 0, 1], 62152));
            let request = format_mascot_request("POST", "/timeline", address, None);
            let result = Err(anyhow::anyhow!("connection refused"));

            log_mascot_request_result("口パク", address, &request, &result).unwrap();

            let (message, dismiss_with_enter) = current_overlay_message().unwrap();
            assert!(dismiss_with_enter);
            assert!(message.contains("port 62152 への 口パクrequest 送信に失敗しました"));
            assert!(message.contains("connection refused"));
            assert!(message.contains("request:"));
            assert!(message.contains("POST /timeline HTTP/1.1"));

            let log = fs::read_to_string(dir.join("request.log")).unwrap();
            assert!(log.contains("port 62152 への 口パクrequest 送信に失敗しました"));
            assert!(log.contains("connection refused"));
            assert!(log.contains("request:"));
            assert!(log.contains("POST /timeline HTTP/1.1"));

            dismiss_blocking_overlay_message();
        });
    });
}

#[test]
fn log_mascot_request_result_writes_success_log_to_file() {
    crate::mascot_render::with_overlay_state_lock(|| {
        with_temp_request_log_dir(|dir| {
            let address = SocketAddr::from(([127, 0, 0, 1], 62152));
            let request = format_mascot_request("POST", "/show", address, None);
            let result = Ok(());

            log_mascot_request_result("表示", address, &request, &result).unwrap();

            assert_eq!(current_overlay_message(), None);

            let log = fs::read_to_string(dir.join("request.log")).unwrap();
            assert!(log.contains("port 62152 に 表示request を送信しました。"));
            assert!(log.contains("request:"));
            assert!(log.contains("POST /show HTTP/1.1"));
        });
    });
}

#[test]
fn log_mascot_request_result_returns_error_when_log_write_fails() {
    with_overlay_state_lock(|| {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let file_path = std::env::temp_dir().join(format!("vpt-mascot-log-file-{unique}"));
        fs::write(&file_path, "occupied").unwrap();

        with_local_data_dir_env(Some(file_path.as_os_str().to_os_string()), || {
            let address = SocketAddr::from(([127, 0, 0, 1], 62152));
            let request = format_mascot_request("POST", "/show", address, None);
            let result = Ok(());

            let log_result = log_mascot_request_result("表示", address, &request, &result);

            assert!(log_result.is_err());
            assert_eq!(current_overlay_message(), None);
        });

        let _ = fs::remove_file(file_path);
    });
}

#[test]
fn log_mascot_request_result_keeps_blocking_overlay_when_log_write_fails() {
    with_overlay_state_lock(|| {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let file_path = std::env::temp_dir().join(format!("vpt-mascot-error-log-file-{unique}"));
        fs::write(&file_path, "occupied").unwrap();

        with_local_data_dir_env(Some(file_path.as_os_str().to_os_string()), || {
            let address = SocketAddr::from(([127, 0, 0, 1], 62152));
            let request = format_mascot_request("POST", "/timeline", address, None);
            let result = Err(anyhow::anyhow!("connection refused"));

            let log_result = log_mascot_request_result("口パク", address, &request, &result);

            assert!(log_result.is_err());
            let (message, dismiss_with_enter) = current_overlay_message().unwrap();
            assert!(dismiss_with_enter);
            assert!(message.contains("connection refused"));
        });

        let _ = fs::remove_file(file_path);
        dismiss_blocking_overlay_message();
    });
}
