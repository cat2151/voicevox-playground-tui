use std::fs;
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::Local;
use mascot_render_client::{
    change_skin_mascot_render_server, mascot_render_server_address,
    play_timeline_mascot_render_server, preview_mouth_flap_timeline_request,
    show_mascot_render_server, ChangeSkinRequest, MotionTimelineKind, MotionTimelineRequest,
    MotionTimelineStep, PREVIEW_MOUTH_FLAP_FPS,
};
use serde::{Deserialize, Serialize};

use crate::tag;

const MIN_DURATION_MS: u64 = 100;
const FALLBACK_DURATION_MS: u64 = 5_000;
const DATA_ROOT_ENV: &str = "MASCOT_RENDER_SERVER_DATA_ROOT";
const LOG_FILE_NAME: &str = "voicevox-playground-tui.log";
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
    expires_at: Option<Instant>,
    dismiss_with_enter: bool,
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
    let mut slot = overlay_message_slot().lock().unwrap();
    if slot
        .as_ref()
        .is_some_and(|message| message.dismiss_with_enter)
    {
        return;
    }
    *slot = Some(OverlayMessage {
        text,
        expires_at: Some(Instant::now() + OVERLAY_DURATION),
        dismiss_with_enter: false,
    });
}

fn set_blocking_overlay_message(text: String) {
    *overlay_message_slot().lock().unwrap() = Some(OverlayMessage {
        text,
        expires_at: None,
        dismiss_with_enter: true,
    });
}

fn clear_overlay_message() {
    let mut slot = overlay_message_slot().lock().unwrap();
    if slot
        .as_ref()
        .is_some_and(|message| message.dismiss_with_enter)
    {
        return;
    }
    *slot = None;
}

pub(crate) fn current_overlay_message() -> Option<(String, bool)> {
    let mut slot = overlay_message_slot().lock().unwrap();
    match slot.as_ref() {
        Some(message) if message.dismiss_with_enter => {
            Some((message.text.clone(), message.dismiss_with_enter))
        }
        Some(message)
            if message
                .expires_at
                .is_some_and(|expires_at| expires_at > Instant::now()) =>
        {
            Some((message.text.clone(), message.dismiss_with_enter))
        }
        Some(_) => {
            *slot = None;
            None
        }
        None => None,
    }
}

pub(crate) fn has_blocking_overlay_message() -> bool {
    overlay_message_slot()
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|message| message.dismiss_with_enter)
}

pub(crate) fn dismiss_blocking_overlay_message() {
    let mut slot = overlay_message_slot().lock().unwrap();
    if slot
        .as_ref()
        .is_some_and(|message| message.dismiss_with_enter)
    {
        *slot = None;
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

fn indented_lines(text: &str) -> String {
    text.lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_mascot_request(
    method: &str,
    path: &str,
    address: SocketAddr,
    body: Option<(&str, usize)>,
) -> String {
    let content_length = body.map(|(_, len)| len).unwrap_or_default();
    let mut headers = vec![
        format!("{method} {path} HTTP/1.1"),
        format!("Host: {address}"),
        "Connection: close".to_string(),
        format!("Content-Length: {content_length}"),
    ];
    if body.is_some() {
        headers.push("Content-Type: application/json".to_string());
    }

    let mut sections = vec!["header:".to_string(), indented_lines(&headers.join("\n"))];
    if let Some((body, _)) = body {
        sections.push("body:".to_string());
        sections.push(indented_lines(body));
    }
    sections.join("\n")
}

fn format_mascot_json_request<T: Serialize>(
    method: &str,
    path: &str,
    address: SocketAddr,
    body: &T,
) -> String {
    let (compact_body, pretty_body) = match serde_json::to_vec(body) {
        Ok(compact_body) => {
            let pretty_body = serde_json::to_string_pretty(body)
                .unwrap_or_else(|_| String::from_utf8_lossy(&compact_body).into_owned());
            (compact_body, pretty_body)
        }
        Err(error) => {
            let fallback_value = serde_json::json!({
                "serialization_error": error.to_string(),
            });
            let compact_body = serde_json::to_vec(&fallback_value).unwrap_or_else(|_| {
                b"{\"serialization_error\":\"failed to encode logging fallback\"}".to_vec()
            });
            let pretty_body = serde_json::to_string_pretty(&fallback_value)
                .unwrap_or_else(|_| String::from_utf8_lossy(&compact_body).into_owned());
            (compact_body, pretty_body)
        }
    };
    format_mascot_request(
        method,
        path,
        address,
        Some((&pretty_body, compact_body.len())),
    )
}

fn current_log_timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S%:z").to_string()
}

fn format_mascot_log_message(message: &str) -> String {
    format!("[{}] [mascot-render] {message}", current_log_timestamp())
}

fn mascot_log_path() -> Option<PathBuf> {
    mascot_data_root().map(|root| root.join(LOG_FILE_NAME))
}

fn append_mascot_log(message: &str) -> anyhow::Result<()> {
    let Some(path) = mascot_log_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{message}")?;
    Ok(())
}

fn log_mascot_request_result(
    action: &str,
    address: SocketAddr,
    request: &str,
    result: &Result<(), anyhow::Error>,
) {
    match result {
        Ok(()) => {
            let _ = append_mascot_log(&format!(
                "{}\nrequest:\n{request}",
                format_mascot_log_message(&format!(
                    "port {} に {action}request を送信しました。",
                    address.port()
                ))
            ));
        }
        Err(error) => {
            let message = format!(
                "{}\nrequest:\n{request}",
                format_mascot_log_message(&format!(
                    "port {} への {action}request 送信に失敗しました: {error}",
                    address.port()
                ))
            );
            let _ = append_mascot_log(&message);
            set_blocking_overlay_message(message);
        }
    }
}

fn handle_playback_sync(sync: MascotPlaybackSync) {
    let address = mascot_render_server_address();

    let show_request = format_mascot_request("POST", "/show", address, None);
    let show_result = show_mascot_render_server();
    log_mascot_request_result("表示", address, &show_request, &show_result);

    if let Some(speaker) = sync.char_name.as_deref() {
        let psd_list = mascot_psd_list();
        if let Some(png_path) = matching_skin_path(speaker, &psd_list.entries) {
            clear_overlay_message();
            let change_skin_request = ChangeSkinRequest {
                png_path: png_path.clone(),
            };
            let request =
                format_mascot_json_request("POST", "/change-skin", address, &change_skin_request);
            let change_skin_result = change_skin_mascot_render_server(&png_path);
            log_mascot_request_result(
                &format!("{speaker} へのskin変更"),
                address,
                &request,
                &change_skin_result,
            );
        } else {
            set_overlay_message(no_matching_skin_message_for_list(speaker, &psd_list));
        }
    } else {
        clear_overlay_message();
    }

    let request = motion_timeline_request(sync.duration_ms);
    let request_log = format_mascot_json_request("POST", "/timeline", address, &request);
    let action = sync
        .char_name
        .as_deref()
        .map(|speaker| format!("{speaker} の口パク"))
        .unwrap_or_else(|| "口パク".to_string());
    let timeline_result = play_timeline_mascot_render_server(&request);
    log_mascot_request_result(&action, address, &request_log, &timeline_result);
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
