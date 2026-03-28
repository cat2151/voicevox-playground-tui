use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::tag;

const MASCOT_RENDER_SERVER_PORT: u16 = 62152;
const MASCOT_RENDER_SERVER_HOST: &str = "127.0.0.1";
const IO_TIMEOUT: Duration = Duration::from_millis(200);
const TIMELINE_FPS: u16 = 20;
const MIN_DURATION_MS: u64 = 100;
const FALLBACK_DURATION_MS: u64 = 5_000;
const ZUNDAMON_CHAR_NAME: &str = "ずんだもん";
const ZUNDAMON_PNG_PATH_ENV: &str = "MASCOT_RENDER_SERVER_ZUNDAMON_PNG_PATH";
const DATA_ROOT_ENV: &str = "MASCOT_RENDER_SERVER_DATA_ROOT";

#[derive(Debug, Serialize)]
struct ChangeSkinRequest<'a> {
    png_path: &'a Path,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum MotionTimelineKind {
    Shake,
}

#[derive(Debug, Serialize)]
struct MotionTimelineStep {
    kind: MotionTimelineKind,
    duration_ms: u64,
    fps: u16,
}

#[derive(Debug, Serialize)]
struct MotionTimelineRequest {
    steps: Vec<MotionTimelineStep>,
}

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

fn server_address() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], MASCOT_RENDER_SERVER_PORT))
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
    let _ = send_request(server_address(), "POST", "/show", None);

    if sync.char_name.as_deref() == Some(ZUNDAMON_CHAR_NAME) {
        if let Some(png_path) = zundamon_png_path() {
            let body = serde_json::to_vec(&ChangeSkinRequest {
                png_path: &png_path,
            })
            .ok();
            if let Some(body) = body.as_deref() {
                let _ = send_request(server_address(), "POST", "/change-skin", Some(body));
            }
        }
    }

    let request = MotionTimelineRequest {
        steps: vec![MotionTimelineStep {
            kind: MotionTimelineKind::Shake,
            duration_ms: sync.duration_ms,
            fps: TIMELINE_FPS,
        }],
    };
    if let Ok(body) = serde_json::to_vec(&request) {
        let _ = send_request(server_address(), "POST", "/timeline", Some(&body));
    }
}

fn send_request(
    address: SocketAddr,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
) -> Result<(), ()> {
    let mut stream = TcpStream::connect_timeout(&address, IO_TIMEOUT).map_err(|_| ())?;
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));

    let body = body.unwrap_or_default();
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {MASCOT_RENDER_SERVER_HOST}:{port}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len(),
        port = address.port()
    );
    if !body.is_empty() {
        request.push_str("Content-Type: application/json\r\n");
    }
    request.push_str("\r\n");

    stream.write_all(request.as_bytes()).map_err(|_| ())?;
    if !body.is_empty() {
        stream.write_all(body).map_err(|_| ())?;
    }
    stream.flush().map_err(|_| ())?;

    read_response(stream)
}

fn read_response(stream: TcpStream) -> Result<(), ()> {
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).map_err(|_| ())?;
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or(())?;
    if !(200..300).contains(&status_code) {
        return Err(());
    }

    let mut content_length = 0usize;
    let mut line = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line).map_err(|_| ())?;
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse::<usize>().map_err(|_| ())?;
            }
        }
    }

    if content_length > 0 {
        let mut body = vec![0; content_length];
        reader.read_exact(&mut body).map_err(|_| ())?;
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests/mascot_render.rs"]
mod tests;
