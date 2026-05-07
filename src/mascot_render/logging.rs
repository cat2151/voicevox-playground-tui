use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use chrono::Local;
use serde::Serialize;

use super::overlay::set_blocking_overlay_message;

const REQUEST_LOG_DIR_NAME: &str = "logs";
const REQUEST_LOG_FILE_NAME: &str = "request.log";
const SNAPSHOT_ENDPOINTS: &[&str] = &["/status", "/placement/anchor-plan"];
const SNAPSHOT_TIMEOUT: Duration = Duration::from_millis(500);
const MAX_SNAPSHOT_RESPONSE_BYTES: usize = 256 * 1024;

#[derive(Debug)]
struct MascotHttpSnapshot {
    status_line: Option<String>,
    body: String,
    truncated: bool,
    timed_out_after_partial: bool,
}

fn indented_lines(text: &str) -> String {
    text.lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn format_mascot_request(
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

pub(super) fn format_mascot_json_request<T: Serialize>(
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

pub(super) fn current_log_timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S%:z").to_string()
}

pub(super) fn format_mascot_log_message(message: &str) -> String {
    format!("[{}] [mascot-render] {message}", current_log_timestamp())
}

fn sync_log_prefix(sync_id: u64, phase: &str) -> String {
    format!("sync_id={sync_id} phase={phase}")
}

pub(super) fn mascot_log_path() -> Option<PathBuf> {
    Some(
        crate::history::history_dir()
            .join(REQUEST_LOG_DIR_NAME)
            .join(REQUEST_LOG_FILE_NAME),
    )
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

pub(super) fn report_mascot_log_failure(error: &anyhow::Error) {
    crate::runtime_notice::set_runtime_notice(format!(
        "[mascot-render] ログ書き込みに失敗しました: {error}"
    ));
}

pub(super) fn log_mascot_request_result(
    action: &str,
    address: SocketAddr,
    request: &str,
    result: &Result<(), anyhow::Error>,
) -> anyhow::Result<()> {
    match result {
        Ok(()) => append_mascot_log(&format!(
            "{}\nrequest:\n{request}",
            format_mascot_log_message(&format!(
                "port {} に {action}request を送信しました。",
                address.port()
            ))
        )),
        Err(error) => {
            let message = format!(
                "{}\nrequest:\n{request}",
                format_mascot_log_message(&format!(
                    "port {} への {action}request 送信に失敗しました: {error}",
                    address.port()
                ))
            );
            set_blocking_overlay_message(&message);
            append_mascot_log(&message)
        }
    }
}

pub(super) fn log_mascot_sync_request_result(
    sync_id: u64,
    phase: &str,
    action: &str,
    address: SocketAddr,
    request: &str,
    result: &Result<(), anyhow::Error>,
) -> anyhow::Result<()> {
    let prefix = sync_log_prefix(sync_id, phase);
    match result {
        Ok(()) => append_mascot_log(&format!(
            "{}\nrequest:\n{request}",
            format_mascot_log_message(&format!(
                "{prefix} port {} に {action}request を送信しました。",
                address.port()
            ))
        )),
        Err(error) => {
            let message = format!(
                "{}\nrequest:\n{request}",
                format_mascot_log_message(&format!(
                    "{prefix} port {} への {action}request 送信に失敗しました: {error}",
                    address.port()
                ))
            );
            set_blocking_overlay_message(&message);
            append_mascot_log(&message)
        }
    }
}

pub(super) fn log_mascot_sync_snapshots(
    sync_id: u64,
    phase: &str,
    timing: &str,
    address: SocketAddr,
) -> anyhow::Result<()> {
    let mut result = Ok(());
    for endpoint in SNAPSHOT_ENDPOINTS {
        if let Err(error) = log_mascot_sync_snapshot(sync_id, phase, timing, address, endpoint) {
            result = Err(error);
        }
    }
    result
}

fn log_mascot_sync_snapshot(
    sync_id: u64,
    phase: &str,
    timing: &str,
    address: SocketAddr,
    endpoint: &str,
) -> anyhow::Result<()> {
    let request = format_mascot_request("GET", endpoint, address, None);
    let snapshot = fetch_mascot_http_snapshot(address, endpoint);
    append_mascot_log(&format_mascot_sync_snapshot_message(
        sync_id, phase, timing, endpoint, address, &request, &snapshot,
    ))
}

fn format_mascot_sync_snapshot_message(
    sync_id: u64,
    phase: &str,
    timing: &str,
    endpoint: &str,
    address: SocketAddr,
    request: &str,
    snapshot: &anyhow::Result<MascotHttpSnapshot>,
) -> String {
    let prefix = sync_log_prefix(sync_id, phase);
    match snapshot {
        Ok(snapshot) => format!(
            "{}\nrequest:\n{request}\nresponse:\n{}",
            format_mascot_log_message(&format!(
                "{prefix} {timing} {endpoint} snapshot を port {} から取得しました。",
                address.port()
            )),
            indented_lines(&format_mascot_snapshot_response(snapshot))
        ),
        Err(error) => format!(
            "{}\nrequest:\n{request}\nerror:\n{}",
            format_mascot_log_message(&format!(
                "{prefix} {timing} {endpoint} snapshot を port {} から取得できませんでした: {error}",
                address.port()
            )),
            indented_lines(&error.to_string())
        ),
    }
}

fn format_mascot_snapshot_response(snapshot: &MascotHttpSnapshot) -> String {
    let mut lines = vec![format!(
        "status: {}",
        snapshot.status_line.as_deref().unwrap_or("(unknown)")
    )];
    if snapshot.truncated {
        lines.push(format!(
            "truncated: true (limit_bytes={MAX_SNAPSHOT_RESPONSE_BYTES})"
        ));
    }
    if snapshot.timed_out_after_partial {
        lines.push("timed_out_after_partial: true".to_string());
    }
    if !snapshot.body.trim().is_empty() {
        lines.push("body:".to_string());
        lines.push(indented_lines(snapshot.body.trim_end()));
    }
    lines.join("\n")
}

fn fetch_mascot_http_snapshot(
    address: SocketAddr,
    endpoint: &str,
) -> anyhow::Result<MascotHttpSnapshot> {
    let mut stream = TcpStream::connect_timeout(&address, SNAPSHOT_TIMEOUT)?;
    stream.set_read_timeout(Some(SNAPSHOT_TIMEOUT))?;
    stream.set_write_timeout(Some(SNAPSHOT_TIMEOUT))?;
    stream.write_all(format_mascot_get_wire_request(endpoint, address).as_bytes())?;
    stream.flush()?;

    let mut response = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    let mut timed_out_after_partial = false;
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read_len) => {
                let remaining = MAX_SNAPSHOT_RESPONSE_BYTES.saturating_sub(response.len());
                if read_len > remaining {
                    response.extend_from_slice(&buffer[..remaining]);
                    truncated = true;
                    break;
                }
                response.extend_from_slice(&buffer[..read_len]);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if response.is_empty() {
                    return Err(error.into());
                }
                timed_out_after_partial = true;
                break;
            }
            Err(error) => return Err(error.into()),
        }
    }

    let response = String::from_utf8_lossy(&response).into_owned();
    let (status_line, body) = split_http_response(&response);
    Ok(MascotHttpSnapshot {
        status_line: status_line.map(ToOwned::to_owned),
        body: pretty_json_body(body),
        truncated,
        timed_out_after_partial,
    })
}

fn format_mascot_get_wire_request(endpoint: &str, address: SocketAddr) -> String {
    format!("GET {endpoint} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n")
}

fn split_http_response(response: &str) -> (Option<&str>, &str) {
    let (head, body) = response
        .split_once("\r\n\r\n")
        .or_else(|| response.split_once("\n\n"))
        .unwrap_or((response, ""));
    (head.lines().next(), body)
}

fn pretty_json_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    serde_json::from_str::<serde_json::Value>(trimmed)
        .and_then(|value| serde_json::to_string_pretty(&value))
        .unwrap_or_else(|_| trimmed.to_string())
}
