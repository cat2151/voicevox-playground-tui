//! VOICEVOXエンジンの自動起動モジュール。
//! エンジンが起動していない場合にVOICEVOX実行ファイルを検索して起動し、
//! 起動完了まで待機する。

use anyhow::{Context, Result};
use mascot_render_client::{
    mascot_render_server_address, mascot_render_server_healthcheck_at,
    wait_for_mascot_render_server_healthcheck_at,
};

use crate::config::EngineConfig;

/// エンジンが応答するまで待つ最大秒数
const MAX_WAIT_SECS: u64 = 60;
const DEFAULT_VOICEVOX_URL: &str = "http://localhost:50021";

/// ポーリング間隔（ミリ秒）
const POLL_INTERVAL_MS: u64 = 1000;

/// 指定されたクライアントを使ってVOICEVOXエンジンが起動しているか確認する。
/// /speakers が 2xx を返したときのみ true を返す。
async fn check_engine_with_client(client: &reqwest::Client, base_url: &str) -> bool {
    match client.get(format!("{base_url}/speakers")).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// VOICEVOXエンジンが起動しているか確認する（/speakersへのリクエストで確認）。
pub async fn is_engine_running(base_url: &str) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    check_engine_with_client(&client, base_url).await
}

/// VOICEVOXの実行ファイルをよく使われるインストール先から探す。
/// 見つかった場合はパスを返す。
fn find_voicevox_executable(config: &EngineConfig) -> Option<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    candidates.extend(crate::config::configured_executable_candidates(config));

    #[cfg(target_os = "windows")]
    {
        // Windowsの標準インストール先
        if let Some(local_app_data) = dirs::data_local_dir() {
            candidates.push(
                local_app_data
                    .join("Programs")
                    .join("VOICEVOX")
                    .join("VOICEVOX.exe"),
            );
            // エンジン単体の実行ファイル（run.exe）も検索
            candidates.push(
                local_app_data
                    .join("Programs")
                    .join("VOICEVOX")
                    .join("run.exe"),
            );
        }
    }

    #[cfg(target_os = "macos")]
    {
        candidates.push(std::path::PathBuf::from(
            "/Applications/VOICEVOX.app/Contents/MacOS/VOICEVOX",
        ));
    }

    #[cfg(target_os = "linux")]
    {
        // Linux向けAppImage等の一般的な配置先
        if let Some(home) = dirs::home_dir() {
            candidates.push(home.join("VOICEVOX").join("run"));
            candidates.push(home.join("voicevox").join("run"));
        }
        candidates.push(std::path::PathBuf::from("/opt/VOICEVOX/run"));
        candidates.push(std::path::PathBuf::from("/opt/voicevox/run"));
    }

    candidates.into_iter().find(|p| p.is_file())
}

fn launch_detached_process(exe: &std::path::Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS: 親プロセスのコンソールから切り離す
        const DETACHED_PROCESS: u32 = 0x00000008;
        std::process::Command::new(exe)
            .creation_flags(DETACHED_PROCESS)
            .spawn()?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Unix系: stdin/stdout/stderrをnullに向けてスポーン。
        // 親プロセス終了後も子プロセスはinit/systemdに引き取られて動き続ける。
        std::process::Command::new(exe)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
    }

    Ok(())
}

/// VOICEVOXを起動する。vptが終了してもVOICEVOXは起動し続ける（デタッチドプロセス）。
fn launch_voicevox(exe: &std::path::Path) -> Result<()> {
    launch_detached_process(exe)
}

fn find_mascot_render_executable(config: &EngineConfig) -> Option<std::path::PathBuf> {
    let mut candidates = crate::config::configured_mascot_render_executable_candidates(config);

    if let Some(home) = dirs::home_dir() {
        candidates.push(
            home.join(".cargo")
                .join("bin")
                .join(crate::config::MASCOT_RENDER_SERVER_EXE_NAME),
        );
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(local_app_data) = dirs::data_local_dir() {
            candidates.push(
                local_app_data
                    .join("Programs")
                    .join("mascot-render-server")
                    .join(crate::config::MASCOT_RENDER_SERVER_EXE_NAME),
            );
        }
    }

    #[cfg(target_os = "macos")]
    {
        candidates.push(std::path::PathBuf::from(
            "/Applications/mascot-render-server.app/Contents/MacOS/mascot-render-server",
        ));
    }

    candidates.into_iter().find(|p| p.is_file())
}

fn launch_mascot_render_server(exe: &std::path::Path) -> Result<()> {
    launch_detached_process(exe)
}

async fn is_mascot_render_running() -> bool {
    let address = mascot_render_server_address();
    tokio::task::spawn_blocking(move || mascot_render_server_healthcheck_at(address).is_ok())
        .await
        .unwrap_or(false)
}

async fn wait_for_mascot_render_server() -> Result<()> {
    let address = mascot_render_server_address();
    tokio::task::spawn_blocking(move || {
        wait_for_mascot_render_server_healthcheck_at(
            address,
            std::time::Duration::from_secs(MAX_WAIT_SECS),
        )
    })
    .await?
}

fn mascot_render_check_status_message() -> String {
    format!(
        "[startup] checking mascot-render-server on port {}...",
        mascot_render_server_address().port()
    )
}

fn mascot_render_start_status_message() -> String {
    String::from("[startup] starting mascot-render-server...")
}

fn mascot_render_wait_status_message() -> String {
    format!(
        "[startup] waiting for mascot-render-server on port {}...",
        mascot_render_server_address().port()
    )
}

fn mascot_render_psd_status_message() -> String {
    String::from("[startup] fetching mascot PSD filenames...")
}

fn voicevox_start_status_message() -> String {
    String::from("[startup] starting VOICEVOX...")
}

fn voicevox_wait_status_message() -> String {
    String::from("[startup] waiting for VOICEVOX...")
}

async fn refresh_mascot_psd_file_names<F>(progress: &mut F, log_to_stderr: bool)
where
    F: FnMut(String),
{
    progress(mascot_render_psd_status_message());
    match tokio::task::spawn_blocking(
        crate::mascot_render::refresh_available_psd_file_names_from_server,
    )
    .await
    .context("mascot PSD fetch task failed")
    {
        Ok(Ok(count)) => {
            if log_to_stderr {
                eprintln!("mascot-render-server の PSD 一覧を取得しました: {count}件");
            }
        }
        Ok(Err(error)) => {
            if log_to_stderr {
                eprintln!("mascot-render-server の PSD 一覧取得に失敗しました: {error:#}");
            } else {
                crate::runtime_notice::set_runtime_notice(format!(
                    "[mascot-render] PSD 一覧取得に失敗しました: {error}"
                ));
            }
        }
        Err(error) => {
            if log_to_stderr {
                eprintln!("mascot-render-server の PSD 一覧取得に失敗しました: {error:#}");
            } else {
                crate::runtime_notice::set_runtime_notice(format!(
                    "[mascot-render] PSD 一覧取得に失敗しました: {error}"
                ));
            }
        }
    }
}

async fn ensure_mascot_render_running_impl<F>(mut progress: F, log_to_stderr: bool) -> Result<()>
where
    F: FnMut(String),
{
    progress(mascot_render_check_status_message());
    if is_mascot_render_running().await {
        return Ok(());
    }

    let config = crate::config::load_or_create()?;
    let Some(exe) = find_mascot_render_executable(&config) else {
        if log_to_stderr {
            eprintln!(
                "mascot-render-server は起動しておらず、実行ファイルも見つからなかったため自動起動をスキップします。\n\
config.toml: {}\n\
設定キー: mascot_render_server_path",
                crate::config::config_path().display()
            );
        }
        return Ok(());
    };

    if log_to_stderr {
        eprintln!("mascot-render-server を起動します: {}", exe.display());
    }
    progress(mascot_render_start_status_message());
    launch_mascot_render_server(&exe)?;

    if log_to_stderr {
        eprintln!("mascot-render-server が起動するまで待機しています...");
    }
    progress(mascot_render_wait_status_message());
    wait_for_mascot_render_server().await?;
    if log_to_stderr {
        eprintln!("mascot-render-server の起動が完了しました。");
    }

    Ok(())
}

async fn ensure_mascot_render_ready_impl<F>(mut progress: F, log_to_stderr: bool) -> Result<()>
where
    F: FnMut(String),
{
    ensure_mascot_render_running_impl(&mut progress, log_to_stderr).await?;
    if is_mascot_render_running().await {
        refresh_mascot_psd_file_names(&mut progress, log_to_stderr).await;
    }
    Ok(())
}

/// エンジンが起動するまでポーリングして待機する。
async fn wait_for_engine(base_url: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()?;
    let start = std::time::Instant::now();
    let deadline = std::time::Duration::from_secs(MAX_WAIT_SECS);
    loop {
        if check_engine_with_client(&client, base_url).await {
            return Ok(());
        }
        if start.elapsed() >= deadline {
            return Err(anyhow::anyhow!(
                "VOICEVOXエンジンの起動がタイムアウトしました（{}秒）",
                MAX_WAIT_SECS
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}

fn select_wait_url_for_engine<'a>(
    base_urls: &'a [&'a str],
    config: &EngineConfig,
    exe: &std::path::Path,
) -> &'a str {
    let primary_url = base_urls.first().copied().unwrap_or(DEFAULT_VOICEVOX_URL);
    let is_nemo_path = config
        .voicevox_nemo_path
        .as_ref()
        .is_some_and(|p| exe.starts_with(p));
    let is_voicevox_path = config
        .voicevox_path
        .as_ref()
        .is_some_and(|p| exe.starts_with(p));
    if is_nemo_path && !is_voicevox_path {
        base_urls.get(1).copied().unwrap_or(primary_url)
    } else {
        primary_url
    }
}

/// エンジンが起動していなければ自動起動し、起動完了まで待機する。
/// base_urlsのうち1つでも起動済みであれば何もしない。
/// 1つも起動していない場合はVOICEVOXを自動起動し、base_urls[0]で待機する。
async fn ensure_engine_running_impl<F>(
    base_urls: &[&str],
    mut progress: F,
    log_to_stderr: bool,
) -> Result<()>
where
    F: FnMut(String),
{
    for &url in base_urls {
        if is_engine_running(url).await {
            return Ok(());
        }
    }

    let config = crate::config::load_or_create()?;
    let exe = find_voicevox_executable(&config).ok_or_else(|| {
        anyhow::anyhow!(
            "VOICEVOXの実行ファイルが見つかりませんでした。\n\
VOICEVOXのインストール先、または設定ファイルを確認してください。\n\
config.toml: {}\n\
設定キー: voicevox_path / voicevox_nemo_path",
            crate::config::config_path().display()
        )
    })?;
    let wait_url = select_wait_url_for_engine(base_urls, &config, &exe);

    progress(voicevox_start_status_message());
    if log_to_stderr {
        eprintln!("VOICEVOXエンジンを起動します: {}", exe.display());
    }
    launch_voicevox(&exe)?;

    progress(voicevox_wait_status_message());
    if log_to_stderr {
        eprintln!("VOICEVOXエンジンが起動するまで待機しています...");
    }
    wait_for_engine(wait_url).await?;
    if log_to_stderr {
        eprintln!("VOICEVOXエンジンの起動が完了しました。");
    }

    Ok(())
}

/// エンジンが起動していなければ自動起動し、起動完了まで待機する。
/// base_urlsのうち1つでも起動済みであれば何もしない。
/// 1つも起動していない場合はVOICEVOXを自動起動し、base_urls[0]で待機する。
pub async fn ensure_engine_running(base_urls: &[&str]) -> Result<()> {
    ensure_engine_running_impl(base_urls, |_| {}, true).await
}

pub async fn ensure_engine_running_with_progress<F>(base_urls: &[&str], progress: F) -> Result<()>
where
    F: FnMut(String),
{
    ensure_engine_running_impl(base_urls, progress, false).await
}

/// mascot-render-server が起動していなければ自動起動し、起動完了まで待機する。
/// 実行ファイルが見つからない場合は自動起動をスキップする。
pub async fn ensure_mascot_render_running() -> Result<()> {
    ensure_mascot_render_ready_impl(|_| {}, true).await
}

pub fn spawn_mascot_render_startup() {
    crate::mascot_render::set_startup_in_progress(true);
    crate::mascot_render::set_startup_overlay_message(mascot_render_check_status_message());
    tokio::spawn(async move {
        let result = ensure_mascot_render_ready_impl(
            crate::mascot_render::set_startup_overlay_message,
            false,
        )
        .await;
        crate::mascot_render::set_startup_in_progress(false);
        crate::mascot_render::clear_startup_overlay_message();
        if let Err(error) = result {
            crate::mascot_render::set_blocking_overlay_message(format!(
                "mascot-render-server の自動起動に失敗しました:\n{error:#}"
            ));
        }
    });
}
