//! VOICEVOXエンジンの自動起動モジュール。
//! エンジンが起動していない場合にVOICEVOX実行ファイルを検索して起動し、
//! 起動完了まで待機する。

use anyhow::Result;

/// エンジンが応答するまで待つ最大秒数
const MAX_WAIT_SECS: u64 = 60;

/// ポーリング間隔（ミリ秒）
const POLL_INTERVAL_MS: u64 = 1000;

/// VOICEVOXエンジンが起動しているか確認する（/speakersへのリクエストで確認）。
pub async fn is_engine_running(base_url: &str) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    client
        .get(format!("{base_url}/speakers"))
        .send()
        .await
        .is_ok()
}

/// VOICEVOXの実行ファイルをよく使われるインストール先から探す。
/// 見つかった場合はパスを返す。
fn find_voicevox_executable() -> Option<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();

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

    candidates.into_iter().find(|p| p.exists())
}

/// VOICEVOXを起動する。vptが終了してもVOICEVOXは起動し続ける（デタッチドプロセス）。
fn launch_voicevox(exe: &std::path::Path) -> Result<()> {
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

/// エンジンが起動するまでポーリングして待機する。
async fn wait_for_engine(base_url: &str) -> Result<()> {
    let start = std::time::Instant::now();
    let deadline = std::time::Duration::from_secs(MAX_WAIT_SECS);
    loop {
        if is_engine_running(base_url).await {
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

/// エンジンが起動していなければ自動起動し、起動完了まで待機する。
/// すでに起動済みの場合は何もしない。
pub async fn ensure_engine_running(base_url: &str) -> Result<()> {
    if is_engine_running(base_url).await {
        return Ok(());
    }

    let exe = find_voicevox_executable().ok_or_else(|| {
        anyhow::anyhow!(
            "VOICEVOXの実行ファイルが見つかりませんでした。\
            VOICEVOXをインストールしてから再度お試しください。"
        )
    })?;

    eprintln!("VOICEVOXエンジンを起動します: {}", exe.display());
    launch_voicevox(&exe)?;

    eprintln!("VOICEVOXエンジンが起動するまで待機しています...");
    wait_for_engine(base_url).await?;
    eprintln!("VOICEVOXエンジンの起動が完了しました。");

    Ok(())
}
