use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use mascot_render_protocol::ServerEnsembleMode;

#[derive(Debug, Default)]
pub(super) struct VptEnsembleSessionState {
    pub(super) startup_mode: Option<ServerEnsembleMode>,
    pub(super) active: bool,
    pub(super) restore_single_character_on_exit: bool,
    pub(super) last_synced_members: Option<Vec<String>>,
}

pub(super) fn is_startup_in_progress() -> bool {
    startup_in_progress_flag().load(Ordering::Relaxed)
}

pub(super) fn is_vpt_ensemble_startup_in_progress() -> bool {
    vpt_ensemble_startup_in_progress_flag().load(Ordering::Relaxed)
}

pub(crate) fn set_startup_in_progress(in_progress: bool) {
    startup_in_progress_flag().store(in_progress, Ordering::Relaxed);
}

pub(super) fn set_vpt_ensemble_startup_in_progress(in_progress: bool) {
    vpt_ensemble_startup_in_progress_flag().store(in_progress, Ordering::Relaxed);
}

fn startup_in_progress_flag() -> &'static AtomicBool {
    static FLAG: OnceLock<AtomicBool> = OnceLock::new();
    FLAG.get_or_init(|| AtomicBool::new(false))
}

fn vpt_ensemble_startup_in_progress_flag() -> &'static AtomicBool {
    static FLAG: OnceLock<AtomicBool> = OnceLock::new();
    FLAG.get_or_init(|| AtomicBool::new(false))
}

pub(super) fn vpt_ensemble_session_state() -> &'static Mutex<VptEnsembleSessionState> {
    static STATE: OnceLock<Mutex<VptEnsembleSessionState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(VptEnsembleSessionState::default()))
}

#[cfg(test)]
pub(super) fn set_vpt_ensemble_session_active(active: bool) {
    vpt_ensemble_session_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .active = active;
}

pub(super) fn sync_vpt_ensemble_session_from_server_mode(mode: ServerEnsembleMode) {
    let mut state = vpt_ensemble_session_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let active = matches!(mode, ServerEnsembleMode::Vpt);
    state.active = active;
    if !active {
        state.restore_single_character_on_exit = false;
    }
}

pub(super) fn vpt_ensemble_session_active() -> bool {
    vpt_ensemble_session_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .active
}

pub(crate) fn init_snapshot_logging_from_config() {
    let enabled = match crate::config::load_or_create() {
        Ok(config) => config.mascot_render_snapshot_log,
        Err(error) => {
            crate::runtime_notice::set_runtime_notice(format!(
                "[mascot-render] config.toml の snapshot 設定を読めませんでした: {error}"
            ));
            false
        }
    };
    set_snapshot_logging_enabled(enabled);
}

pub(super) fn set_snapshot_logging_enabled(enabled: bool) {
    snapshot_logging_enabled_flag().store(enabled, Ordering::Relaxed);
    snapshot_logging_initialized_flag().store(true, Ordering::Relaxed);
}

pub(super) fn snapshot_logging_enabled() -> bool {
    if !snapshot_logging_initialized_flag().load(Ordering::Relaxed) {
        init_snapshot_logging_from_config();
    }
    snapshot_logging_enabled_flag().load(Ordering::Relaxed)
}

fn snapshot_logging_enabled_flag() -> &'static AtomicBool {
    static FLAG: OnceLock<AtomicBool> = OnceLock::new();
    FLAG.get_or_init(|| AtomicBool::new(false))
}

fn snapshot_logging_initialized_flag() -> &'static AtomicBool {
    static FLAG: OnceLock<AtomicBool> = OnceLock::new();
    FLAG.get_or_init(|| AtomicBool::new(false))
}

pub(super) fn next_mascot_sync_id() -> u64 {
    static NEXT_SYNC_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_SYNC_ID.fetch_add(1, Ordering::Relaxed)
}
