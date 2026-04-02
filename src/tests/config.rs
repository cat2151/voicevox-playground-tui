use super::*;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn parse_config_toml_reads_voicevox_keys() {
    let config = parse_config_toml(
        r#"
voicevox_path = "/opt/voicevox"
voicevox_nemo_path = "/opt/voicevox-nemo"
mascot_render_server_path = "/opt/mascot-render-server"
"#,
    )
    .unwrap();
    assert_eq!(config.voicevox_path, Some(PathBuf::from("/opt/voicevox")));
    assert_eq!(
        config.voicevox_nemo_path,
        Some(PathBuf::from("/opt/voicevox-nemo"))
    );
    assert_eq!(
        config.mascot_render_server_path,
        Some(PathBuf::from("/opt/mascot-render-server"))
    );
}

#[test]
fn parse_config_toml_supports_single_quoted_paths() {
    let config = parse_config_toml(
        r#"
voicevox_path = '/opt/voicevox'
voicevox_nemo_path = '/opt/voicevox-nemo'
mascot_render_server_path = '/opt/mascot-render-server'
"#,
    )
    .unwrap();
    assert_eq!(config.voicevox_path, Some(PathBuf::from("/opt/voicevox")));
    assert_eq!(
        config.voicevox_nemo_path,
        Some(PathBuf::from("/opt/voicevox-nemo"))
    );
    assert_eq!(
        config.mascot_render_server_path,
        Some(PathBuf::from("/opt/mascot-render-server"))
    );
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
    assert_eq!(config.mascot_render_server_path, None);
}

#[test]
fn configured_mascot_render_executable_candidates_supports_directory_or_executable_path() {
    #[cfg(target_os = "windows")]
    let configured_path = PathBuf::from(r"C:\tools\mascot-render");
    #[cfg(not(target_os = "windows"))]
    let configured_path = PathBuf::from("/opt/mascot-render");

    let config = EngineConfig {
        mascot_render_server_path: Some(configured_path.clone()),
        ..EngineConfig::default()
    };
    let candidates = configured_mascot_render_executable_candidates(&config);

    assert_eq!(candidates[0], configured_path);
    #[cfg(target_os = "windows")]
    assert_eq!(
        candidates[1],
        PathBuf::from(r"C:\tools\mascot-render").join("mascot-render-server.exe")
    );
    #[cfg(not(target_os = "windows"))]
    assert_eq!(
        candidates[1],
        PathBuf::from("/opt/mascot-render").join("mascot-render-server")
    );
}

#[test]
fn configured_mascot_render_executable_candidates_keeps_direct_executable_path() {
    #[cfg(target_os = "windows")]
    let configured_path = PathBuf::from(r"C:\tools\mascot-render-server.exe");
    #[cfg(not(target_os = "windows"))]
    let configured_path = PathBuf::from("/opt/bin/mascot-render-server");

    let config = EngineConfig {
        mascot_render_server_path: Some(configured_path.clone()),
        ..EngineConfig::default()
    };

    assert_eq!(
        configured_mascot_render_executable_candidates(&config),
        vec![configured_path]
    );
}

#[test]
fn configured_mascot_render_executable_candidates_supports_directory_named_like_executable() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base_dir = std::env::temp_dir().join(format!("vpt-mascot-path-{unique}"));
    let configured_path = base_dir.join("mascot-render-server");
    fs::create_dir_all(&configured_path).unwrap();

    let config = EngineConfig {
        mascot_render_server_path: Some(configured_path.clone()),
        ..EngineConfig::default()
    };
    let candidates = configured_mascot_render_executable_candidates(&config);

    assert_eq!(candidates[0], configured_path);
    assert_eq!(
        candidates[1],
        candidates[0].join(MASCOT_RENDER_SERVER_EXE_NAME)
    );

    fs::remove_dir_all(base_dir).unwrap();
}
