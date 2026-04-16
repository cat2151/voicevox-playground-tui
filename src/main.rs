mod app;
mod background_prefetch;
mod clipboard;
mod config;
mod engine_launcher;
mod fetch;
mod history;
mod mascot_render;
mod player;
mod runtime_notice;
mod speakers;
mod startup;
mod tag;
mod tui;
mod ui;
mod updater;
mod voicevox;

use anyhow::Result;
use app::App;

const BASE_URLS: &[&str] = &[
    "http://localhost:50021", // VOICEVOX
    "http://localhost:50121", // VOICEVOX nemo
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupMode {
    Normal,
    Clipboard,
    Update,
    Check,
}

fn startup_mode(args: &[String]) -> StartupMode {
    match args {
        [_, command] if command == "update" => StartupMode::Update,
        [_, command] if command == "check" => StartupMode::Check,
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
            updater::run_self_update().await?;
            return Ok(());
        }
        StartupMode::Check => {
            updater::run_check().await?;
            return Ok(());
        }
        StartupMode::Clipboard | StartupMode::Normal => {}
    }

    mascot_render::init_data_root_env();
    if mode == StartupMode::Clipboard {
        if let Err(error) = engine_launcher::ensure_mascot_render_running().await {
            eprintln!("mascot-render-server の自動起動に失敗しました: {error:#}");
        }
        // エンジンが起動していなければ自動起動する
        engine_launcher::ensure_engine_running(BASE_URLS).await?;

        // 起動時に speaker テーブルをAPIから取得する（ハードコーディングなし）
        speakers::load(BASE_URLS).await?;
        // --clipboard: クリップボードを読み上げて終了（history.txtには追加しない）
        return clipboard::run().await;
    }

    let history_rx = if mode == StartupMode::Normal {
        Some(startup::spawn_history_loader())
    } else {
        None
    };
    let runtime_startup_rx = if mode == StartupMode::Normal {
        Some(startup::spawn_runtime_loader(BASE_URLS))
    } else {
        None
    };
    let mut app = App::new(vec![String::new()]);
    if history_rx.is_some() || runtime_startup_rx.is_some() {
        app.status_msg = String::from("[startup] loading history...");
    }
    engine_launcher::spawn_mascot_render_startup();

    let exit_disposition = tui::run(&mut app, history_rx, runtime_startup_rx).await?;

    if exit_disposition == tui::ExitDisposition::PersistState {
        let final_lines = app.all_tab_lines();
        let final_intonations = app.all_tab_intonations();
        let final_session_state = app.collect_session_state();

        history::save_all(&final_lines, &final_intonations)?;
        history::save_session_state(&final_session_state)?;
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests/main.rs"]
mod tests;
