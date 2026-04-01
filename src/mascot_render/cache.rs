use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use super::{mascot_data_root, PSD_CACHE_TTL};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MascotPsdEntry {
    pub(super) psd_label: String,
    pub(super) png_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MascotPsdList {
    pub(super) entries: Vec<MascotPsdEntry>,
    pub(super) load_reason: Option<String>,
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

fn is_cache_fresh(cache: &MascotPsdCache, cache_dir: &Option<PathBuf>) -> bool {
    cache.cache_dir == *cache_dir
        && cache
            .loaded_at
            .is_some_and(|loaded_at| loaded_at.elapsed() < PSD_CACHE_TTL)
}

pub(super) fn mascot_psd_list() -> MascotPsdList {
    let cache_dir = mascot_data_root().map(|path| path.join("cache"));
    let cached_list = {
        let cache = mascot_psd_cache_slot().lock().unwrap();
        if is_cache_fresh(&cache, &cache_dir) {
            Some(cache.list.clone())
        } else {
            None
        }
    };
    if let Some(list) = cached_list {
        return list;
    }

    let list = load_mascot_psd_list(cache_dir.as_deref());
    let cached_list = {
        let cache = mascot_psd_cache_slot().lock().unwrap();
        if is_cache_fresh(&cache, &cache_dir) {
            Some(cache.list.clone())
        } else {
            None
        }
    };
    if let Some(list) = cached_list {
        return list;
    }

    let mut cache = mascot_psd_cache_slot().lock().unwrap();
    cache.cache_dir = cache_dir;
    cache.loaded_at = Some(Instant::now());
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

pub(super) fn mascot_psd_list_from_cache_dir(cache_dir: &Path) -> MascotPsdList {
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

pub(super) fn matching_skin_path(speaker: &str, psd_entries: &[MascotPsdEntry]) -> Option<PathBuf> {
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

pub(super) fn no_matching_skin_message(speaker: &str, psd_entries: &[MascotPsdEntry]) -> String {
    let psd_list = psd_entries
        .iter()
        .map(|entry| entry.psd_label.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!("hitしませんでした。speaker:{speaker} psdのlist:{psd_list}")
}

pub(super) fn no_matching_skin_message_for_list(speaker: &str, psd_list: &MascotPsdList) -> String {
    if psd_list.entries.is_empty() {
        let reason = psd_list
            .load_reason
            .as_deref()
            .unwrap_or("psd list is empty");
        return format!("hitしませんでした。speaker:{speaker} psdのlist:({reason})");
    }
    no_matching_skin_message(speaker, &psd_list.entries)
}
