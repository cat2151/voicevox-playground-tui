use std::net::SocketAddr;
use std::sync::mpsc::{self, Sender};
#[cfg(test)]
use std::sync::Mutex;
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::bail;
use mascot_render_client::{
    change_character_mascot_render_server, mascot_render_server_address,
    mascot_render_server_healthcheck, mascot_render_server_status,
    play_timeline_mascot_render_server, preview_mouth_flap_timeline_request,
    set_single_character_mode_mascot_render_server, set_vpt_ensemble_mascot_render_server,
    show_mascot_render_server, PREVIEW_MOUTH_FLAP_FPS,
};
use mascot_render_protocol::{
    ChangeCharacterRequest, MotionTimelineKind, MotionTimelineRequest, MotionTimelineStep,
    ServerEnsembleMode, VptEnsembleRequest,
};

mod data;
mod http;
mod logging;
mod overlay;
mod playback_logging;
mod state;
#[cfg(test)]
mod test_support;

#[cfg(test)]
use self::data::{default_mascot_data_root, mascot_data_root, set_loaded_psd_file_names};
pub(crate) use self::data::{
    init_data_root_env, refresh_available_psd_file_names_from_server, speaker_has_psd,
};
use self::data::{mascot_char_name_for_line, vpt_ensemble_character_names, wav_duration_ms};
pub(crate) use self::http::disable_favorite_ensemble_mascot_render_server_at;
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
pub(crate) use self::state::{init_snapshot_logging_from_config, set_startup_in_progress};
use self::state::{
    is_startup_in_progress, is_vpt_ensemble_startup_in_progress, next_mascot_sync_id,
    set_vpt_ensemble_startup_in_progress, snapshot_logging_enabled, vpt_ensemble_session_active,
    vpt_ensemble_session_state,
};
#[cfg(test)]
use self::state::{
    set_snapshot_logging_enabled, set_vpt_ensemble_session_active, VptEnsembleSessionState,
};

const MIN_DURATION_MS: u64 = 100;
const FALLBACK_DURATION_MS: u64 = 5_000;
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

pub(crate) async fn prepare_vpt_ensemble_startup() {
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

    let result = tokio::task::spawn_blocking(configure_vpt_ensemble_startup)
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

fn configure_vpt_ensemble_startup() -> anyhow::Result<()> {
    if mascot_render_server_healthcheck().is_err() {
        return Ok(());
    }

    let status = mascot_render_server_status()?;
    configure_vpt_ensemble_startup_for_mode(status.ensemble_mode, |character_names| {
        set_vpt_ensemble_mascot_render_server(character_names)
    })
}

fn configure_vpt_ensemble_startup_for_mode<F>(
    startup_mode: ServerEnsembleMode,
    set_vpt_ensemble: F,
) -> anyhow::Result<()>
where
    F: FnOnce(&[String]) -> anyhow::Result<()>,
{
    {
        let mut state = vpt_ensemble_session_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.startup_mode = Some(startup_mode);
        state.active = matches!(startup_mode, ServerEnsembleMode::Vpt);
        state.restore_single_character_on_exit = false;
    }

    if startup_mode != ServerEnsembleMode::Favorite {
        return Ok(());
    }

    let character_names = vpt_ensemble_character_names();
    if character_names.is_empty() {
        bail!("vpt ensemble に使える mascot PSD 付き speaker がありません");
    }

    let address = mascot_render_server_address();
    let request_body = VptEnsembleRequest {
        character_names: character_names.clone(),
    };
    let request = format_mascot_json_request("POST", "/vpt-ensemble", address, &request_body);
    let result = set_vpt_ensemble(&character_names);
    if let Err(error) = log_mascot_request_result("vpt ensemble切替", address, &request, &result)
    {
        report_mascot_log_failure(&error);
    }
    result?;

    let mut state = vpt_ensemble_session_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    state.active = true;
    state.restore_single_character_on_exit = true;
    Ok(())
}

fn restore_vpt_ensemble_session_on_exit() {
    restore_vpt_ensemble_session_on_exit_with(set_single_character_mode_mascot_render_server);
}

fn restore_vpt_ensemble_session_on_exit_with<F>(set_single_character_mode: F) -> bool
where
    F: FnOnce() -> anyhow::Result<()>,
{
    let should_restore = {
        let mut state = vpt_ensemble_session_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let should_restore = state.restore_single_character_on_exit;
        state.restore_single_character_on_exit = false;
        state.active = false;
        should_restore
    };
    if !should_restore {
        return false;
    }

    let address = mascot_render_server_address();
    let request = format_mascot_request("POST", "/ensemble-mode/single-character", address, None);
    let result = set_single_character_mode();
    if let Err(error) =
        log_mascot_request_result("single character mode復元", address, &request, &result)
    {
        report_mascot_log_failure(&error);
    }
    if let Err(error) = result {
        crate::runtime_notice::set_runtime_notice(format!(
            "[mascot-render] 終了時の ensemble mode 復元に失敗しました: {error}"
        ));
    }
    true
}

#[cfg(test)]
fn sync_character_change<F>(address: SocketAddr, speaker: Option<&str>, change_character: F) -> bool
where
    F: FnOnce(&str) -> anyhow::Result<()>,
{
    sync_character_change_with_context(None, None, address, speaker, change_character)
}

#[cfg(test)]
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
    sync_character_change_inner(
        sync_id,
        sync_started_at,
        address,
        speaker,
        None::<fn() -> anyhow::Result<()>>,
        change_character,
    )
}

fn sync_character_change_with_favorite_disable<D, F>(
    sync_id: Option<u64>,
    sync_started_at: Option<Instant>,
    address: SocketAddr,
    speaker: Option<&str>,
    disable_favorite_ensemble: D,
    change_character: F,
) -> bool
where
    D: FnOnce() -> anyhow::Result<()>,
    F: FnOnce(&str) -> anyhow::Result<()>,
{
    sync_character_change_inner(
        sync_id,
        sync_started_at,
        address,
        speaker,
        Some(disable_favorite_ensemble),
        change_character,
    )
}

fn sync_character_change_inner<D, F>(
    sync_id: Option<u64>,
    sync_started_at: Option<Instant>,
    address: SocketAddr,
    speaker: Option<&str>,
    disable_favorite_ensemble: Option<D>,
    change_character: F,
) -> bool
where
    D: FnOnce() -> anyhow::Result<()>,
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
    if let Some(disable_favorite_ensemble) = disable_favorite_ensemble {
        let disable_request =
            format_mascot_request("POST", "/favorite-ensemble/disable", address, None);
        if let Some(sync_id) = sync_id {
            log_playback_snapshots(
                sync_id,
                "favorite-ensemble-disable",
                "before",
                address,
                sync_started_at,
            );
            log_playback_request_start(
                sync_id,
                "favorite-ensemble-disable",
                "favorite ensemble無効化",
                address,
                sync_started_at,
            );
        }
        let disable_started_at = Instant::now();
        let disable_result = disable_favorite_ensemble();
        let disable_duration = disable_started_at.elapsed();
        let disable_log_result = if let Some(sync_id) = sync_id {
            log_mascot_sync_request_result_timed(
                MascotSyncRequestContext {
                    sync_id,
                    phase: "favorite-ensemble-disable",
                    action: "favorite ensemble無効化",
                    address,
                    sync_started_at,
                },
                &disable_request,
                &disable_result,
                disable_duration,
            )
        } else {
            log_mascot_request_result(
                "favorite ensemble無効化",
                address,
                &disable_request,
                &disable_result,
            )
        };
        if let Err(error) = disable_log_result {
            report_mascot_log_failure(&error);
        }
        if let Some(sync_id) = sync_id {
            log_playback_error_snapshots(
                sync_id,
                "favorite-ensemble-disable",
                address,
                sync_started_at,
                &disable_result,
            );
            log_playback_snapshots(
                sync_id,
                "favorite-ensemble-disable",
                "after",
                address,
                sync_started_at,
            );
        }
        if disable_result.is_err() {
            return false;
        }
    }

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

    if !sync_character_change_with_favorite_disable(
        Some(sync_id),
        Some(sync_started_at),
        address,
        sync.char_name.as_deref(),
        || disable_favorite_ensemble_mascot_render_server_at(address),
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
    let request_log = format_mascot_json_request("POST", "/timeline", address, &request);
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
    let timeline_result = play_timeline_mascot_render_server(&request);
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

fn motion_timeline_request(duration_ms: u64) -> MotionTimelineRequest {
    let mut request = preview_mouth_flap_timeline_request();
    if let Some(step) = request.steps.first_mut() {
        step.duration_ms = duration_ms;
    } else {
        request.steps.push(MotionTimelineStep {
            kind: MotionTimelineKind::MouthFlap,
            duration_ms,
            fps: PREVIEW_MOUTH_FLAP_FPS,
        });
    }
    request
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
