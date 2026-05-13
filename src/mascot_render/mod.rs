use std::net::SocketAddr;
use std::sync::mpsc::{self, Sender};
#[cfg(test)]
use std::sync::Mutex;
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use mascot_render_client::{
    change_character_mascot_render_server, mascot_render_server_address, show_mascot_render_server,
};
use mascot_render_protocol::ChangeCharacterRequest;
#[cfg(test)]
use mascot_render_protocol::ServerEnsembleMode;

mod data;
mod ensemble;
mod logging;
mod overlay;
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
    set_vpt_ensemble_startup_in_progress, snapshot_logging_enabled, vpt_ensemble_session_active,
};
#[cfg(test)]
use self::state::{
    set_snapshot_logging_enabled, set_vpt_ensemble_session_active, vpt_ensemble_session_state,
    VptEnsembleSessionState,
};

const MIN_DURATION_MS: u64 = 100;
const FALLBACK_DURATION_MS: u64 = 5_000;
const MASCOT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const MASCOT_IO_TIMEOUT: Duration = Duration::from_secs(2);
const MASCOT_APPLY_TIMEOUT: Duration = Duration::from_secs(15);
const DATA_ROOT_ENV: &str = "MASCOT_RENDER_SERVER_DATA_ROOT";
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

pub fn sync_playback(line: &str, wav: &[u8]) {
    if line.trim().is_empty() || wav.is_empty() {
        return;
    }
    if is_startup_in_progress() || is_vpt_ensemble_startup_in_progress() {
        return;
    }

    let line = line.to_string();
    let duration_ms = wav_duration_ms(wav).unwrap_or(FALLBACK_DURATION_MS);
    let char_name = mascot_char_name_for_line(&line);
    let sync_id = next_mascot_sync_id();
    let queued_at = Instant::now();

    log_playback_event(
        sync_id,
        "enqueue",
        "sync_playback_enqueue",
        &format!("elapsed_ms=0 duration_ms={duration_ms} char_name={char_name:?}"),
    );

    if let Err(error) = mascot_worker_tx().send(MascotPlaybackSync {
        sync_id,
        queued_at,
        char_name,
        duration_ms,
    }) {
        log_playback_event(
            sync_id,
            "enqueue",
            "sync_playback_enqueue_failed",
            &format!(
                "elapsed_ms={} error={error}",
                queued_at.elapsed().as_millis()
            ),
        );
    }
}

#[derive(Debug)]
struct MascotPlaybackSync {
    sync_id: u64,
    queued_at: Instant,
    char_name: Option<String>,
    duration_ms: u64,
}

fn mascot_worker_tx() -> &'static Sender<MascotPlaybackSync> {
    static TX: OnceLock<Sender<MascotPlaybackSync>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<MascotPlaybackSync>();
        thread::spawn(move || {
            while let Ok(sync) = rx.recv() {
                let received_at = Instant::now();
                let queue_wait_ms = received_at.duration_since(sync.queued_at).as_millis();
                log_playback_event(
                    sync.sync_id,
                    "worker",
                    "sync_request_received",
                    &format!(
                        "elapsed_ms={} queue_wait_ms={} duration_ms={} char_name={:?}",
                        queue_wait_ms, queue_wait_ms, sync.duration_ms, &sync.char_name
                    ),
                );
                handle_playback_sync(sync, received_at);
            }
        });
        tx
    })
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
fn sync_character_change<F>(address: SocketAddr, speaker: Option<&str>, change_character: F) -> bool
where
    F: FnOnce(&str) -> anyhow::Result<()>,
{
    sync_character_change_with_context(None, None, address, speaker, change_character)
}

fn sync_character_change_with_context<F>(
    sync_id: Option<u64>,
    sync_started_at: Option<Instant>,
    address: SocketAddr,
    speaker: Option<&str>,
    change_character: F,
) -> bool
where
    F: FnOnce(&str) -> anyhow::Result<()>,
{
    if vpt_ensemble_session_active() {
        if let Some(sync_id) = sync_id {
            let details =
                event_details_with_elapsed(sync_started_at, "reason=vpt_ensemble_session_active");
            log_playback_event(sync_id, "change-character", "request_skipped", &details);
        }
        clear_overlay_message();
        return true;
    }

    let Some(speaker) = speaker else {
        if let Some(sync_id) = sync_id {
            let details = event_details_with_elapsed(sync_started_at, "reason=no_character");
            log_playback_event(sync_id, "change-character", "request_skipped", &details);
        }
        clear_overlay_message();
        return true;
    };
    if !speaker_has_psd(speaker) {
        if let Some(sync_id) = sync_id {
            let details = event_details_with_elapsed(
                sync_started_at,
                &format!("reason=no_psd speaker={speaker}"),
            );
            log_playback_event(sync_id, "change-character", "request_skipped", &details);
        }
        clear_overlay_message();
        return true;
    }

    clear_overlay_message();
    let request_body = ChangeCharacterRequest {
        character_name: speaker.to_string(),
    };
    let request = format_mascot_json_request("POST", "/change-character", address, &request_body);
    if let Some(sync_id) = sync_id {
        log_playback_snapshots(
            sync_id,
            "change-character",
            "before",
            address,
            sync_started_at,
        );
        log_playback_request_start(
            sync_id,
            "change-character",
            &format!("{speaker} へのcharacter変更"),
            address,
            sync_started_at,
        );
    }
    let request_started_at = Instant::now();
    let change_character_result = change_character(speaker);
    let request_duration = request_started_at.elapsed();
    let log_result = if let Some(sync_id) = sync_id {
        log_mascot_sync_request_result_timed(
            MascotSyncRequestContext {
                sync_id,
                phase: "change-character",
                action: &format!("{speaker} へのcharacter変更"),
                address,
                sync_started_at,
            },
            &request,
            &change_character_result,
            request_duration,
        )
    } else {
        log_mascot_request_result(
            &format!("{speaker} へのcharacter変更"),
            address,
            &request,
            &change_character_result,
        )
    };
    if let Err(error) = log_result {
        report_mascot_log_failure(&error);
    }
    if let Some(sync_id) = sync_id {
        log_playback_error_snapshots(
            sync_id,
            "change-character",
            address,
            sync_started_at,
            &change_character_result,
        );
        log_playback_snapshots(
            sync_id,
            "change-character",
            "after",
            address,
            sync_started_at,
        );
    }
    change_character_result.is_ok()
}

fn handle_playback_sync(sync: MascotPlaybackSync, worker_received_at: Instant) {
    let address = mascot_render_server_address();
    let sync_id = sync.sync_id;
    let sync_started_at = Instant::now();
    let queue_wait_ms = worker_received_at
        .duration_since(sync.queued_at)
        .as_millis();

    log_playback_event(
        sync_id,
        "handle",
        "handle_playback_sync_start",
        &format!("elapsed_ms=0 queue_wait_ms={queue_wait_ms}"),
    );

    let show_request = format_mascot_request("POST", "/show", address, None);
    log_playback_snapshots(sync_id, "show", "before", address, Some(sync_started_at));
    log_playback_request_start(sync_id, "show", "表示", address, Some(sync_started_at));
    let show_started_at = Instant::now();
    let show_result = show_mascot_render_server();
    let show_duration = show_started_at.elapsed();
    if let Err(error) = log_mascot_sync_request_result_timed(
        MascotSyncRequestContext {
            sync_id,
            phase: "show",
            action: "表示",
            address,
            sync_started_at: Some(sync_started_at),
        },
        &show_request,
        &show_result,
        show_duration,
    ) {
        report_mascot_log_failure(&error);
    }
    log_playback_error_snapshots(
        sync_id,
        "show",
        address,
        Some(sync_started_at),
        &show_result,
    );
    log_playback_snapshots(sync_id, "show", "after", address, Some(sync_started_at));

    if !sync_character_change_with_context(
        Some(sync_id),
        Some(sync_started_at),
        address,
        sync.char_name.as_deref(),
        change_character_mascot_render_server,
    ) {
        let elapsed_ms = sync_started_at.elapsed().as_millis();
        log_playback_event(
            sync_id,
            "handle",
            "handle_playback_sync_end",
            &format!(
                "elapsed_ms={} duration_ms={} status=stopped_after_change_character_error",
                elapsed_ms, elapsed_ms
            ),
        );
        return;
    }

    let request = motion_timeline_request(sync.duration_ms);
    let target_character_name = sync.char_name.as_deref();
    let request_body = motion_timeline_request_body(&request, target_character_name);
    let request_log = format_mascot_json_request("POST", "/timeline", address, &request_body);
    let action = sync
        .char_name
        .as_deref()
        .map(|speaker| format!("{speaker} の口パク"))
        .unwrap_or_else(|| "口パク".to_string());
    log_playback_snapshots(
        sync_id,
        "timeline",
        "before",
        address,
        Some(sync_started_at),
    );
    log_playback_request_start(sync_id, "timeline", &action, address, Some(sync_started_at));
    let timeline_started_at = Instant::now();
    let timeline_result =
        play_timeline_mascot_render_server_with_target(address, &request, target_character_name);
    let timeline_duration = timeline_started_at.elapsed();
    if let Err(error) = log_mascot_sync_request_result_timed(
        MascotSyncRequestContext {
            sync_id,
            phase: "timeline",
            action: &action,
            address,
            sync_started_at: Some(sync_started_at),
        },
        &request_log,
        &timeline_result,
        timeline_duration,
    ) {
        report_mascot_log_failure(&error);
    }
    log_playback_error_snapshots(
        sync_id,
        "timeline",
        address,
        Some(sync_started_at),
        &timeline_result,
    );
    log_playback_snapshots(sync_id, "timeline", "after", address, Some(sync_started_at));
    let elapsed_ms = sync_started_at.elapsed().as_millis();
    log_playback_event(
        sync_id,
        "handle",
        "handle_playback_sync_end",
        &format!(
            "elapsed_ms={} duration_ms={} status=ok",
            elapsed_ms, elapsed_ms
        ),
    );
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
