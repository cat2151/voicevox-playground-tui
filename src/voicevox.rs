//! VOICEVOX REST APIクライアント。

use anyhow::Result;
use serde_json::Value;

use crate::{speakers, tag};

/// audio_queryのJSONをそのまま返す（イントネーション編集用）。
pub async fn get_audio_query(text: &str, speaker_id: u32) -> Result<serde_json::Value> {
    let table = speakers::get();
    let base_url = table
        .speaker_base_url
        .get(&speaker_id)
        .map(|s| s.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!("speaker_id {speaker_id} に対応するエンジンが見つからない")
        })?;
    let client = reqwest::Client::new();
    let query: serde_json::Value = client
        .post(format!("{base_url}/audio_query"))
        .query(&[("text", text), ("speaker", &speaker_id.to_string())])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(query)
}

/// 指定されたaudio_queryのJSONを使って合成する（イントネーション編集用）。
pub async fn synthesize_with_query(query: &serde_json::Value, speaker_id: u32) -> Result<Vec<u8>> {
    let table = speakers::get();
    let base_url = table
        .speaker_base_url
        .get(&speaker_id)
        .map(|s| s.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!("speaker_id {speaker_id} に対応するエンジンが見つからない")
        })?;
    let client = reqwest::Client::new();
    let wav = client
        .post(format!("{base_url}/synthesis"))
        .query(&[("speaker", speaker_id.to_string())])
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(query)?)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    Ok(wav.to_vec())
}

/// audio_queryのJSONからモーラテキストとpitch値を抽出する。
pub fn extract_mora_data(query: &serde_json::Value) -> (Vec<String>, Vec<f64>) {
    let mut mora_texts = Vec::new();
    let mut pitches = Vec::new();
    if let Some(accent_phrases) = query["accent_phrases"].as_array() {
        for phrase in accent_phrases {
            if let Some(moras) = phrase["moras"].as_array() {
                for mora in moras {
                    mora_texts.push(mora["text"].as_str().unwrap_or("").to_string());
                    pitches.push(mora["pitch"].as_f64().unwrap_or(0.0));
                }
            }
        }
    }
    (mora_texts, pitches)
}

/// audio_queryのJSONのpitch値をpitchesスライスで上書きする。
pub fn set_mora_pitches(query: &mut serde_json::Value, pitches: &[f64]) {
    let mut idx = 0usize;
    if let Some(accent_phrases) = query["accent_phrases"].as_array_mut() {
        for phrase in accent_phrases.iter_mut() {
            if let Some(moras) = phrase["moras"].as_array_mut() {
                for mora in moras.iter_mut() {
                    if idx < pitches.len() {
                        mora["pitch"] = serde_json::json!(pitches[idx]);
                        idx += 1;
                    }
                }
            }
        }
    }
}

pub async fn synthesize(text: &str, speaker_id: u32) -> Result<Vec<u8>> {
    let table = speakers::get();
    let base_url = table
        .speaker_base_url
        .get(&speaker_id)
        .map(|s| s.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!("speaker_id {speaker_id} に対応するエンジンが見つからない")
        })?;
    let client = reqwest::Client::new();

    let query: Value = client
        .post(format!("{base_url}/audio_query"))
        .query(&[("text", text), ("speaker", &speaker_id.to_string())])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let wav = client
        .post(format!("{base_url}/synthesis"))
        .query(&[("speaker", speaker_id.to_string())])
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&query)?)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    Ok(wav.to_vec())
}

/// タグ付き行を解析してセグメントごとに合成し、WAVを連結して返す。
pub async fn synthesize_line(line: &str) -> Result<Vec<u8>> {
    let segments = tag::parse_line(line);
    if segments.is_empty() {
        return Ok(vec![]);
    }

    let mut wavs = Vec::new();
    for (text, ctx) in &segments {
        wavs.push(synthesize(text, ctx.speaker_id).await?);
    }
    Ok(concat_wavs(wavs))
}

fn concat_wavs(wavs: Vec<Vec<u8>>) -> Vec<u8> {
    if wavs.is_empty() {
        return vec![];
    }
    if wavs.len() == 1 {
        return wavs.into_iter().next().unwrap();
    }
    const HDR: usize = 44;
    let pcm: Vec<u8> = wavs
        .iter()
        .filter(|w| w.len() > HDR)
        .flat_map(|w| w[HDR..].iter().copied())
        .collect();
    let mut out = wavs[0][..HDR].to_vec();
    let total = pcm.len() as u32;
    out[4..8].copy_from_slice(&(36 + total).to_le_bytes());
    out[40..44].copy_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&pcm);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_query_with_pitches(texts: &[&str], pitches: &[f64]) -> serde_json::Value {
        let moras: Vec<serde_json::Value> = texts
            .iter()
            .zip(pitches.iter())
            .map(|(&t, &p)| serde_json::json!({ "text": t, "pitch": p }))
            .collect();
        serde_json::json!({ "accent_phrases": [{ "moras": moras }] })
    }

    #[test]
    fn extract_mora_data_returns_texts_and_pitches() {
        let query = make_query_with_pitches(&["ず", "ん", "だ"], &[5.87, 6.0, 0.0]);
        let (texts, pitches) = extract_mora_data(&query);
        assert_eq!(texts, vec!["ず", "ん", "だ"]);
        assert_eq!(pitches, vec![5.87, 6.0, 0.0]);
    }

    #[test]
    fn extract_mora_data_empty_query_returns_empty() {
        let query = serde_json::json!({ "accent_phrases": [] });
        let (texts, pitches) = extract_mora_data(&query);
        assert!(texts.is_empty());
        assert!(pitches.is_empty());
    }

    #[test]
    fn set_mora_pitches_updates_values_in_query() {
        let mut query = make_query_with_pitches(&["ず", "ん", "だ"], &[5.87, 6.0, 0.0]);
        set_mora_pitches(&mut query, &[1.1, 2.2, 3.3]);
        let (_, pitches) = extract_mora_data(&query);
        assert!((pitches[0] - 1.1).abs() < 1e-9);
        assert!((pitches[1] - 2.2).abs() < 1e-9);
        assert!((pitches[2] - 3.3).abs() < 1e-9);
    }

    #[test]
    fn set_mora_pitches_partial_update_leaves_rest_unchanged() {
        let mut query = make_query_with_pitches(&["ず", "ん", "だ"], &[1.0, 2.0, 3.0]);
        // Only 2 new pitches: only the first 2 should change
        set_mora_pitches(&mut query, &[9.0, 8.0]);
        let (_, pitches) = extract_mora_data(&query);
        assert!((pitches[0] - 9.0).abs() < 1e-9);
        assert!((pitches[1] - 8.0).abs() < 1e-9);
        assert!((pitches[2] - 3.0).abs() < 1e-9);
    }
}
