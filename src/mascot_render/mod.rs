#[cfg(test)]
use std::sync::Mutex;
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use mascot_render_client::mascot_render_server_status;
use mascot_render_protocol::ServerEnsembleMode;

mod data;
mod ensemble;
mod logging;
mod overlay;
mod playback;
mod playback_logging;
mod requests;
mod state;
#[cfg(test)]
mod test_support;

#[cfg(test)]
use self::data::vpt_ensemble_character_names;
#[cfg(test)]
use self::data::{default_mascot_data_root, mascot_data_root, set_loaded_psd_file_names};
pub(crate) use self::data::{
    init_data_root_env, refresh_available_psd_file_names_from_server, speaker_has_psd,
};
use self::data::{mascot_char_name_for_line, wav_duration_ms};
use self::ensemble::{
    configure_vpt_ensemble_members, configure_vpt_ensemble_startup,
    restore_vpt_ensemble_session_on_exit,
};
#[cfg(test)]
use self::ensemble::{
    configure_vpt_ensemble_startup_for_mode, configure_vpt_ensemble_startup_for_mode_with_members,
    restore_vpt_ensemble_session_on_exit_with,
};
#[cfg(test)]
use self::logging::{current_log_timestamp, format_mascot_log_message, mascot_log_path};
use self::logging::{
    format_mascot_json_request, format_mascot_request, log_mascot_request_result,
    log_mascot_sync_request_result_timed, report_mascot_log_failure, MascotSyncRequestContext,
};
#[cfg(test)]
use self::logging::{log_mascot_sync_request_start, log_mascot_sync_snapshots_timed};
use self::overlay::clear_overlay_message;
#[cfg(test)]
use self::overlay::set_overlay_message;
pub(crate) use self::overlay::{
    clear_startup_overlay_message, current_overlay_message, current_startup_overlay_message,
    dismiss_blocking_overlay_message, has_blocking_overlay_message, set_blocking_overlay_message,
    set_startup_overlay_message,
};
pub use self::playback::sync_playback;
#[cfg(test)]
use self::playback::{sync_character_change, sync_character_change_with_context};
pub(crate) use self::playback_logging::{
    event_details_with_elapsed, log_playback_error_snapshots, log_playback_event,
    log_playback_request_start, log_playback_snapshots,
};
use self::requests::{
    motion_timeline_request, motion_timeline_request_body,
    play_timeline_mascot_render_server_with_target,
};
pub(crate) use self::state::{init_snapshot_logging_from_config, set_startup_in_progress};
use self::state::{
    is_startup_in_progress, is_vpt_ensemble_startup_in_progress, next_mascot_sync_id,
    set_vpt_ensemble_startup_in_progress, snapshot_logging_enabled,
    sync_vpt_ensemble_session_from_server_mode, vpt_ensemble_session_active,
};
#[cfg(test)]
use self::state::{
    set_snapshot_logging_enabled, set_vpt_ensemble_session_active, vpt_ensemble_session_state,
    VptEnsembleSessionState,
};

const MIN_DURATION_MS: u64 = 100;
pub(super) const FALLBACK_DURATION_MS: u64 = 5_000;
const MASCOT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const MASCOT_IO_TIMEOUT: Duration = Duration::from_secs(2);
const MASCOT_APPLY_TIMEOUT: Duration = Duration::from_secs(15);
const DATA_ROOT_ENV: &str = "MASCOT_RENDER_SERVER_DATA_ROOT";
const MASCOT_MODE_SYNC_INTERVAL: Duration = Duration::from_secs(1);
#[cfg(test)]
const OVERLAY_DURATION: std::time::Duration = std::time::Duration::from_secs(5);

pub(crate) struct MascotEnsembleSessionGuard;

impl MascotEnsembleSessionGuard {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl Drop for MascotEnsembleSessionGuard {
    fn drop(&mut self) {
        restore_vpt_ensemble_session_on_exit();
    }
}

pub(crate) fn spawn_mascot_mode_sync() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        thread::spawn(|| loop {
            thread::sleep(MASCOT_MODE_SYNC_INTERVAL);
            if is_startup_in_progress() || is_vpt_ensemble_startup_in_progress() {
                continue;
            }
            let Ok(status) = mascot_render_server_status() else {
                continue;
            };
            sync_vpt_ensemble_session_from_server_mode(status.ensemble_mode);
        });
    });
}

pub(crate) async fn prepare_vpt_ensemble_startup(lines: Vec<String>) {
    set_vpt_ensemble_startup_in_progress(true);
    let wait_started_at = Instant::now();
    while is_startup_in_progress() {
        if wait_started_at.elapsed() >= Duration::from_secs(60) {
            set_vpt_ensemble_startup_in_progress(false);
            crate::runtime_notice::set_runtime_notice(
                "[mascot-render] vpt ensemble 準備をスキップしました: mascot startup timeout",
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let result = tokio::task::spawn_blocking(move || configure_vpt_ensemble_startup(&lines))
        .await
        .map_err(|error| anyhow::anyhow!("mascot vpt ensemble startup task failed: {error}"))
        .and_then(|result| result);
    set_vpt_ensemble_startup_in_progress(false);

    if let Err(error) = result {
        crate::runtime_notice::set_runtime_notice(format!(
            "[mascot-render] vpt ensemble 準備をスキップしました: {error}"
        ));
    }
}

pub(crate) async fn sync_vpt_ensemble_members(lines: Vec<String>) {
    if is_startup_in_progress() || is_vpt_ensemble_startup_in_progress() {
        return;
    }

    let result = tokio::task::spawn_blocking(move || configure_vpt_ensemble_members(&lines))
        .await
        .map_err(|error| anyhow::anyhow!("mascot vpt ensemble members sync task failed: {error}"))
        .and_then(|result| result);

    if let Err(error) = result {
        crate::runtime_notice::set_runtime_notice(format!(
            "[mascot-render] vpt ensemble members更新をスキップしました: {error}"
        ));
    }
}

#[cfg(test)]
pub(crate) fn with_overlay_state_lock<T>(f: impl FnOnce() -> T) -> T {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    set_startup_in_progress(false);
    dismiss_blocking_overlay_message();
    clear_overlay_message();
    clear_startup_overlay_message();
    set_snapshot_logging_enabled(false);
    set_loaded_psd_file_names(Vec::new());
    set_vpt_ensemble_startup_in_progress(false);
    *vpt_ensemble_session_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = VptEnsembleSessionState::default();
    let result = f();
    set_startup_in_progress(false);
    dismiss_blocking_overlay_message();
    clear_overlay_message();
    clear_startup_overlay_message();
    set_snapshot_logging_enabled(false);
    set_loaded_psd_file_names(Vec::new());
    set_vpt_ensemble_startup_in_progress(false);
    *vpt_ensemble_session_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = VptEnsembleSessionState::default();
    result
}

#[cfg(test)]
pub(crate) fn set_loaded_psd_file_names_for_test(file_names: &[&str]) {
    set_loaded_psd_file_names(file_names.iter().map(ToString::to_string).collect());
}

#[cfg(test)]
#[path = "../tests/mascot_render/mod.rs"]
mod tests;
