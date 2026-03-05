//! 起動時に GET /speakers で取得したspeaker情報を保持するグローバルテーブル。
//! [N] タグ記法は「style_id = N のスタイルを直接指定する」という意味。

use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use serde::Deserialize;

// ── APIレスポンス型 ────────────────────────────────────────────────────────────
#[derive(Debug, Deserialize, Clone)]
pub struct SpeakerStyle {
    pub name: String,
    pub id:   u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Speaker {
    pub name:   String,
    pub styles: Vec<SpeakerStyle>,
}

// ── グローバルテーブル ─────────────────────────────────────────────────────────
pub struct SpeakerTable {
    /// (キャラ名, スタイル名) → style_id
    pub by_name:     HashMap<(String, String), u32>,
    /// style_id → (キャラ名, スタイル名)
    pub by_style_id: HashMap<u32, (String, String)>,
    /// キャラ名 → [(style_name, style_id)] （APIレスポンスの順序を保持）
    pub char_styles: HashMap<String, Vec<(String, u32)>>,
    /// キャラ名の出現順リスト（重複なし）
    pub char_names:  Vec<String>,
    /// 全スタイル名（重複なし・ソート済み）
    pub style_names: Vec<String>,
    /// デフォルトのstyle_id
    pub default_id:    u32,
    pub default_char:  String,
    pub default_style: String,
    /// VOICEVOX APIのベースURL（voicevox.rsが参照する）
    pub base_url:    String,
}

static TABLE: OnceLock<SpeakerTable> = OnceLock::new();

/// 起動時に1回だけ呼ぶ。
pub async fn load(base_url: &str) -> Result<()> {
    let url = format!("{base_url}/speakers");
    let speakers: Vec<Speaker> = reqwest::get(&url)
        .await
        .context("GET /speakers に接続できなかった。VOICEVOXが起動しているか確認してくれ")?
        .error_for_status()?
        .json()
        .await?;

    let mut by_name     = HashMap::new();
    let mut by_style_id = HashMap::new();
    let mut char_names  = Vec::new();
    let mut char_styles: HashMap<String, Vec<(String, u32)>> = HashMap::new();
    let mut style_name_set = std::collections::BTreeSet::new();

    for speaker in &speakers {
        if !char_names.contains(&speaker.name) {
            char_names.push(speaker.name.clone());
        }
        for style in &speaker.styles {
            by_name.insert((speaker.name.clone(), style.name.clone()), style.id);
            by_style_id.insert(style.id, (speaker.name.clone(), style.name.clone()));
            style_name_set.insert(style.name.clone());
            char_styles.entry(speaker.name.clone())
                .or_default()
                .push((style.name.clone(), style.id));
        }
    }

    let style_names: Vec<String> = style_name_set.into_iter().collect();

    // デフォルト: id=3が存在すればそれを使う（ずんだもんノーマル）
    // なければAPIレスポンス先頭のstyleにフォールバック
    let (default_id, default_char, default_style) =
        if let Some((char_name, style_name)) = by_style_id.get(&3) {
            (3u32, char_name.clone(), style_name.clone())
        } else {
            speakers.iter()
                .flat_map(|sp| sp.styles.iter().map(move |st| (st.id, sp.name.clone(), st.name.clone())))
                .next()
                .unwrap_or((0, String::new(), String::new()))
        };

    TABLE.set(SpeakerTable {
        by_name, by_style_id, char_styles, char_names, style_names,
        default_id, default_char, default_style,
        base_url: base_url.to_string(),
    }).ok();
    Ok(())
}

pub fn get() -> &'static SpeakerTable {
    TABLE.get().expect("speakers::load() が呼ばれていない")
}

impl SpeakerTable {
    /// (キャラ名, スタイル名) → style_id
    pub fn resolve_by_name(&self, char_name: &str, style_name: &str) -> Option<u32> {
        self.by_name.get(&(char_name.to_string(), style_name.to_string())).copied()
    }

    /// [N] 記法: style_id = N を直接指定 → (char_name, style_name, style_id)
    pub fn resolve_by_id(&self, id: u32) -> Option<(String, String, u32)> {
        self.by_style_id.get(&id)
            .map(|(c, s)| (c.clone(), s.clone(), id))
    }
}

#[cfg(test)]
pub(crate) fn init_test_table() {
    let mut by_name     = HashMap::new();
    let mut by_style_id = HashMap::new();
    let mut char_styles: HashMap<String, Vec<(String, u32)>> = HashMap::new();

    // ずんだもん: ノーマル(3), あまあま(1)
    by_name.insert(("ずんだもん".to_string(), "ノーマル".to_string()), 3u32);
    by_name.insert(("ずんだもん".to_string(), "あまあま".to_string()), 1u32);
    by_style_id.insert(3u32, ("ずんだもん".to_string(), "ノーマル".to_string()));
    by_style_id.insert(1u32, ("ずんだもん".to_string(), "あまあま".to_string()));
    char_styles.insert("ずんだもん".to_string(), vec![
        ("ノーマル".to_string(), 3u32),
        ("あまあま".to_string(), 1u32),
    ]);

    // 四国めたん: ノーマル(2)
    by_name.insert(("四国めたん".to_string(), "ノーマル".to_string()), 2u32);
    by_style_id.insert(2u32, ("四国めたん".to_string(), "ノーマル".to_string()));
    char_styles.insert("四国めたん".to_string(), vec![
        ("ノーマル".to_string(), 2u32),
    ]);

    TABLE.set(SpeakerTable {
        by_name,
        by_style_id,
        char_styles,
        char_names:    vec!["ずんだもん".to_string(), "四国めたん".to_string()],
        style_names:   vec!["あまあま".to_string(), "ノーマル".to_string()],
        default_id:    3,
        default_char:  "ずんだもん".to_string(),
        default_style: "ノーマル".to_string(),
        base_url:      String::new(),
    }).ok();
}
