use std::net::SocketAddr;
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::thread;
use std::time::Instant;

use mascot_render_client::{
    change_character_mascot_render_server, mascot_render_server_address, show_mascot_render_server,
};
use mascot_render_protocol::{ChangeCharacterRequest, ServerEnsembleMode};

use super::{
    clear_overlay_message, dismiss_blocking_overlay_message, event_details_with_elapsed,
    format_mascot_json_request, format_mascot_request, log_mascot_request_result,
    log_mascot_sync_request_result_timed, log_playback_error_snapshots, log_playback_event,
    log_playback_request_start, log_playback_snapshots, mascot_char_name_for_line,
    motion_timeline_request, motion_timeline_request_body, next_mascot_sync_id,
    play_timeline_mascot_render_server_with_target, report_mascot_log_failure, speaker_has_psd,
    sync_vpt_ensemble_session_from_server_mode, vpt_ensemble_session_active, wav_duration_ms,
    MascotSyncRequestContext, FALLBACK_DURATION_MS,
};
use crate::mascot_render::{is_startup_in_progress, is_vpt_ensemble_startup_in_progress};

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
                        "queue_wait_ms={} duration_ms={} char_name={:?}",
                        queue_wait_ms, sync.duration_ms, &sync.char_name
                    ),
                );
                handle_playback_sync(sync, received_at);
            }
        });
        tx
    })
}

#[cfg(test)]
pub(super) fn sync_character_change<F>(
    address: SocketAddr,
    speaker: Option<&str>,
    change_character: F,
) -> bool
where
    F: FnOnce(&str) -> anyhow::Result<()>,
{
    sync_character_change_with_context(None, None, address, speaker, change_character)
}

pub(super) fn sync_character_change_with_context<F>(
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
    let rejected_by_vpt_ensemble =
        change_character_error_indicates_vpt_ensemble_active(&change_character_result);
    if let Some(sync_id) = sync_id {
        if rejected_by_vpt_ensemble {
            log_playback_event(
                sync_id,
                "change-character",
                "request_recovered",
                &event_details_with_elapsed(
                    sync_started_at,
                    "reason=server_reported_vpt_ensemble_active",
                ),
            );
        } else {
            log_playback_error_snapshots(
                sync_id,
                "change-character",
                address,
                sync_started_at,
                &change_character_result,
            );
        }
        log_playback_snapshots(
            sync_id,
            "change-character",
            "after",
            address,
            sync_started_at,
        );
    }
    if rejected_by_vpt_ensemble {
        sync_vpt_ensemble_session_from_server_mode(ServerEnsembleMode::Vpt);
        dismiss_blocking_overlay_message();
        clear_overlay_message();
        return true;
    }
    change_character_result.is_ok()
}

fn change_character_error_indicates_vpt_ensemble_active(result: &anyhow::Result<()>) -> bool {
    let Err(error) = result else {
        return false;
    };
    let message = format!("{error:#}");
    message.contains("ensemble_mode=Vpt")
        && message.contains("cannot change character while ensemble mode is active")
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
                elapsed_ms, sync.duration_ms
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
            elapsed_ms, sync.duration_ms
        ),
    );
}
