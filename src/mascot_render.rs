use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::thread;

use mascot_render_client::{
    change_skin_mascot_render_server, play_timeline_mascot_render_server,
    preview_mouth_flap_timeline_request, show_mascot_render_server, MotionTimelineRequest,
};
use serde::Deserialize;

use crate::tag;

const MIN_DURATION_MS: u64 = 100;
const FALLBACK_DURATION_MS: u64 = 5_000;
const ZUNDAMON_CHAR_NAME: &str = "ずんだもん";
const ZUNDAMON_PNG_PATH_ENV: &str = "MASCOT_RENDER_SERVER_ZUNDAMON_PNG_PATH";
const DATA_ROOT_ENV: &str = "MASCOT_RENDER_SERVER_DATA_ROOT";

#[derive(Debug, Deserialize)]
struct MascotRuntimeState {
    png_path: PathBuf,
}

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

fn zundamon_png_path() -> Option<PathBuf> {
    env_png_path(ZUNDAMON_PNG_PATH_ENV).or_else(runtime_state_png_path)
}

fn env_png_path(name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os(name)?);
    (path.extension().and_then(|ext| ext.to_str()) == Some("png") && path.exists()).then_some(path)
}

fn runtime_state_png_path() -> Option<PathBuf> {
    let cache_dir = mascot_data_root()?.join("cache");
    let state_path = newest_runtime_state_path(&cache_dir)?;
    let bytes = fs::read(&state_path).ok()?;
    let state: MascotRuntimeState = serde_json::from_slice(&bytes).ok()?;
    (state.png_path.extension().and_then(|ext| ext.to_str()) == Some("png")
        && state.png_path.exists())
    .then_some(state.png_path)
}

fn mascot_data_root() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os(DATA_ROOT_ENV) {
        let path = PathBuf::from(root);
        return if path.is_absolute() {
            Some(path)
        } else {
            dirs::data_local_dir().map(|base| base.join(path))
        };
    }

    dirs::data_local_dir().map(|base| base.join("mascot-render-server"))
}

fn newest_runtime_state_path(cache_dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(cache_dir).ok()?;
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let file_name = path.file_name()?.to_str()?;
            if !file_name.starts_with("mascot-render-server-")
                || !file_name.ends_with(".state.json")
            {
                return None;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, path))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
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

fn handle_playback_sync(sync: MascotPlaybackSync) {
    let _ = show_mascot_render_server();

    if sync.char_name.as_deref() == Some(ZUNDAMON_CHAR_NAME) {
        if let Some(png_path) = zundamon_png_path() {
            let _ = change_skin_mascot_render_server(&png_path);
        }
    }

    let request = motion_timeline_request(sync.duration_ms);
    let _ = play_timeline_mascot_render_server(&request);
}

fn motion_timeline_request(duration_ms: u64) -> MotionTimelineRequest {
    let mut request = preview_mouth_flap_timeline_request();
    let step = request
        .steps
        .first_mut()
        .expect("preview_mouth_flap_timeline_request() must contain a step");
    step.duration_ms = duration_ms;
    request
}

#[cfg(test)]
#[path = "tests/mascot_render.rs"]
mod tests;
