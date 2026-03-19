use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use serde::Deserialize;

const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Debug, Clone, Default)]
pub struct EngineConfig {
    pub voicevox_path: Option<PathBuf>,
    pub voicevox_nemo_path: Option<PathBuf>,
}

pub fn config_path() -> PathBuf {
    crate::history::history_dir().join(CONFIG_FILE_NAME)
}

pub fn load_or_create() -> Result<EngineConfig> {
    let dir = crate::history::history_dir();
    fs::create_dir_all(&dir)?;
    let path = config_path();

    if !path.exists() {
        fs::write(&path, default_config_toml())?;
        return Ok(EngineConfig::default());
    }

    let content = fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("config.toml 読み込み失敗: {} ({})", path.display(), e))?;
    parse_config_toml(&content)
        .map_err(|e| anyhow::anyhow!("config.toml のパース失敗: {} ({})", path.display(), e))
}

fn default_config_toml() -> String {
    r#"# VOICEVOX executable base paths (optional)
# voicevox_path = "<your voicevox path>"
# voicevox_nemo_path = "<your voicevox nemo path>"
"#
    .to_string()
}

fn parse_config_toml(content: &str) -> Result<EngineConfig> {
    #[derive(Deserialize)]
    struct RawConfig {
        voicevox_path: Option<String>,
        voicevox_nemo_path: Option<String>,
    }

    let raw: RawConfig = toml::from_str(content)?;
    Ok(EngineConfig {
        voicevox_path: raw.voicevox_path.filter(|s| !s.is_empty()).map(PathBuf::from),
        voicevox_nemo_path: raw
            .voicevox_nemo_path
            .filter(|s| !s.is_empty())
            .map(PathBuf::from),
    })
}

pub fn configured_executable_candidates(config: &EngineConfig) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for base in [&config.voicevox_path, &config.voicevox_nemo_path]
        .into_iter()
        .flatten()
    {
        #[cfg(target_os = "windows")]
        {
            candidates.push(base.join("vv-engine").join("run.exe"));
            candidates.push(base.join("run.exe"));
        }
        #[cfg(not(target_os = "windows"))]
        {
            candidates.push(base.join("vv-engine").join("run"));
            candidates.push(base.join("run"));
        }
    }
    candidates
}

#[cfg(test)]
mod tests {
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
"#,
        )
        .unwrap();
        assert_eq!(config.voicevox_path, None);
        assert_eq!(config.voicevox_nemo_path, None);
    }
}
