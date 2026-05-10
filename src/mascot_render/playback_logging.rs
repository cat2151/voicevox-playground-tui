use std::net::SocketAddr;
use std::time::Instant;

use super::logging::{
    log_mascot_sync_event, log_mascot_sync_request_start, log_mascot_sync_snapshots_timed,
    report_mascot_log_failure, MascotSyncRequestContext,
};
use super::snapshot_logging_enabled;

pub(crate) fn event_details_with_elapsed(
    sync_started_at: Option<Instant>,
    details: &str,
) -> String {
    if let Some(started_at) = sync_started_at {
        format!("elapsed_ms={} {details}", started_at.elapsed().as_millis())
    } else {
        details.to_string()
    }
}

pub(crate) fn log_playback_event(sync_id: u64, phase: &str, event: &str, details: &str) {
    if let Err(error) = log_mascot_sync_event(sync_id, phase, event, details) {
        report_mascot_log_failure(&error);
    }
}

pub(crate) fn log_playback_request_start(
    sync_id: u64,
    phase: &str,
    action: &str,
    address: SocketAddr,
    sync_started_at: Option<Instant>,
) {
    if let Err(error) = log_mascot_sync_request_start(MascotSyncRequestContext {
        sync_id,
        phase,
        action,
        address,
        sync_started_at,
    }) {
        report_mascot_log_failure(&error);
    }
}

pub(crate) fn log_playback_snapshots(
    sync_id: u64,
    phase: &str,
    timing: &str,
    address: SocketAddr,
    sync_started_at: Option<Instant>,
) {
    if !snapshot_logging_enabled() {
        return;
    }
    log_playback_snapshots_forced(sync_id, phase, timing, address, sync_started_at);
}

pub(crate) fn log_playback_error_snapshots(
    sync_id: u64,
    phase: &str,
    address: SocketAddr,
    sync_started_at: Option<Instant>,
    request_result: &anyhow::Result<()>,
) {
    if request_result.is_ok() || snapshot_logging_enabled() {
        return;
    }
    log_playback_snapshots_forced(sync_id, phase, "error", address, sync_started_at);
}

fn log_playback_snapshots_forced(
    sync_id: u64,
    phase: &str,
    timing: &str,
    address: SocketAddr,
    sync_started_at: Option<Instant>,
) {
    if let Err(error) =
        log_mascot_sync_snapshots_timed(sync_id, phase, timing, address, sync_started_at)
    {
        report_mascot_log_failure(&error);
    }
}
