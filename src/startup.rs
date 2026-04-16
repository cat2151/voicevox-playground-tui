use anyhow::{Context, Result};
use tokio::sync::mpsc;

use crate::app::{AllTabIntonations, AllTabLines};
use crate::history::SessionState;

pub struct LoadedHistory {
    pub all_lines: AllTabLines,
    pub all_intonations: AllTabIntonations,
    pub session_state: SessionState,
}

pub type LoadedHistoryResult = Result<LoadedHistory>;
pub type RuntimeStartupResult = Result<()>;

pub enum RuntimeStartupEvent {
    Status(String),
    Ready(RuntimeStartupResult),
}

pub fn spawn_history_loader() -> mpsc::UnboundedReceiver<LoadedHistoryResult> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let result = match tokio::task::spawn_blocking(load_history).await {
            Ok(result) => result,
            Err(err) => Err(anyhow::anyhow!("history loader task failed: {}", err)),
        };
        let _ = tx.send(result);
    });
    rx
}

pub fn spawn_runtime_loader(
    base_urls: &'static [&'static str],
) -> mpsc::UnboundedReceiver<RuntimeStartupEvent> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let _ = tx.send(RuntimeStartupEvent::Status(String::from(
            "[startup] checking VOICEVOX...",
        )));
        let engine_result =
            crate::engine_launcher::ensure_engine_running_with_progress(base_urls, |status| {
                let _ = tx.send(RuntimeStartupEvent::Status(status));
            })
            .await
            .context("VOICEVOX startup failed");
        if let Err(err) = engine_result {
            let _ = tx.send(RuntimeStartupEvent::Ready(Err(err)));
            return;
        }

        let _ = tx.send(RuntimeStartupEvent::Status(String::from(
            "[startup] loading speakers...",
        )));
        let speakers_result = crate::speakers::load(base_urls)
            .await
            .context("speaker load failed");
        let _ = tx.send(RuntimeStartupEvent::Ready(speakers_result));
    });
    rx
}

fn load_history() -> LoadedHistoryResult {
    let (all_lines, all_intonations) = crate::history::load_all()?;
    let session_state = crate::history::load_session_state();
    Ok(LoadedHistory {
        all_lines,
        all_intonations,
        session_state,
    })
}
