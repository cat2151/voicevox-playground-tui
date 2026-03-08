//! VOICEVOX REST APIクライアント。

use anyhow::Result;
use serde_json::Value;

use crate::{speakers, tag};

pub async fn synthesize(text: &str, speaker_id: u32) -> Result<Vec<u8>> {
    let table    = speakers::get();
    let base_url = table.speaker_base_url
        .get(&speaker_id)
        .map(|s| s.as_str())
        .ok_or_else(|| anyhow::anyhow!("speaker_id {speaker_id} に対応するエンジンが見つからない"))?;
    let client   = reqwest::Client::new();

    let query: Value = client
        .post(format!("{base_url}/audio_query"))
        .query(&[("text", text), ("speaker", &speaker_id.to_string())])
        .send().await?
        .error_for_status()?
        .json().await?;

    let wav = client
        .post(format!("{base_url}/synthesis"))
        .query(&[("speaker", speaker_id.to_string())])
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&query)?)
        .send().await?
        .error_for_status()?
        .bytes().await?;

    Ok(wav.to_vec())
}

/// タグ付き行を解析してセグメントごとに合成し、WAVを連結して返す。
pub async fn synthesize_line(line: &str) -> Result<Vec<u8>> {
    let segments = tag::parse_line(line);
    if segments.is_empty() { return Ok(vec![]); }

    let mut wavs = Vec::new();
    for (text, ctx) in &segments {
        wavs.push(synthesize(text, ctx.speaker_id).await?);
    }
    Ok(concat_wavs(wavs))
}

fn concat_wavs(wavs: Vec<Vec<u8>>) -> Vec<u8> {
    if wavs.is_empty() { return vec![]; }
    if wavs.len() == 1 { return wavs.into_iter().next().unwrap(); }
    const HDR: usize = 44;
    let pcm: Vec<u8> = wavs.iter()
        .filter(|w| w.len() > HDR)
        .flat_map(|w| w[HDR..].iter().copied())
        .collect();
    let mut out = wavs[0][..HDR].to_vec();
    let total   = pcm.len() as u32;
    out[4..8].copy_from_slice(&(36 + total).to_le_bytes());
    out[40..44].copy_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&pcm);
    out
}
