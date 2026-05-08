use super::*;

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
fn startup_overlay_message_stays_visible_until_cleared() {
    crate::mascot_render::with_overlay_state_lock(|| {
        set_startup_overlay_message("checking mascot-render-server".to_string());

        assert_eq!(
            current_startup_overlay_message(),
            Some("checking mascot-render-server".to_string())
        );

        clear_startup_overlay_message();

        assert_eq!(current_startup_overlay_message(), None);
    });
}

#[test]
fn startup_in_progress_flag_round_trips() {
    crate::mascot_render::with_overlay_state_lock(|| {
        assert!(!is_startup_in_progress());

        set_startup_in_progress(true);
        assert!(is_startup_in_progress());
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
fn log_mascot_sync_request_result_writes_sync_context() {
    crate::mascot_render::with_overlay_state_lock(|| {
        with_temp_request_log_dir(|dir| {
            let address = SocketAddr::from(([127, 0, 0, 1], 62152));
            let request = format_mascot_request("POST", "/show", address, None);
            let result = Ok(());

            log_mascot_sync_request_result(42, "show", "表示", address, &request, &result).unwrap();

            let log = fs::read_to_string(dir.join("request.log")).unwrap();
            assert!(log.contains("sync_id=42 phase=show"));
            assert!(log.contains("port 62152 に 表示request を送信しました。"));
            assert!(log.contains("POST /show HTTP/1.1"));
        });
    });
}

#[test]
fn log_mascot_sync_snapshots_logs_fetch_failures_without_stopping() {
    crate::mascot_render::with_overlay_state_lock(|| {
        with_temp_request_log_dir(|dir| {
            let address = SocketAddr::from(([127, 0, 0, 1], 0));

            log_mascot_sync_snapshots(7, "timeline", "before", address).unwrap();

            let log = fs::read_to_string(dir.join("request.log")).unwrap();
            assert!(log.contains("sync_id=7 phase=timeline"));
            assert!(log.contains("before /status snapshot を port 0 から取得できませんでした"));
            assert!(log.contains(
                "before /placement/anchor-plan snapshot を port 0 から取得できませんでした"
            ));
            assert!(log.contains("GET /status HTTP/1.1"));
            assert!(log.contains("GET /placement/anchor-plan HTTP/1.1"));
        });
    });
}

#[test]
fn next_mascot_sync_id_is_monotonic() {
    let first = next_mascot_sync_id();
    let second = next_mascot_sync_id();

    assert_eq!(second, first + 1);
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
