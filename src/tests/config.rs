use super::*;

#[test]
fn parse_config_toml_reads_voicevox_keys() {
    let config = parse_config_toml(
        r#"
voicevox_path = "/opt/voicevox"
voicevox_nemo_path = "/opt/voicevox-nemo"
"#,
    )
    .unwrap();
    assert_eq!(config.voicevox_path, Some(PathBuf::from("/opt/voicevox")));
    assert_eq!(
        config.voicevox_nemo_path,
        Some(PathBuf::from("/opt/voicevox-nemo"))
    );
    assert!(!config.mascot_render_snapshot_log);
    assert!(!config.deprecated_mascot_render_server_path_present);
}

#[test]
fn parse_config_toml_supports_single_quoted_paths() {
    let config = parse_config_toml(
        r#"
voicevox_path = '/opt/voicevox'
voicevox_nemo_path = '/opt/voicevox-nemo'
"#,
    )
    .unwrap();
    assert_eq!(config.voicevox_path, Some(PathBuf::from("/opt/voicevox")));
    assert_eq!(
        config.voicevox_nemo_path,
        Some(PathBuf::from("/opt/voicevox-nemo"))
    );
}

#[test]
fn parse_config_toml_reads_mascot_render_snapshot_log() {
    let config = parse_config_toml(
        r#"
mascot_render_snapshot_log = true
"#,
    )
    .unwrap();

    assert!(config.mascot_render_snapshot_log);
}

#[test]
fn parse_config_toml_returns_error_for_invalid_toml() {
    let err = parse_config_toml("voicevox_path = /invalid").err();
    assert!(err.is_some());
}

#[test]
fn parse_config_toml_empty_strings_are_treated_as_none() {
    let config = parse_config_toml(
        r#"
voicevox_path = ""
voicevox_nemo_path = ""
mascot_render_server_path = ""
"#,
    )
    .unwrap();
    assert_eq!(config.voicevox_path, None);
    assert_eq!(config.voicevox_nemo_path, None);
    assert!(!config.deprecated_mascot_render_server_path_present);
}

#[test]
fn parse_config_toml_marks_deprecated_mascot_render_server_path() {
    let config = parse_config_toml(
        r#"
mascot_render_server_path = "/opt/mascot-render-server"
"#,
    )
    .unwrap();

    assert!(config.deprecated_mascot_render_server_path_present);
}
