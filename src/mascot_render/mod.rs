use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
#[cfg(test)]
use std::sync::Mutex;
use std::sync::OnceLock;
use std::thread;

use mascot_render_client::{
    change_character_mascot_render_server, mascot_render_server_address,
    play_timeline_mascot_render_server, preview_mouth_flap_timeline_request,
    show_mascot_render_server, PREVIEW_MOUTH_FLAP_FPS,
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
    report_mascot_log_failure,
};
use self::overlay::clear_overlay_message;
#[cfg(test)]
use self::overlay::set_blocking_overlay_message;
#[cfg(test)]
use self::overlay::set_overlay_message;
pub(crate) use self::overlay::{
    current_overlay_message, dismiss_blocking_overlay_message, has_blocking_overlay_message,
};

const MIN_DURATION_MS: u64 = 100;
const FALLBACK_DURATION_MS: u64 = 5_000;
const DATA_ROOT_ENV: &str = "MASCOT_RENDER_SERVER_DATA_ROOT";
#[cfg(test)]
const OVERLAY_DURATION: std::time::Duration = std::time::Duration::from_secs(5);

pub fn sync_playback(line: &str, wav: &[u8]) {
    if line.trim().is_empty() || wav.is_empty() {
        return;
    }

    let line = line.to_string();
    let duration_ms = wav_duration_ms(wav).unwrap_or(FALLBACK_DURATION_MS);
    let char_name = mascot_char_name_for_line(&line);

    let _ = mascot_worker_tx().send(MascotPlaybackSync {
        char_name,
        duration_ms,
    });
}

#[derive(Debug)]
struct MascotPlaybackSync {
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
                handle_playback_sync(sync);
            }
        });
        tx
    })
}

fn sync_character_change<F>(address: SocketAddr, speaker: Option<&str>, change_character: F) -> bool
where
    F: FnOnce(&str) -> anyhow::Result<()>,
{
    let Some(speaker) = speaker else {
        clear_overlay_message();
        return true;
    };

    clear_overlay_message();
    let request_body = ChangeCharacterRequest {
        character_name: speaker.to_string(),
    };
    let request = format_mascot_json_request("POST", "/change-character", address, &request_body);
    let change_character_result = change_character(speaker);
    if let Err(error) = log_mascot_request_result(
        &format!("{speaker} へのcharacter変更"),
        address,
        &request,
        &change_character_result,
    ) {
        report_mascot_log_failure(&error);
    }
    change_character_result.is_ok()
}

fn handle_playback_sync(sync: MascotPlaybackSync) {
    let address = mascot_render_server_address();

    let show_request = format_mascot_request("POST", "/show", address, None);
    let show_result = show_mascot_render_server();
    if let Err(error) = log_mascot_request_result("表示", address, &show_request, &show_result) {
        report_mascot_log_failure(&error);
    }

    if !sync_character_change(
        address,
        sync.char_name.as_deref(),
        change_character_mascot_render_server,
    ) {
        return;
    }

    let request = motion_timeline_request(sync.duration_ms);
    let request_log = format_mascot_json_request("POST", "/timeline", address, &request);
    let action = sync
        .char_name
        .as_deref()
        .map(|speaker| format!("{speaker} の口パク"))
        .unwrap_or_else(|| "口パク".to_string());
    let timeline_result = play_timeline_mascot_render_server(&request);
    if let Err(error) = log_mascot_request_result(&action, address, &request_log, &timeline_result)
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

#[cfg(test)]
pub(crate) fn with_overlay_state_lock<T>(f: impl FnOnce() -> T) -> T {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    dismiss_blocking_overlay_message();
    clear_overlay_message();
    let result = f();
    dismiss_blocking_overlay_message();
    clear_overlay_message();
    result
}

#[cfg(test)]
#[path = "../tests/mascot_render.rs"]
mod tests;
