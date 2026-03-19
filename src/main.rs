mod app;
mod background_prefetch;
mod clipboard;
mod engine_launcher;
mod fetch;
mod history;
mod player;
mod speakers;
mod tag;
mod tui;
mod ui;
mod updater;
mod voicevox;

use anyhow::Result;
use app::{App, UpdateAction};

const BASE_URLS: &[&str] = &[
    "http://localhost:50021",   // VOICEVOX
    "http://localhost:50121",   // VOICEVOX nemo
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupMode {
    Normal,
    Clipboard,
    Update,
}

fn startup_mode(args: &[String]) -> StartupMode {
    match args {
        [_, command] if command == "update" => StartupMode::Update,
        _ if args.iter().any(|arg| arg == "--clipboard") => StartupMode::Clipboard,
        _ => StartupMode::Normal,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mode = startup_mode(&args);

    match mode {
        StartupMode::Update => {
            updater::run_foreground_update().await?;
            return Ok(());
        }
        StartupMode::Clipboard | StartupMode::Normal => {}
    }

    // エンジンが起動していなければ自動起動する
    engine_launcher::ensure_engine_running(BASE_URLS).await?;

    // 起動時に speaker テーブルをAPIから取得する（ハードコーディングなし）
    speakers::load(BASE_URLS).await?;

    if mode == StartupMode::Clipboard {
        // --clipboard: クリップボードを読み上げて終了（history.txtには追加しない）
        return clipboard::run().await;
    }

    let (all_lines, all_intonations) = history::load_all()?;
    let mut app = App::new_with_tabs(all_lines, all_intonations);

    // 前回終了時のタブ・カーソル位置・折りたたみ状態を復元する
    let session_state = history::load_session_state();
    app.restore_session_state(&session_state);

    // バックグラウンドで自動アップデートチェックを開始する
    updater::spawn_update_check(std::sync::Arc::clone(&app.update_available));

    app.init().await;
    tui::run(&mut app).await?;

    let final_lines = app.all_tab_lines();
    let final_intonations = app.all_tab_intonations();
    let final_session_state = app.collect_session_state();

    history::save_all(&final_lines, &final_intonations)?;
    history::save_session_state(&final_session_state)?;

    // ユーザーが選択したアップデート実行方法に応じて処理する
    match app.update_action {
        Some(UpdateAction::Foreground) => {
            if let Err(e) = updater::run_foreground_update().await {
                eprintln!("フォアグラウンドアップデートに失敗しました: {}", e);
            }
        }
        None => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{startup_mode, StartupMode};

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn startup_mode_is_update_when_only_update_subcommand_is_provided() {
        let actual = startup_mode(&args(&["vpt", "update"]));
        assert_eq!(actual, StartupMode::Update);
    }

    #[test]
    fn startup_mode_is_not_update_when_extra_args_are_present() {
        let actual = startup_mode(&args(&["vpt", "update", "--clipboard"]));
        assert_eq!(actual, StartupMode::Clipboard);
    }

    #[test]
    fn startup_mode_is_clipboard_when_clipboard_flag_is_present() {
        let actual = startup_mode(&args(&["vpt", "--clipboard"]));
        assert_eq!(actual, StartupMode::Clipboard);
    }

    #[test]
    fn startup_mode_is_normal_without_update_or_clipboard() {
        let actual = startup_mode(&args(&["vpt"]));
        assert_eq!(actual, StartupMode::Normal);
    }
}
