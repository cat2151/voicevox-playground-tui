//! 自動アップデート機能。
//! 起動時にGitHubのmainブランチのhashをチェックし、
//! ローカルのhashと異なる場合はcargo installで更新する。

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

/// cargo installのstderrにlockエラーが含まれているか判定する（主にWindows用）
fn is_lock_error(stderr: &[u8]) -> bool {
    let s = String::from_utf8_lossy(stderr);
    // Windows: "Access is denied" (os error 5) または "being used by another process" (os error 32)
    s.contains("Access is denied")
        || s.contains("being used by another process")
        || s.contains("os error 5")
        || s.contains("os error 32")
}

/// ユニークなファイル名を生成するためのタイムスタンプ（ナノ秒）を返す
fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// アップデータースクリプトを一時ディレクトリに書き込みspawnする。
/// スクリプトは: メインプロセス終了を待つ → cargo install → vpt を起動。
/// ユニークなファイル名を使い、実行後に自身を削除する。
fn spawn_updater_process() -> Result<()> {
    let suffix = unique_suffix();

    #[cfg(target_os = "windows")]
    {
        let script_path = std::env::temp_dir().join(format!("vpt_updater_{}.bat", suffix));
        let script = format!(
            "@echo off\r\ntimeout /t 3 /nobreak >nul\r\ncargo install --force --git https://github.com/{}/{}\r\ndel \"%~f0\"\r\nvpt\r\n",
            REPO_OWNER, REPO_NAME
        );
        std::fs::write(&script_path, &script)?;
        let script_str = script_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Updater script path contains invalid UTF-8"))?;
        std::process::Command::new("cmd")
            .args(["/C", "start", "vpt updater", script_str])
            .spawn()?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        let script_path = std::env::temp_dir().join(format!("vpt_updater_{}.sh", suffix));
        let script = format!(
            "#!/bin/sh\nsleep 3\ncargo install --force --git https://github.com/{}/{}\nrm -- \"$0\"\nvpt\n",
            REPO_OWNER, REPO_NAME
        );
        std::fs::write(&script_path, &script)?;
        // 実行権限を付与
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                &script_path,
                std::fs::Permissions::from_mode(0o755),
            )?;
        }
        std::process::Command::new("sh")
            .arg(&script_path)
            .spawn()?;
    }

    Ok(())
}

/// バックグラウンドでアップデートチェックを実行する。
/// 更新が必要で自動インストールが開始されたら `should_exit` を true にセットする。
pub fn spawn_update_check(should_exit: Arc<AtomicBool>) {
    tokio::spawn(async move {
        if let Err(e) = check_and_update(should_exit).await {
            eprintln!("[updater] error: {}", e);
        }
    });
}

async fn check_and_update(should_exit: Arc<AtomicBool>) -> Result<()> {
    // デバッグビルド時は自動アップデートをスキップ（開発中の誤更新を防止）
    if cfg!(debug_assertions) {
        return Ok(());
    }

    // リモートhashを取得
    let remote_hash = match fetch_remote_hash().await {
        Ok(h) => h,
        Err(e) => {
            eprintln!("[updater] Failed to fetch remote hash: {}", e);
            return Ok(());
        }
    };

    let local = LOCAL_HASH.trim();

    // hashが不明またはリモートと一致していれば何もしない
    if local == "unknown" || local.is_empty() || remote_hash == local {
        return Ok(());
    }

    eprintln!(
        "[updater] Update available (local: {}, remote: {})",
        &local[..8.min(local.len())],
        &remote_hash[..8.min(remote_hash.len())]
    );

    // cargo install を実行（コンパイルに時間がかかるためspawn_blockingで実行）。
    // stdoutは大量のビルドログを含むためnullに捨て、lockエラー検出に必要なstderrのみ収集する。
    let output = tokio::task::spawn_blocking(|| {
        std::process::Command::new("cargo")
            .args([
                "install",
                "--force",
                "--git",
                &format!("https://github.com/{}/{}", REPO_OWNER, REPO_NAME),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()
    })
    .await??;

    if output.status.success() {
        // インストール成功: vpt を再起動して終了
        if let Err(e) = std::process::Command::new("vpt").spawn() {
            eprintln!("[updater] Failed to launch updated vpt: {}", e);
        }
        should_exit.store(true, Ordering::Relaxed);
    } else if is_lock_error(&output.stderr) {
        // 実行中のexeがlockされているため置き換えに失敗（Windows特有）。
        // アップデータープロセスを起動してメインアプリを終了する。
        // メインアプリ終了後にアップデータープロセスがcargo installを再実行し、
        // 最終フェーズを成功させて自動アップデートを完了させる。
        match spawn_updater_process() {
            Ok(()) => {
                should_exit.store(true, Ordering::Relaxed);
            }
            Err(e) => {
                eprintln!("[updater] Failed to spawn updater process: {}", e);
            }
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("[updater] cargo install failed: {}", stderr);
    }

    Ok(())
}
