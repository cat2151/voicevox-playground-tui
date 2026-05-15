use super::test_support::{with_data_root_env, with_local_data_dir_env, with_temp_request_log_dir};
use super::*;
use crate::speakers;
use std::ffi::OsString;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

mod logging;
mod requests;

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

#[test]
fn vpt_ensemble_character_names_use_lines_with_text_in_order() {
    speakers::init_test_table();
    with_overlay_state_lock(|| {
        set_loaded_psd_file_names_for_test(&["ずんだもん.psd", "四国めたん.psd"]);
        let lines = vec![
            "[四国めたん]こんにちは".to_string(),
            "[ずんだもん]".to_string(),
            "ずんだもん本文".to_string(),
            "[四国めたん]もう一度".to_string(),
        ];

        assert_eq!(
            vpt_ensemble_character_names(&lines),
            vec!["四国めたん".to_string(), "ずんだもん".to_string()]
        );
    });
}

#[test]
fn vpt_ensemble_startup_posts_current_line_speakers_when_startup_mode_is_favorite() {
    speakers::init_test_table();
    with_overlay_state_lock(|| {
        with_temp_request_log_dir(|dir| {
            set_loaded_psd_file_names_for_test(&["ずんだもん.psd", "四国めたん.psd"]);
            let lines = vec![
                "[四国めたん]こんにちは".to_string(),
                "ずんだもんです".to_string(),
            ];
            let mut called_with = None;

            configure_vpt_ensemble_startup_for_mode(
                ServerEnsembleMode::Favorite,
                &lines,
                |names| {
                    called_with = Some(names.to_vec());
                    Ok(())
                },
            )
            .unwrap();

            assert_eq!(
                called_with,
                Some(vec!["四国めたん".to_string(), "ずんだもん".to_string()])
            );
            assert!(vpt_ensemble_session_active());
            let state = vpt_ensemble_session_state()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            assert_eq!(state.startup_mode, Some(ServerEnsembleMode::Favorite));
            assert!(state.restore_single_character_on_exit);

            let log = fs::read_to_string(dir.join("request.log")).unwrap();
            assert!(log.contains("POST /vpt-ensemble HTTP/1.1"));
            assert!(log.contains(r#""character_names": ["#));
            assert!(log.contains("vpt ensemble切替request を送信しました。"));
        });
    });
}

#[test]
fn vpt_ensemble_startup_updates_members_before_mode_switch() {
    speakers::init_test_table();
    with_overlay_state_lock(|| {
        set_loaded_psd_file_names_for_test(&["ずんだもん.psd", "四国めたん.psd"]);
        let lines = vec![
            "[四国めたん]こんにちは".to_string(),
            "ずんだもんです".to_string(),
        ];
        let calls = std::cell::RefCell::new(Vec::new());

        configure_vpt_ensemble_startup_for_mode_with_members(
            ServerEnsembleMode::Favorite,
            &lines,
            |names| {
                calls.borrow_mut().push(("members", names.to_vec()));
                Ok(())
            },
            |names| {
                calls.borrow_mut().push(("mode", names.to_vec()));
                Ok(())
            },
        )
        .unwrap();

        let calls = calls.into_inner();
        assert_eq!(
            calls,
            vec![
                (
                    "members",
                    vec!["四国めたん".to_string(), "ずんだもん".to_string()]
                ),
                (
                    "mode",
                    vec!["四国めたん".to_string(), "ずんだもん".to_string()]
                ),
            ]
        );
    });
}

#[test]
fn vpt_ensemble_startup_reposts_current_lines_when_startup_mode_is_vpt() {
    speakers::init_test_table();
    with_overlay_state_lock(|| {
        set_loaded_psd_file_names_for_test(&["四国めたん.psd"]);
        let lines = vec!["[四国めたん]こんにちは".to_string()];
        let mut called_with = None;

        configure_vpt_ensemble_startup_for_mode(ServerEnsembleMode::Vpt, &lines, |names| {
            called_with = Some(names.to_vec());
            Ok(())
        })
        .unwrap();

        assert_eq!(called_with, Some(vec!["四国めたん".to_string()]));
        assert!(vpt_ensemble_session_active());
        let state = vpt_ensemble_session_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert_eq!(state.startup_mode, Some(ServerEnsembleMode::Vpt));
        assert!(!state.restore_single_character_on_exit);
    });
}

#[test]
fn vpt_ensemble_startup_updates_members_without_mode_switch_for_single_character() {
    speakers::init_test_table();
    with_overlay_state_lock(|| {
        set_loaded_psd_file_names_for_test(&["四国めたん.psd"]);
        let lines = vec!["[四国めたん]こんにちは".to_string()];
        let mut members_called_with = None;
        let mut mode_called = false;

        configure_vpt_ensemble_startup_for_mode_with_members(
            ServerEnsembleMode::SingleCharacter,
            &lines,
            |names| {
                members_called_with = Some(names.to_vec());
                Ok(())
            },
            |_| {
                mode_called = true;
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(members_called_with, Some(vec!["四国めたん".to_string()]));
        assert!(!mode_called);
        assert!(!vpt_ensemble_session_active());
        let state = vpt_ensemble_session_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert_eq!(
            state.startup_mode,
            Some(ServerEnsembleMode::SingleCharacter)
        );
        assert!(!state.restore_single_character_on_exit);
    });
}

#[test]
fn server_mode_sync_updates_vpt_ensemble_session_active() {
    with_overlay_state_lock(|| {
        assert!(!vpt_ensemble_session_active());

        sync_vpt_ensemble_session_from_server_mode(ServerEnsembleMode::Vpt);
        assert!(vpt_ensemble_session_active());

        {
            let mut state = vpt_ensemble_session_state()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            state.restore_single_character_on_exit = true;
        }
        sync_vpt_ensemble_session_from_server_mode(ServerEnsembleMode::SingleCharacter);

        assert!(!vpt_ensemble_session_active());
        let state = vpt_ensemble_session_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert!(!state.restore_single_character_on_exit);
    });
}

#[test]
fn restore_vpt_ensemble_session_posts_single_character_mode_after_startup_favorite() {
    with_overlay_state_lock(|| {
        with_temp_request_log_dir(|dir| {
            {
                let mut state = vpt_ensemble_session_state()
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                state.startup_mode = Some(ServerEnsembleMode::Favorite);
                state.active = true;
                state.restore_single_character_on_exit = true;
            }
            let mut called = false;

            let restored = restore_vpt_ensemble_session_on_exit_with(|| {
                called = true;
                Ok(())
            });

            assert!(restored);
            assert!(called);
            assert!(!vpt_ensemble_session_active());
            let state = vpt_ensemble_session_state()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            assert!(!state.restore_single_character_on_exit);

            let log = fs::read_to_string(dir.join("request.log")).unwrap();
            assert!(log.contains("POST /ensemble-mode/single-character HTTP/1.1"));
            assert!(log.contains("single character mode復元request を送信しました。"));
        });
    });
}

#[test]
fn sync_character_change_sends_current_speaker_as_character_name() {
    with_overlay_state_lock(|| {
        with_temp_request_log_dir(|dir| {
            let address = SocketAddr::from(([127, 0, 0, 1], 62152));
            let mut called_with = None;
            set_loaded_psd_file_names_for_test(&["四国めたん.psd"]);

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
fn sync_character_change_posts_change_character_without_ensemble_disable() {
    with_overlay_state_lock(|| {
        with_temp_request_log_dir(|dir| {
            let address = SocketAddr::from(([127, 0, 0, 1], 62152));
            let mut called_with = None;
            set_loaded_psd_file_names_for_test(&["四国めたん.psd"]);

            let result = sync_character_change_with_context(
                None,
                None,
                address,
                Some("四国めたん"),
                |speaker| {
                    called_with = Some(speaker.to_string());
                    Ok(())
                },
            );

            assert!(result);
            assert_eq!(called_with.as_deref(), Some("四国めたん"));

            let log = fs::read_to_string(dir.join("request.log")).unwrap();
            assert!(!log.contains("POST /favorite-ensemble/disable HTTP/1.1"));
            assert!(log.contains("POST /change-character HTTP/1.1"));
        });
    });
}

#[test]
fn sync_character_change_skips_post_while_vpt_ensemble_session_is_active() {
    with_overlay_state_lock(|| {
        with_temp_request_log_dir(|dir| {
            let address = SocketAddr::from(([127, 0, 0, 1], 62152));
            let mut change_called = false;
            set_loaded_psd_file_names_for_test(&["四国めたん.psd"]);
            set_vpt_ensemble_session_active(true);

            let result = sync_character_change_with_context(
                Some(7),
                Some(Instant::now()),
                address,
                Some("四国めたん"),
                |_| {
                    change_called = true;
                    Ok(())
                },
            );

            assert!(result);
            assert!(!change_called);

            let log = fs::read_to_string(dir.join("request.log")).unwrap();
            assert!(log.contains("sync_id=7 phase=change-character"));
            assert!(log.contains("event=request_skipped"));
            assert!(log.contains("reason=vpt_ensemble_session_active"));
            assert!(!log.contains("POST /change-character HTTP/1.1"));
        });
    });
}

#[test]
fn sync_character_change_recovers_when_server_reports_vpt_ensemble_active() {
    with_overlay_state_lock(|| {
        with_temp_request_log_dir(|dir| {
            let address = SocketAddr::from(([127, 0, 0, 1], 62152));
            set_loaded_psd_file_names_for_test(&["春日部つむぎ.psd"]);

            let result = sync_character_change_with_context(
                Some(9),
                Some(Instant::now()),
                address,
                Some("春日部つむぎ"),
                |_| {
                    Err(anyhow::anyhow!(
                        "mascot-render-server request failed with HTTP 500: mascot change_character command failed while applying in the UI thread: failed to apply mascot change-character command: requested_character=春日部つむぎ: ensemble_mode=Vpt; cannot change character while ensemble mode is active"
                    ))
                },
            );

            assert!(result);
            assert!(vpt_ensemble_session_active());
            assert_eq!(current_overlay_message(), None);

            let log = fs::read_to_string(dir.join("request.log")).unwrap();
            assert!(log.contains("POST /change-character HTTP/1.1"));
            assert!(log.contains("status=error"));
            assert!(log.contains("event=request_recovered"));
            assert!(log.contains("reason=server_reported_vpt_ensemble_active"));
            assert!(!log.contains("event=snapshot_start"));
        });
    });
}

#[test]
fn sync_character_change_failure_sets_blocking_overlay_and_stops_timeline() {
    with_overlay_state_lock(|| {
        with_temp_request_log_dir(|dir| {
            let address = SocketAddr::from(([127, 0, 0, 1], 62152));
            set_loaded_psd_file_names_for_test(&["四国めたん.psd"]);

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
fn sync_character_change_skips_post_when_speaker_has_no_psd() {
    with_overlay_state_lock(|| {
        let address = SocketAddr::from(([127, 0, 0, 1], 62152));
        let mut called = false;
        set_loaded_psd_file_names_for_test(&["ずんだもん.psd"]);

        let result = sync_character_change(address, Some("四国めたん"), |_| {
            called = true;
            Ok(())
        });

        assert!(result);
        assert!(!called);
        assert_eq!(current_overlay_message(), None);
    });
}
