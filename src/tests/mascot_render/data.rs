use super::*;

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
fn speaker_has_psd_matches_normalized_filename_text() {
    with_overlay_state_lock(|| {
        set_loaded_psd_file_names_for_test(&["assets\\ずんだもん-立ち絵.PSD", "White_CUL.psd"]);

        assert!(speaker_has_psd("ずんだもん"));
        assert!(speaker_has_psd("WhiteCUL"));
        assert!(!speaker_has_psd("四国めたん"));
    });
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
fn playback_snapshots_are_skipped_when_snapshot_log_is_disabled() {
    with_overlay_state_lock(|| {
        with_temp_request_log_dir(|dir| {
            set_snapshot_logging_enabled(false);
            let address = SocketAddr::from(([127, 0, 0, 1], 0));

            log_playback_snapshots(7, "timeline", "before", address, None);

            assert!(!dir.join("request.log").exists());
        });
    });
}

#[test]
fn playback_snapshots_run_when_snapshot_log_is_enabled() {
    with_overlay_state_lock(|| {
        with_temp_request_log_dir(|dir| {
            set_snapshot_logging_enabled(true);
            let address = SocketAddr::from(([127, 0, 0, 1], 0));

            log_playback_snapshots(7, "timeline", "before", address, None);

            let log = fs::read_to_string(dir.join("request.log")).unwrap();
            assert!(log.contains("sync_id=7 phase=timeline"));
            assert!(log.contains("timing=before"));
            assert!(log.contains("event=snapshot_start"));
        });
    });
}

#[test]
fn playback_error_snapshot_runs_even_when_snapshot_log_is_disabled() {
    with_overlay_state_lock(|| {
        with_temp_request_log_dir(|dir| {
            set_snapshot_logging_enabled(false);
            let address = SocketAddr::from(([127, 0, 0, 1], 0));
            let result = Err(anyhow::anyhow!("request failed"));

            log_playback_error_snapshots(7, "timeline", address, None, &result);

            let log = fs::read_to_string(dir.join("request.log")).unwrap();
            assert!(log.contains("sync_id=7 phase=timeline"));
            assert!(log.contains("timing=error"));
            assert!(log.contains("event=snapshot_start"));
        });
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
