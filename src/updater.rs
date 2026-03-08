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

/// 表でアップデートする（端末にビルドログを表示しながら cargo install を実行）。
/// TUIを終了してから呼び出すこと。
pub async fn run_foreground_update() -> Result<()> {
    println!("アップデートを開始します...");
    println!("cargo install --force --git https://github.com/{}/{}", REPO_OWNER, REPO_NAME);

    let status = tokio::task::spawn_blocking(|| {
        std::process::Command::new("cargo")
            .args([
                "install",
                "--force",
                "--git",
                &format!("https://github.com/{}/{}", REPO_OWNER, REPO_NAME),
            ])
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
    })
    .await??;

    if status.success() {
        println!("アップデート成功！再起動します...");
        if let Err(e) = std::process::Command::new("vpt").spawn() {
            eprintln!("vptの再起動に失敗しました: {}", e);
        }
    } else {
        eprintln!("アップデートに失敗しました。");
    }

    Ok(())
}
