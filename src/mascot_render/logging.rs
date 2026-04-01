use std::fs;
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;

use chrono::Local;
use serde::Serialize;

use super::overlay::set_blocking_overlay_message;
use super::{mascot_data_root, LOG_FILE_NAME};

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

pub(super) fn mascot_log_path() -> Option<PathBuf> {
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

pub(super) fn log_mascot_request_result(
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
