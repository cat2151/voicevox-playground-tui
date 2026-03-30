use super::*;
use crate::speakers;
use mascot_render_client::{
    preview_mouth_flap_timeline_request, MotionTimelineKind, PREVIEW_MOUTH_FLAP_FPS,
};
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

    let entries = mascot_psd_entries_from_cache_dir(&cache_dir);

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
