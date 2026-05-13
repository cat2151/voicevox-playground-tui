use std::io::{BufRead, BufReader, Write};
use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{bail, Context};
use mascot_render_client::{preview_mouth_flap_timeline_request, PREVIEW_MOUTH_FLAP_FPS};
use mascot_render_protocol::{
    validate_motion_timeline_request, MotionTimelineKind, MotionTimelineRequest, MotionTimelineStep,
};

use super::{MASCOT_APPLY_TIMEOUT, MASCOT_CONNECT_TIMEOUT, MASCOT_IO_TIMEOUT};

#[derive(serde::Serialize)]
pub(super) struct MotionTimelineRequestBody<'a> {
    steps: &'a [MotionTimelineStep],
    #[serde(skip_serializing_if = "Option::is_none")]
    target_character_name: Option<&'a str>,
}

pub(super) fn motion_timeline_request_body<'a>(
    request: &'a MotionTimelineRequest,
    target_character_name: Option<&'a str>,
) -> MotionTimelineRequestBody<'a> {
    MotionTimelineRequestBody {
        steps: &request.steps,
        target_character_name,
    }
}

pub(super) fn play_timeline_mascot_render_server_with_target(
    address: SocketAddr,
    request: &MotionTimelineRequest,
    target_character_name: Option<&str>,
) -> anyhow::Result<()> {
    validate_motion_timeline_request(request)?;
    let body = serde_json::to_vec(&motion_timeline_request_body(
        request,
        target_character_name,
    ))
    .context("failed to serialize mascot motion timeline request")?;
    post_mascot_json_request(address, "/timeline", &body, MASCOT_APPLY_TIMEOUT)
}

pub(super) fn post_mascot_json_request(
    address: SocketAddr,
    path: &str,
    body: &[u8],
    read_timeout: Duration,
) -> anyhow::Result<()> {
    let mut stream = std::net::TcpStream::connect_timeout(&address, MASCOT_CONNECT_TIMEOUT)
        .with_context(|| format!("failed to connect to mascot-render-server at {address}"))?;
    stream
        .set_read_timeout(Some(read_timeout))
        .with_context(|| format!("failed to set read timeout for {address}"))?;
    stream
        .set_write_timeout(Some(MASCOT_IO_TIMEOUT))
        .with_context(|| format!("failed to set write timeout for {address}"))?;

    let host = match address {
        SocketAddr::V4(address) => format!("{}:{}", address.ip(), address.port()),
        SocketAddr::V6(address) => format!("[{}]:{}", address.ip(), address.port()),
    };
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .with_context(|| format!("failed to write HTTP request to {address}"))?;
    stream
        .write_all(body)
        .with_context(|| format!("failed to write HTTP body to {address}"))?;
    stream
        .flush()
        .with_context(|| format!("failed to flush HTTP request to {address}"))?;

    read_mascot_response(&mut stream, path)
}

fn read_mascot_response(stream: &mut std::net::TcpStream, path: &str) -> anyhow::Result<()> {
    let mut reader = BufReader::new(stream);
    read_mascot_response_from_reader(&mut reader, path)
}

fn read_mascot_response_from_reader(reader: &mut impl BufRead, path: &str) -> anyhow::Result<()> {
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
        let bytes_read = reader
            .read_line(&mut header_line)
            .context("failed to read HTTP response header")?;
        if bytes_read == 0 {
            bail!("unexpected EOF while reading HTTP headers");
        }
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

pub(super) fn motion_timeline_request(duration_ms: u64) -> MotionTimelineRequest {
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
mod tests {
    use super::read_mascot_response_from_reader;
    use std::io::{BufReader, Cursor};

    #[test]
    fn read_mascot_response_returns_error_on_unexpected_eof_while_reading_headers() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n";
        let mut reader = BufReader::new(Cursor::new(response));

        let result = read_mascot_response_from_reader(&mut reader, "/timeline");

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "unexpected EOF while reading HTTP headers"
        );
    }
}
