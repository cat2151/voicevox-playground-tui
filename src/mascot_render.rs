use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mascot_render_client::{
    change_skin_mascot_render_server, play_timeline_mascot_render_server,
    preview_mouth_flap_timeline_request, show_mascot_render_server, MotionTimelineKind,
    MotionTimelineRequest, MotionTimelineStep, PREVIEW_MOUTH_FLAP_FPS,
};
use serde::Deserialize;

use crate::tag;

const MIN_DURATION_MS: u64 = 100;
const FALLBACK_DURATION_MS: u64 = 5_000;
const DATA_ROOT_ENV: &str = "MASCOT_RENDER_SERVER_DATA_ROOT";
const OVERLAY_DURATION: Duration = Duration::from_secs(5);
const PSD_CACHE_TTL: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq, Eq)]
struct MascotPsdEntry {
    psd_label: String,
    png_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MascotPsdList {
    entries: Vec<MascotPsdEntry>,
    load_reason: Option<String>,
}

#[derive(Debug)]
struct MascotPsdCache {
    cache_dir: Option<PathBuf>,
    loaded_at: Option<Instant>,
    list: MascotPsdList,
}

#[derive(Debug, Deserialize)]
struct MascotPsdMetaFile {
    psds: Vec<MascotPsdMetaEntry>,
}

#[derive(Debug, Deserialize)]
struct MascotPsdMetaEntry {
    file_name: String,
    #[serde(default)]
    path: Option<PathBuf>,
    #[serde(default)]
    rendered_png_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct OverlayMessage {
    text: String,
    expires_at: Instant,
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

fn mascot_psd_cache_slot() -> &'static Mutex<MascotPsdCache> {
    static SLOT: OnceLock<Mutex<MascotPsdCache>> = OnceLock::new();
    SLOT.get_or_init(|| {
        Mutex::new(MascotPsdCache {
            cache_dir: None,
            loaded_at: None,
            list: MascotPsdList {
                entries: Vec::new(),
                load_reason: None,
            },
        })
    })
}

fn mascot_psd_list() -> MascotPsdList {
    let cache_dir = mascot_data_root().map(|path| path.join("cache"));
    let mut cache = mascot_psd_cache_slot().lock().unwrap();
    let now = Instant::now();
    let cache_is_fresh = cache.cache_dir == cache_dir
        && cache
            .loaded_at
            .is_some_and(|loaded_at| now.duration_since(loaded_at) < PSD_CACHE_TTL);

    if cache_is_fresh {
        return cache.list.clone();
    }

    let list = load_mascot_psd_list(cache_dir.as_deref());
    cache.cache_dir = cache_dir;
    cache.loaded_at = Some(now);
    cache.list = list.clone();
    list
}

fn load_mascot_psd_list(cache_dir: Option<&Path>) -> MascotPsdList {
    let Some(cache_dir) = cache_dir else {
        return MascotPsdList {
            entries: Vec::new(),
            load_reason: Some("cache path could not be resolved".to_string()),
        };
    };
    mascot_psd_list_from_cache_dir(cache_dir)
}

fn mascot_psd_list_from_cache_dir(cache_dir: &Path) -> MascotPsdList {
    let entries = match fs::read_dir(cache_dir) {
        Ok(entries) => entries,
        Err(error) => {
            return MascotPsdList {
                entries: Vec::new(),
                load_reason: Some(format!(
                    "cache path could not be read: {} ({error})",
                    cache_dir.display()
                )),
            };
        }
    };

    let mut entries = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().ok().is_some_and(|kind| kind.is_dir()))
        .flat_map(|entry| {
            let meta_path = entry.path().join("psd-meta.json");
            let bytes = match fs::read(&meta_path) {
                Ok(bytes) => bytes,
                Err(_) => return Vec::new(),
            };
            let meta = match serde_json::from_slice::<MascotPsdMetaFile>(&bytes) {
                Ok(meta) => meta,
                Err(_) => return Vec::new(),
            };
            meta.psds
                .into_iter()
                .map(|psd| MascotPsdEntry {
                    psd_label: psd
                        .path
                        .as_ref()
                        .map(|path| path.to_string_lossy().into_owned())
                        .unwrap_or(psd.file_name),
                    png_path: psd.rendered_png_path.filter(|path| {
                        path.extension().and_then(|ext| ext.to_str()) == Some("png")
                            && path.exists()
                    }),
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.psd_label.cmp(&right.psd_label));
    let load_reason = if entries.is_empty() {
        Some(format!(
            "no valid psd-meta.json entries were found under {}",
            cache_dir.display()
        ))
    } else {
        None
    };
    MascotPsdList {
        entries,
        load_reason,
    }
}

fn matching_skin_path(speaker: &str, psd_entries: &[MascotPsdEntry]) -> Option<PathBuf> {
    let speaker = speaker.trim();
    if speaker.is_empty() {
        return None;
    }
    let speaker = speaker.to_lowercase();
    let matches = psd_entries
        .iter()
        .filter(|entry| entry.psd_label.to_lowercase().contains(&speaker))
        .filter_map(|entry| entry.png_path.as_ref())
        .collect::<Vec<_>>();

    matches
        .get(random_index(matches.len())?)
        .map(|path| (*path).clone())
}

fn random_index(len: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.subsec_nanos() as usize)
        .unwrap_or_default();
    Some(nanos % len)
}

fn no_matching_skin_message(speaker: &str, psd_entries: &[MascotPsdEntry]) -> String {
    let psd_list = psd_entries
        .iter()
        .map(|entry| entry.psd_label.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!("hitしませんでした。speaker:{speaker} psdのlist:{psd_list}")
}

fn no_matching_skin_message_for_list(speaker: &str, psd_list: &MascotPsdList) -> String {
    if psd_list.entries.is_empty() {
        let reason = psd_list
            .load_reason
            .as_deref()
            .unwrap_or("psd list is empty");
        return format!("hitしませんでした。speaker:{speaker} psdのlist:({reason})");
    }
    no_matching_skin_message(speaker, &psd_list.entries)
}

fn overlay_message_slot() -> &'static Mutex<Option<OverlayMessage>> {
    static SLOT: OnceLock<Mutex<Option<OverlayMessage>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn set_overlay_message(text: String) {
    *overlay_message_slot().lock().unwrap() = Some(OverlayMessage {
        text,
        expires_at: Instant::now() + OVERLAY_DURATION,
    });
}

fn clear_overlay_message() {
    *overlay_message_slot().lock().unwrap() = None;
}

pub(crate) fn current_overlay_message() -> Option<String> {
    let mut slot = overlay_message_slot().lock().unwrap();
    match slot.as_ref() {
        Some(message) if message.expires_at > Instant::now() => Some(message.text.clone()),
        Some(_) => {
            *slot = None;
            None
        }
        None => None,
    }
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

    if let Some(speaker) = sync.char_name.as_deref() {
        let psd_list = mascot_psd_list();
        if let Some(png_path) = matching_skin_path(speaker, &psd_list.entries) {
            clear_overlay_message();
            let _ = change_skin_mascot_render_server(&png_path);
        } else {
            set_overlay_message(no_matching_skin_message_for_list(speaker, &psd_list));
        }
    } else {
        clear_overlay_message();
    }

    let request = motion_timeline_request(sync.duration_ms);
    let _ = play_timeline_mascot_render_server(&request);
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
#[path = "tests/mascot_render.rs"]
mod tests;
