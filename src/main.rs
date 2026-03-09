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

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let use_clipboard = args.iter().any(|arg| arg == "--clipboard");

    // エンジンが起動していなければ自動起動する
    engine_launcher::ensure_engine_running(BASE_URLS).await?;

    // 起動時に speaker テーブルをAPIから取得する（ハードコーディングなし）
    speakers::load(BASE_URLS).await?;

    if use_clipboard {
        // --clipboard: クリップボードを読み上げて終了（history.txtには追加しない）
        return clipboard::run().await;
    }

    let all_lines = history::load_all()?;
    let all_intonations = history::load_all_intonations(&all_lines)?;
    let mut app = App::new_with_tabs(all_lines, all_intonations);

    // バックグラウンドで自動アップデートチェックを開始する
    updater::spawn_update_check(std::sync::Arc::clone(&app.update_available));

    app.init().await;
    tui::run(&mut app).await?;

    history::save_all(&app.all_tab_lines())?;
    history::save_all_intonations(&app.all_tab_lines(), &app.all_tab_intonations())?;

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
