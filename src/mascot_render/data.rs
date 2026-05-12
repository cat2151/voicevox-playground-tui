use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use mascot_render_client::mascot_render_server_psd_file_names;

use crate::tag;

use super::{DATA_ROOT_ENV, MIN_DURATION_MS};

#[derive(Debug, Default)]
struct MascotPsdAvailability {
    normalized_file_names: Vec<String>,
}

pub(super) fn mascot_char_name_for_line(line: &str) -> Option<String> {
    let mut segments = tag::parse_line(line).into_iter();
    let (_, first_ctx) = segments.next()?;
    let first = first_ctx.char_name;

    if segments.all(|(_, ctx)| ctx.char_name == first) {
        Some(first)
    } else {
        None
    }
}

fn mascot_psd_availability() -> &'static Mutex<MascotPsdAvailability> {
    static AVAILABILITY: OnceLock<Mutex<MascotPsdAvailability>> = OnceLock::new();
    AVAILABILITY.get_or_init(|| Mutex::new(MascotPsdAvailability::default()))
}

pub(super) fn set_loaded_psd_file_names(file_names: Vec<String>) {
    let normalized_file_names = file_names
        .into_iter()
        .map(|file_name| normalize_mascot_lookup_text(&file_name))
        .filter(|file_name| !file_name.is_empty())
        .collect();
    *mascot_psd_availability()
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = MascotPsdAvailability {
        normalized_file_names,
    };
}

pub(crate) fn refresh_available_psd_file_names_from_server() -> anyhow::Result<usize> {
    let file_names = mascot_render_server_psd_file_names()?;
    let count = file_names.len();
    set_loaded_psd_file_names(file_names);
    Ok(count)
}

pub(crate) fn speaker_has_psd(speaker: &str) -> bool {
    let normalized_speaker = normalize_mascot_lookup_text(speaker);
    if normalized_speaker.is_empty() {
        return false;
    }

    mascot_psd_availability()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .normalized_file_names
        .iter()
        .any(|file_name| file_name.contains(&normalized_speaker))
}

pub(super) fn vpt_ensemble_character_names(lines: &[String]) -> Vec<String> {
    if crate::speakers::try_get().is_none() {
        return Vec::new();
    }

    let mut names = Vec::new();
    for line in lines {
        for (_, ctx) in tag::parse_line(line) {
            if speaker_has_psd(&ctx.char_name) && !names.contains(&ctx.char_name) {
                names.push(ctx.char_name);
            }
        }
    }
    names
}

fn normalize_mascot_lookup_text(text: &str) -> String {
    trim_psd_extension(text.trim())
        .chars()
        .filter(|ch| {
            !matches!(
                ch,
                '/' | '\\' | '_' | '-' | ' ' | '　' | '.' | '(' | ')' | '[' | ']'
            )
        })
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn trim_psd_extension(text: &str) -> &str {
    match text.rsplit_once('.') {
        Some((stem, ext)) if ext.eq_ignore_ascii_case("psd") => stem,
        _ => text,
    }
}

pub(super) fn wav_duration_ms(wav: &[u8]) -> Option<u64> {
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

pub(super) fn default_mascot_data_root() -> Option<PathBuf> {
    dirs::data_local_dir().map(|base| base.join("mascot-render-server"))
}

#[cfg(test)]
pub(super) fn mascot_data_root() -> Option<PathBuf> {
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
