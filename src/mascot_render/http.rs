use std::io::{BufRead, BufReader, Read, Write};
use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{bail, Context};

pub(crate) fn disable_favorite_ensemble_mascot_render_server_at(
    address: SocketAddr,
) -> anyhow::Result<()> {
    post_empty_mascot_request(address, "/favorite-ensemble/disable")
}

fn post_empty_mascot_request(address: SocketAddr, path: &str) -> anyhow::Result<()> {
    let mut stream = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(2))
        .with_context(|| format!("failed to connect to mascot-render-server at {address}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .with_context(|| format!("failed to set read timeout for {address}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
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
