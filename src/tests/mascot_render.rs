use super::*;
use crate::speakers;
use mascot_render_client::{
    preview_mouth_flap_timeline_request, ChangeSkinRequest, MotionTimelineKind,
    PREVIEW_MOUTH_FLAP_FPS,
};
use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

fn with_data_root_env<T>(value: Option<OsString>, f: impl FnOnce() -> T) -> T {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct EnvGuard {
        original: Option<OsString>,
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.original.as_ref() {
                Some(value) => std::env::set_var(DATA_ROOT_ENV, value),
                None => std::env::remove_var(DATA_ROOT_ENV),
            }
        }
    }

    let _mutex_guard = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let original = std::env::var_os(DATA_ROOT_ENV);
    match value.as_ref() {
        Some(value) => std::env::set_var(DATA_ROOT_ENV, value),
        None => std::env::remove_var(DATA_ROOT_ENV),
    }
    let _env_guard = EnvGuard { original };

    f()
}

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
fn mascot_psd_entries_from_cache_dir_reads_rendered_pngs() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let cache_dir = std::env::temp_dir().join(format!("vpt-mascot-cache-{unique}"));
    let meta_dir = cache_dir.join("demo");
    let png = cache_dir.join("zundamon-front.png");
    fs::create_dir_all(&meta_dir).unwrap();
    fs::write(&png, []).unwrap();
    fs::write(
        meta_dir.join("psd-meta.json"),
        format!(
            r#"{{
  "psds": [
    {{
      "file_name": "ずんだもん-front.psd",
      "path": "characters/ずんだもん-front.psd",
      "rendered_png_path": "{}"
    }},
    {{
      "file_name": "四国めたん.psd",
      "path": "characters/四国めたん.psd",
      "rendered_png_path": null
    }}
  ]
}}"#,
            png.display()
        ),
    )
    .unwrap();

    let entries = mascot_psd_list_from_cache_dir(&cache_dir).entries;

    assert_eq!(entries.len(), 2);
    let zundamon = entries
        .iter()
        .find(|entry| entry.psd_label == "characters/ずんだもん-front.psd")
        .unwrap();
    let metan = entries
        .iter()
        .find(|entry| entry.psd_label == "characters/四国めたん.psd")
        .unwrap();
    assert_eq!(zundamon.png_path, Some(png.clone()));
    assert_eq!(metan.png_path, None);

    let _ = fs::remove_file(png);
    let _ = fs::remove_dir_all(cache_dir);
}

#[test]
fn matching_skin_path_selects_a_matching_png() {
    let entries = vec![
        MascotPsdEntry {
            psd_label: "characters/ずんだもん-front.psd".to_string(),
            png_path: Some(PathBuf::from("/tmp/first.png")),
        },
        MascotPsdEntry {
            psd_label: "characters/ずんだもん-back.psd".to_string(),
            png_path: Some(PathBuf::from("/tmp/second.png")),
        },
        MascotPsdEntry {
            psd_label: "characters/四国めたん.psd".to_string(),
            png_path: Some(PathBuf::from("/tmp/metan.png")),
        },
    ];

    let selected = matching_skin_path("ずんだもん", &entries);

    assert!(matches!(
        selected.as_deref(),
        Some(path) if path == Path::new("/tmp/first.png") || path == Path::new("/tmp/second.png")
    ));
}

#[test]
fn no_matching_skin_message_includes_speaker_and_psd_list() {
    let message = no_matching_skin_message(
        "春日部つむぎ",
        &[
            MascotPsdEntry {
                psd_label: "characters/ずんだもん.psd".to_string(),
                png_path: Some(PathBuf::from("/tmp/zundamon.png")),
            },
            MascotPsdEntry {
                psd_label: "characters/四国めたん.psd".to_string(),
                png_path: Some(PathBuf::from("/tmp/metan.png")),
            },
        ],
    );

    assert!(message.contains("speaker:春日部つむぎ"));
    assert!(message.contains("characters/ずんだもん.psd"));
    assert!(message.contains("characters/四国めたん.psd"));
}

#[test]
fn mascot_psd_list_from_missing_cache_dir_reports_reason() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let missing = std::env::temp_dir().join(format!("vpt-missing-cache-{unique}"));

    let list = mascot_psd_list_from_cache_dir(&missing);

    assert!(list.entries.is_empty());
    assert!(list
        .load_reason
        .as_deref()
        .is_some_and(|reason| reason.contains("cache path could not be read")));
    assert!(no_matching_skin_message_for_list("春日部つむぎ", &list)
        .contains(&missing.display().to_string()));
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
    let body = ChangeSkinRequest {
        png_path: PathBuf::from("/tmp/metan.png"),
    };

    let request = format_mascot_json_request("POST", "/change-skin", address, &body);

    let compact_body = serde_json::to_vec(&body).unwrap();
    assert!(request.contains("header:"));
    assert!(request.contains("  POST /change-skin HTTP/1.1"));
    assert!(request.contains("  Host: 127.0.0.1:62152"));
    assert!(request.contains(&format!("  Content-Length: {}", compact_body.len())));
    assert!(request.contains("  Content-Type: application/json"));
    assert!(request.contains("body:"));
    assert!(request.contains("  {"));
    assert!(request.contains(r#"    "png_path": "/tmp/metan.png""#));
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
    clear_overlay_message();

    set_blocking_overlay_message("request failed".to_string());

    assert_eq!(
        current_overlay_message(),
        Some(("request failed".to_string(), true))
    );
    assert!(has_blocking_overlay_message());

    dismiss_blocking_overlay_message();

    assert_eq!(current_overlay_message(), None);
    assert!(!has_blocking_overlay_message());
}

#[test]
fn non_blocking_overlay_does_not_replace_blocking_overlay() {
    clear_overlay_message();

    set_blocking_overlay_message("request failed".to_string());
    set_overlay_message("temporary info".to_string());

    assert_eq!(
        current_overlay_message(),
        Some(("request failed".to_string(), true))
    );

    dismiss_blocking_overlay_message();
    assert_eq!(current_overlay_message(), None);
}

#[test]
fn clear_overlay_message_keeps_blocking_overlay_until_dismissed() {
    clear_overlay_message();

    set_blocking_overlay_message("request failed".to_string());
    clear_overlay_message();

    assert_eq!(
        current_overlay_message(),
        Some(("request failed".to_string(), true))
    );

    dismiss_blocking_overlay_message();
    assert_eq!(current_overlay_message(), None);
}

#[test]
fn log_mascot_request_result_shows_blocking_overlay_on_error() {
    clear_overlay_message();
    let address = SocketAddr::from(([127, 0, 0, 1], 62152));
    let request = format_mascot_request("POST", "/timeline", address, None);
    let result = Err(anyhow::anyhow!("connection refused"));

    log_mascot_request_result("口パク", address, &request, &result);

    let (message, dismiss_with_enter) = current_overlay_message().unwrap();
    assert!(dismiss_with_enter);
    assert!(message.contains("port 62152 への 口パクrequest 送信に失敗しました"));
    assert!(message.contains("connection refused"));
    assert!(message.contains("request:"));
    assert!(message.contains("POST /timeline HTTP/1.1"));
}
