//! update / check サブコマンド用の更新機能。

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
