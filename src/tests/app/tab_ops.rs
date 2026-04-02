use super::*;

#[tokio::test]
async fn apply_loaded_history_restores_tabs_and_session_state() {
    let mut app = App::new(vec![String::from("")]);
    app.pending_d = true;
    app.pending_g = true;
    app.command_buf = String::from("tabnew");
    app.help_key_buf = String::from("z");

    let all_lines = vec![
        vec![
            String::from("tab1 line1"),
            String::from("tab1 line2"),
            String::from(""),
        ],
        vec![String::from("tab2 line1"), String::from("")],
    ];
    let all_intonations = vec![vec![None; 3], vec![None; 2]];
    let session_state = crate::history::SessionState {
        active_tab: 1,
        tabs: vec![
            crate::history::TabSessionState {
                cursor: 1,
                folded: false,
            },
            crate::history::TabSessionState {
                cursor: 0,
                folded: true,
            },
        ],
    };

    app.apply_loaded_history(all_lines, all_intonations, &session_state);

    assert_eq!(app.active_tab, 1);
    assert_eq!(app.cursor, 0);
    assert!(app.folded);
    assert_eq!(
        app.lines,
        vec![String::from("tab2 line1"), String::from("")]
    );
    assert_eq!(
        app.tabs[0].0,
        vec![
            String::from("tab1 line1"),
            String::from("tab1 line2"),
            String::from("")
        ]
    );
    assert!(!app.pending_d);
    assert!(!app.pending_g);
    assert!(app.command_buf.is_empty());
    assert!(app.help_key_buf.is_empty());
}
