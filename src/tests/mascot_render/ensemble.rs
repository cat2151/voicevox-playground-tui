use super::*;

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
