use super::*;

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
