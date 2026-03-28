//! 自動アップデート機能。
//! 起動時にGitHubのmainブランチのhashをチェックし、
//! ローカルのhashと異なる場合はユーザーに選択を委ねる。

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::Result;

const REPO_OWNER: &str = "cat2151";
const REPO_NAME: &str = "voicevox-playground-tui";
const GIT_URL: &str = "https://github.com/cat2151/voicevox-playground-tui";

/// ビルド時に埋め込まれたgit commit hash
const LOCAL_HASH: &str = env!("GIT_COMMIT_HASH");

/// リモートのmainブランチの最新commit hashをGitHub APIで取得する
async fn fetch_remote_hash() -> Result<String> {
    let client = reqwest::Client::builder()
        .user_agent("voicevox-playground-tui-updater")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let url = format!(
        "https://api.github.com/repos/{}/{}/commits/main",
        REPO_OWNER, REPO_NAME
    );

    let resp: serde_json::Value = client
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    resp["sha"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("SHA field not found in GitHub API response"))
}

/// バックグラウンドでアップデートチェックを実行する。
/// 更新が必要な場合は `update_available` を true にセットし、ユーザーの選択を待つ。
pub fn spawn_update_check(update_available: Arc<AtomicBool>) {
    tokio::spawn(async move {
        if let Err(e) = check_for_update(update_available).await {
            // TUI動作中のためeprintlnは使わない（表示崩れ防止）
            let _ = e; // エラーは無視してサイレントに失敗する
        }
    });
}

async fn check_for_update(update_available: Arc<AtomicBool>) -> Result<()> {
    // デバッグビルド時は自動アップデートをスキップ（開発中の誤更新を防止）
    if cfg!(debug_assertions) {
        return Ok(());
    }

    // リモートhashを取得
    let remote_hash = match fetch_remote_hash().await {
        Ok(h) => h,
        Err(_) => return Ok(()), // ネットワークエラーはサイレントに無視
    };

    let local = LOCAL_HASH.trim();

    // hashが不明またはリモートと一致していれば何もしない
    if local == "unknown" || local.is_empty() || remote_hash == local {
        return Ok(());
    }

    // アップデートが利用可能: フラグをセットしてユーザーの選択を待つ
    update_available.store(true, Ordering::Relaxed);

    Ok(())
}

fn install_command() -> String {
    format!("cargo install --force --git {GIT_URL}")
}

#[cfg(any(target_os = "windows", test))]
fn update_bat_content(relaunch_command: Option<&str>) -> String {
    let install = install_command();
    let mut script = String::from("@echo off\r\ntimeout /t 3 /nobreak >nul\r\n");
    match relaunch_command {
        Some(command) => {
            script.push_str(&format!("{install} && start \"\" /b {command}\r\n"));
        }
        None => {
            script.push_str(&install);
            script.push_str("\r\n");
        }
    }
    script.push_str("del \"%~f0\"\r\n");
    script
}

#[cfg(target_os = "windows")]
fn write_update_script(prefix: &str, relaunch_command: Option<&str>) -> Result<std::path::PathBuf> {
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    let pid = std::process::id();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let script_path = std::env::temp_dir().join(format!("{prefix}_{pid}_{timestamp}.bat"));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&script_path)?;
    file.write_all(update_bat_content(relaunch_command).as_bytes())?;
    Ok(script_path)
}

/// Windowsでのアップデートを行うバッチファイルをspawnする。
/// スクリプトは: メインプロセス終了を待つ → cargo install → 必要なら vpt を起動。
/// ユニークなファイル名を使い、実行後に自身を削除する。
#[cfg(target_os = "windows")]
fn spawn_updater_process(relaunch_command: Option<&str>) -> Result<()> {
    let script_path = write_update_script("vpt_updater", relaunch_command)?;
    let script_str = script_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Updater script path contains invalid UTF-8"))?;
    std::process::Command::new("cmd")
        .args(["/C", "start", "vpt updater", script_str])
        .spawn()?;
    Ok(())
}

async fn run_install() -> Result<()> {
    let install_command = install_command();
    println!("{install_command}");

    let status = tokio::task::spawn_blocking(move || {
        std::process::Command::new("cargo")
            .args(["install", "--force", "--git", GIT_URL])
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
    })
    .await??;

    if !status.success() {
        anyhow::bail!("アップデートに失敗しました。");
    }

    Ok(())
}

/// 表でアップデートする（端末にビルドログを表示しながら cargo install を実行）。
/// TUIを終了してから呼び出すこと。
/// Windowsではexeファイルのロックにより直接インストールできないため、バッチファイルを使用する。
pub async fn run_foreground_update() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        println!("アップデートをバッチファイルで開始します...");
        spawn_updater_process(Some("vpt")).map_err(|e| {
            anyhow::anyhow!("バッチファイルアップデーターの起動に失敗しました: {}", e)
        })?;
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        println!("アップデートを開始します...");
        run_install().await?;
        println!("アップデート成功！再起動します...");
        if let Err(e) = std::process::Command::new("vpt").spawn() {
            eprintln!("vptの再起動に失敗しました: {}", e);
        }

        Ok(())
    }
}

/// updateサブコマンド用のself updateを実行する。
/// Windowsでは自分自身のexeを置き換えるため、batを起動して即時終了する。
pub async fn run_self_update() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        println!("セルフアップデートをバッチファイルで開始します...");
        spawn_updater_process(None).map_err(|e| {
            anyhow::anyhow!(
                "セルフアップデート用バッチファイルの起動に失敗しました: {}",
                e
            )
        })?;
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        println!("セルフアップデートを開始します...");
        run_install().await
    }
}

#[cfg(test)]
#[path = "tests/updater.rs"]
mod tests;
