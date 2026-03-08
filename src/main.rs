mod app;
mod background_prefetch;
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

const BASE_URL: &str = "http://localhost:50021";

#[tokio::main]
async fn main() -> Result<()> {
    // エンジンが起動していなければ自動起動する
    engine_launcher::ensure_engine_running(BASE_URL).await?;

    // 起動時に speaker テーブルをAPIから取得する（ハードコーディングなし）
    speakers::load(BASE_URL).await?;

    let lines   = history::load()?;
    let mut app = App::new(lines);

    // バックグラウンドで自動アップデートチェックを開始する
    updater::spawn_update_check(std::sync::Arc::clone(&app.update_available));

    app.init().await;
    tui::run(&mut app).await?;

    history::append_new(&app.lines)?;

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
