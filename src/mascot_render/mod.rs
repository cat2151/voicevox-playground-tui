use std::io::{BufRead, BufReader, Read, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use mascot_render_client::{
    change_character_mascot_render_server, mascot_render_server_address,
    mascot_render_server_psd_file_names, play_timeline_mascot_render_server,
    preview_mouth_flap_timeline_request, show_mascot_render_server, PREVIEW_MOUTH_FLAP_FPS,
};
use mascot_render_protocol::{
    ChangeCharacterRequest, MotionTimelineKind, MotionTimelineRequest, MotionTimelineStep,
};

use crate::tag;

mod logging;
mod overlay;
#[cfg(test)]
mod test_support;

#[cfg(test)]
use self::logging::{current_log_timestamp, format_mascot_log_message, mascot_log_path};
use self::logging::{
    format_mascot_json_request, format_mascot_request, log_mascot_request_result,
    log_mascot_sync_event, log_mascot_sync_request_result_timed, log_mascot_sync_request_start,
    log_mascot_sync_snapshots_timed, report_mascot_log_failure, MascotSyncRequestContext,
};
use self::overlay::clear_overlay_message;
#[cfg(test)]
use self::overlay::set_overlay_message;
pub(crate) use self::overlay::{
    clear_startup_overlay_message, current_overlay_message, current_startup_overlay_message,
    dismiss_blocking_overlay_message, has_blocking_overlay_message, set_blocking_overlay_message,
    set_startup_overlay_message,
};

const MIN_DURATION_MS: u64 = 100;
const FALLBACK_DURATION_MS: u64 = 5_000;
const DATA_ROOT_ENV: &str = "MASCOT_RENDER_SERVER_DATA_ROOT";
const MASCOT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const MASCOT_IO_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(test)]
const OVERLAY_DURATION: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug, Default)]
struct MascotPsdAvailability {
    normalized_file_names: Vec<String>,
}

pub fn sync_playback(line: &str, wav: &[u8]) {
    if line.trim().is_empty() || wav.is_empty() {
        return;
    }
    if is_startup_in_progress() {
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

fn mascot_char_name_for_line(line: &str) -> Option<String> {
    let mut segments = tag::parse_line(line).into_iter();
    let (_, first_ctx) = segments.next()?;
    let first = first_ctx.char_name;

    if segments.all(|(_, ctx)| ctx.char_name == first) {
        Some(first)
    } else {
        None
    }
}

fn mascot_psd_availability() -> &'static Mutex<MascotPsdAvailability> {
    static AVAILABILITY: OnceLock<Mutex<MascotPsdAvailability>> = OnceLock::new();
    AVAILABILITY.get_or_init(|| Mutex::new(MascotPsdAvailability::default()))
}

fn set_loaded_psd_file_names(file_names: Vec<String>) {
    let normalized_file_names = file_names
        .into_iter()
        .map(|file_name| normalize_mascot_lookup_text(&file_name))
        .filter(|file_name| !file_name.is_empty())
        .collect();
    *mascot_psd_availability()
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = MascotPsdAvailability {
        normalized_file_names,
    };
}

pub(crate) fn refresh_available_psd_file_names_from_server() -> anyhow::Result<usize> {
    let file_names = mascot_render_server_psd_file_names()?;
    let count = file_names.len();
    set_loaded_psd_file_names(file_names);
    Ok(count)
}

pub(crate) fn speaker_has_psd(speaker: &str) -> bool {
    let normalized_speaker = normalize_mascot_lookup_text(speaker);
    if normalized_speaker.is_empty() {
        return false;
    }

    mascot_psd_availability()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .normalized_file_names
        .iter()
        .any(|file_name| file_name.contains(&normalized_speaker))
}

fn normalize_mascot_lookup_text(text: &str) -> String {
    trim_psd_extension(text.trim())
        .chars()
        .filter(|ch| {
            !matches!(
                ch,
                '/' | '\\' | '_' | '-' | ' ' | '　' | '.' | '(' | ')' | '[' | ']'
            )
        })
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn trim_psd_extension(text: &str) -> &str {
    match text.rsplit_once('.') {
        Some((stem, ext)) if ext.eq_ignore_ascii_case("psd") => stem,
        _ => text,
    }
}

fn wav_duration_ms(wav: &[u8]) -> Option<u64> {
    if wav.len() < 44 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }

    let byte_rate = u32::from_le_bytes(wav.get(28..32)?.try_into().ok()?);
    let data_len = u32::from_le_bytes(wav.get(40..44)?.try_into().ok()?);
    if byte_rate == 0 {
        return None;
    }

    let duration_ms = ((data_len as u64) * 1000).div_ceil(byte_rate as u64);
    Some(duration_ms.max(MIN_DURATION_MS))
}

pub(crate) fn init_data_root_env() {
    if std::env::var_os(DATA_ROOT_ENV).is_none() {
        if let Some(root) = default_mascot_data_root() {
            std::env::set_var(DATA_ROOT_ENV, root);
        }
    }
}

fn default_mascot_data_root() -> Option<PathBuf> {
    dirs::data_local_dir().map(|base| base.join("mascot-render-server"))
}

#[cfg(test)]
fn mascot_data_root() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os(DATA_ROOT_ENV) {
        let path = PathBuf::from(root);
        return if path.is_absolute() {
            Some(path)
        } else {
            dirs::data_local_dir().map(|base| base.join(path))
        };
    }

    default_mascot_data_root()
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

fn startup_in_progress_flag() -> &'static AtomicBool {
    static FLAG: OnceLock<AtomicBool> = OnceLock::new();
    FLAG.get_or_init(|| AtomicBool::new(false))
}

fn is_startup_in_progress() -> bool {
    startup_in_progress_flag().load(Ordering::Relaxed)
}

pub(crate) fn set_startup_in_progress(in_progress: bool) {
    startup_in_progress_flag().store(in_progress, Ordering::Relaxed);
}

fn snapshot_logging_enabled_flag() -> &'static AtomicBool {
    static FLAG: OnceLock<AtomicBool> = OnceLock::new();
    FLAG.get_or_init(|| AtomicBool::new(false))
}

fn snapshot_logging_initialized_flag() -> &'static AtomicBool {
    static FLAG: OnceLock<AtomicBool> = OnceLock::new();
    FLAG.get_or_init(|| AtomicBool::new(false))
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

pub(crate) fn set_snapshot_logging_enabled(enabled: bool) {
    snapshot_logging_enabled_flag().store(enabled, Ordering::Relaxed);
    snapshot_logging_initialized_flag().store(true, Ordering::Relaxed);
}

fn snapshot_logging_enabled() -> bool {
    if !snapshot_logging_initialized_flag().load(Ordering::Relaxed) {
        init_snapshot_logging_from_config();
    }
    snapshot_logging_enabled_flag().load(Ordering::Relaxed)
}

fn next_mascot_sync_id() -> u64 {
    static NEXT_SYNC_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_SYNC_ID.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
fn sync_character_change<F>(address: SocketAddr, speaker: Option<&str>, change_character: F) -> bool
where
    F: FnOnce(&str) -> anyhow::Result<()>,
{
    sync_character_change_with_context(None, None, address, speaker, || Ok(()), change_character)
}

fn sync_character_change_with_context<D, F>(
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

fn event_details_with_elapsed(sync_started_at: Option<Instant>, details: &str) -> String {
    if let Some(started_at) = sync_started_at {
        format!("elapsed_ms={} {details}", started_at.elapsed().as_millis())
    } else {
        details.to_string()
    }
}

fn log_playback_event(sync_id: u64, phase: &str, event: &str, details: &str) {
    if let Err(error) = log_mascot_sync_event(sync_id, phase, event, details) {
        report_mascot_log_failure(&error);
    }
}

fn log_playback_request_start(
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

fn log_playback_snapshots(
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

fn log_playback_error_snapshots(
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

fn disable_favorite_ensemble_mascot_render_server_at(address: SocketAddr) -> anyhow::Result<()> {
    post_empty_mascot_request(address, "/favorite-ensemble/disable")
}

fn post_empty_mascot_request(address: SocketAddr, path: &str) -> anyhow::Result<()> {
    let mut stream = std::net::TcpStream::connect_timeout(&address, MASCOT_CONNECT_TIMEOUT)
        .with_context(|| format!("failed to connect to mascot-render-server at {address}"))?;
    stream
        .set_read_timeout(Some(MASCOT_IO_TIMEOUT))
        .with_context(|| format!("failed to set read timeout for {address}"))?;
    stream
        .set_write_timeout(Some(MASCOT_IO_TIMEOUT))
        .with_context(|| format!("failed to set write timeout for {address}"))?;

    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .with_context(|| format!("failed to write HTTP request to {address}"))?;
    stream
        .flush()
        .with_context(|| format!("failed to flush HTTP request to {address}"))?;

    read_empty_post_response(&mut stream, path)
}

fn read_empty_post_response(stream: &mut std::net::TcpStream, path: &str) -> anyhow::Result<()> {
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .context("failed to read HTTP response status line")?;
    if status_line.trim().is_empty() {
        bail!("empty HTTP response");
    }

    let status_code = parse_http_status_code(&status_line)?;
    let mut content_length = 0usize;
    let mut header_line = String::new();
    loop {
        header_line.clear();
        reader
            .read_line(&mut header_line)
            .context("failed to read HTTP response header")?;
        if header_line == "\r\n" || header_line == "\n" {
            break;
        }
        if let Some((name, value)) = header_line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value
                    .trim()
                    .parse::<usize>()
                    .context("invalid HTTP response Content-Length header")?;
            }
        }
    }

    let mut body = vec![0; content_length];
    if !body.is_empty() {
        reader
            .read_exact(&mut body)
            .context("failed to read HTTP response body")?;
    }

    if (200..300).contains(&status_code) {
        return Ok(());
    }

    bail!(
        "mascot-render-server request {path} failed with HTTP {}: {}",
        status_code,
        String::from_utf8_lossy(&body).trim()
    )
}

fn parse_http_status_code(status_line: &str) -> anyhow::Result<u16> {
    status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("missing HTTP status code"))?
        .parse::<u16>()
        .context("invalid HTTP status code")
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
    let result = f();
    set_startup_in_progress(false);
    dismiss_blocking_overlay_message();
    clear_overlay_message();
    clear_startup_overlay_message();
    set_snapshot_logging_enabled(false);
    set_loaded_psd_file_names(Vec::new());
    result
}

#[cfg(test)]
pub(crate) fn set_loaded_psd_file_names_for_test(file_names: &[&str]) {
    set_loaded_psd_file_names(file_names.iter().map(ToString::to_string).collect());
}

#[cfg(test)]
#[path = "../tests/mascot_render/mod.rs"]
mod tests;
