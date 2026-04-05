//! 自動アップデート機能。
//! 起動時にGitHubのmainブランチのhashをチェックし、
//! ローカルのhashと異なる場合はユーザーに選択を委ねる。

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::{anyhow, Result};
use cat_self_update_lib::{check_remote_commit, self_update, CheckResult};

const REPO_OWNER: &str = "cat2151";
const REPO_NAME: &str = "voicevox-playground-tui";
const MAIN_BRANCH: &str = "main";
const BIN_NAME: &str = "vpt";

/// ビルド時に埋め込まれたgit commit hash
const LOCAL_HASH: &str = env!("GIT_COMMIT_HASH");

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

    let result = match tokio::task::spawn_blocking(|| {
        check_remote_commit(REPO_OWNER, REPO_NAME, MAIN_BRANCH, LOCAL_HASH)
            .map_err(|error| anyhow!(error.to_string()))
    })
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(_)) | Err(_) => return Ok(()),
    };

    if !is_update_available(&result) {
        return Ok(());
    }

    // アップデートが利用可能: フラグをセットしてユーザーの選択を待つ
    update_available.store(true, Ordering::Relaxed);

    Ok(())
}

fn is_update_available(result: &CheckResult) -> bool {
    let local = result.embedded_hash.trim();
    !local.is_empty() && local != "unknown" && !result.is_up_to_date()
}

/// 表でアップデートする（端末にビルドログを表示しながら cargo install を実行）。
/// TUIを終了してから呼び出すこと。
pub async fn run_foreground_update() -> Result<()> {
    println!("アップデートを開始します...");
    self_update(REPO_OWNER, REPO_NAME, &[BIN_NAME])
        .map_err(|error| anyhow!("アップデートに失敗しました: {error}"))
}

/// updateサブコマンド用のself updateを実行する。
pub async fn run_self_update() -> Result<()> {
    println!("セルフアップデートを開始します...");
    self_update(REPO_OWNER, REPO_NAME, &[BIN_NAME])
        .map_err(|error| anyhow!("セルフアップデートに失敗しました: {error}"))
}

/// checkサブコマンド用のアップデートチェックを実行する。
pub async fn run_check() -> Result<()> {
    let result = tokio::task::spawn_blocking(|| {
        check_remote_commit(REPO_OWNER, REPO_NAME, MAIN_BRANCH, LOCAL_HASH)
            .map_err(|error| anyhow!(error.to_string()))
    })
    .await??;

    println!("{result}");
    Ok(())
}

#[cfg(test)]
#[path = "tests/updater.rs"]
mod tests;
