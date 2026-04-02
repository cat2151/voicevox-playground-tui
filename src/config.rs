use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use serde::Deserialize;

const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Debug, Clone, Default)]
pub struct EngineConfig {
    pub voicevox_path: Option<PathBuf>,
    pub voicevox_nemo_path: Option<PathBuf>,
    pub mascot_render_server_path: Option<PathBuf>,
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
# mascot-render-server executable path or parent directory (optional)
# mascot_render_server_path = "<your mascot-render-server path>"
"#
    .to_string()
}

fn parse_config_toml(content: &str) -> Result<EngineConfig> {
    #[derive(Deserialize)]
    struct RawConfig {
        voicevox_path: Option<String>,
        voicevox_nemo_path: Option<String>,
        mascot_render_server_path: Option<String>,
    }

    let raw: RawConfig = toml::from_str(content)?;
    Ok(EngineConfig {
        voicevox_path: raw
            .voicevox_path
            .filter(|s| !s.is_empty())
            .map(PathBuf::from),
        voicevox_nemo_path: raw
            .voicevox_nemo_path
            .filter(|s| !s.is_empty())
            .map(PathBuf::from),
        mascot_render_server_path: raw
            .mascot_render_server_path
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

#[cfg(target_os = "windows")]
pub(crate) const MASCOT_RENDER_SERVER_EXE_NAME: &str = "mascot-render-server.exe";
#[cfg(not(target_os = "windows"))]
pub(crate) const MASCOT_RENDER_SERVER_EXE_NAME: &str = "mascot-render-server";

pub fn configured_mascot_render_executable_candidates(config: &EngineConfig) -> Vec<PathBuf> {
    let Some(path) = config.mascot_render_server_path.as_ref() else {
        return Vec::new();
    };

    let mut candidates = vec![path.clone()];
    if path.is_dir() || path.file_name() != Some(OsStr::new(MASCOT_RENDER_SERVER_EXE_NAME)) {
        candidates.push(path.join(MASCOT_RENDER_SERVER_EXE_NAME));
    }
    candidates
}

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;
