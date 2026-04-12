//! 自動アップデート機能。
//! 起動時にGitHubのmainブランチのhashをチェックし、
//! ローカルのhashと異なる場合はユーザーに選択を委ねる。

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::{anyhow, Context, Result};
use cat_self_update_lib::{check_remote_commit, self_update, CheckResult};

const REPO_OWNER: &str = "cat2151";
const REPO_NAME: &str = "voicevox-playground-tui";
const MAIN_BRANCH: &str = "main";

/// ビルド時に埋め込まれたgit commit hash
const LOCAL_HASH: &str = env!("GIT_COMMIT_HASH");

// `cat_self_update_lib` の現行 API では、crates 引数に空配列を渡す必要がある。
fn self_update_crates() -> &'static [&'static str] {
    &[]
}

/// `block_in_place` から呼び出す同期的な更新確認ヘルパー。
/// `check_remote_commit()` の結果をそのまま返し、呼び出し側で失敗時の扱いを決める。
fn check_remote_commit_sync() -> std::result::Result<CheckResult, Box<dyn std::error::Error>> {
    check_remote_commit(REPO_OWNER, REPO_NAME, MAIN_BRANCH, LOCAL_HASH)
}

/// `self_update()` を `spawn_blocking` 上で実行する。
/// 更新処理は同期的で重くなりうるため、tokio ランタイムのワーカースレッドを塞がないようにする。
async fn run_self_update_blocking() -> Result<()> {
    tokio::task::spawn_blocking(|| {
        self_update(REPO_OWNER, REPO_NAME, self_update_crates())
            .map_err(|error| format!("{error:#}"))
    })
    .await
    .context("アップデートタスクの実行に失敗しました")?
    .map_err(|error| anyhow!(error))?;
    Ok(())
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

    let result = match tokio::task::block_in_place(check_remote_commit_sync) {
        Ok(result) => result,
        Err(_) => return Ok(()),
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

/// TUI終了後に前景でアップデート処理を実行する。
/// 標準出力に開始メッセージを表示してから `cat_self_update_lib::self_update()` を呼び出す。
pub async fn run_foreground_update() -> Result<()> {
    println!("アップデートを開始します...");
    run_self_update_blocking().await
}

/// updateサブコマンド用のself updateを実行する。
pub async fn run_self_update() -> Result<()> {
    println!("セルフアップデートを開始します...");
    run_self_update_blocking().await
}

/// checkサブコマンド用のアップデートチェックを実行する。
pub async fn run_check() -> Result<()> {
    let result = tokio::task::block_in_place(check_remote_commit_sync)
        .map_err(|error| anyhow!("{error:#}"))?;

    println!("{result}");
    Ok(())
}

#[cfg(test)]
#[path = "tests/updater.rs"]
mod tests;
