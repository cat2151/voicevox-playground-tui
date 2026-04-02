use anyhow::Result;
use tokio::sync::mpsc;

use crate::app::{AllTabIntonations, AllTabLines};
use crate::history::SessionState;

pub struct LoadedHistory {
    pub all_lines: AllTabLines,
    pub all_intonations: AllTabIntonations,
    pub session_state: SessionState,
}

pub type LoadedHistoryResult = Result<LoadedHistory>;

pub fn spawn_history_loader() -> mpsc::UnboundedReceiver<LoadedHistoryResult> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::task::spawn_blocking(move || {
        let result = load_history();
        let _ = tx.send(result);
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
