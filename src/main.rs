mod app;
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
use app::App;

const BASE_URL: &str = "http://localhost:50021";

#[tokio::main]
async fn main() -> Result<()> {
    // 起動時に speaker テーブルをAPIから取得する（ハードコーディングなし）
    speakers::load(BASE_URL).await?;

    let lines   = history::load()?;
    let mut app = App::new(lines);

    // バックグラウンドで自動アップデートチェックを開始する
    updater::spawn_update_check(std::sync::Arc::clone(&app.should_exit_for_update));

    app.init().await;
    tui::run(&mut app).await?;

    history::append_new(&app.lines)?;
    Ok(())
}
