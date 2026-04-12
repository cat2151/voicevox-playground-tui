use std::fs;
use std::io::{BufRead, Write};
use std::path::PathBuf;
#[cfg(test)]
use std::sync::{Mutex, OnceLock};

use anyhow::Result;

use crate::app::{AllTabIntonations, AllTabLines, IntonationLineData, LineIntonations};

/// タブごとのセッション状態（カーソル行番号・折りたたみ状態）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TabSessionState {
    pub cursor: usize,
    pub folded: bool,
}

/// 起動・終了・自動保存で保存・復元するセッション状態。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SessionState {
    /// アクティブなタブのインデックス（0始まり）。
    pub active_tab: usize,
    /// 各タブのカーソル行番号・折りたたみ状態。インデックスはタブインデックスに対応する。
    pub tabs: Vec<TabSessionState>,
}

#[cfg(test)]
fn local_data_dir_override_slot() -> &'static Mutex<Option<PathBuf>> {
    static SLOT: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

#[cfg(test)]
pub(crate) fn with_local_data_dir_override<T>(value: Option<PathBuf>, f: impl FnOnce() -> T) -> T {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct OverrideGuard {
        original: Option<PathBuf>,
    }

    impl Drop for OverrideGuard {
        fn drop(&mut self) {
            *local_data_dir_override_slot()
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = self.original.take();
        }
    }

    let _guard = LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let original = local_data_dir_override_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    *local_data_dir_override_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = value;
    let _override_guard = OverrideGuard { original };

    f()
}

pub fn history_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(base) = local_data_dir_override_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
    {
        return base.join("voicevox-playground-tui");
    }

    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("voicevox-playground-tui")
}

/// タブインデックスに対応するhistoryファイルのパスを返す。
/// tab 0 → history.txt（既存ファイルとの後方互換性を維持）、
/// tab 1 → history2.txt、tab 2 → history3.txt …
/// （issue #75 の指示: "tab1はhistory.txtのままで、tab2はhistory2.txt ..."）
fn history_path_for_tab(tab_index: usize) -> PathBuf {
    let dir = history_dir();
    if tab_index == 0 {
        dir.join("history.txt")
    } else {
        dir.join(format!("history{}.txt", tab_index + 1))
    }
}

/// 1行の末尾から `\t{"pitches":[...]}` サフィックスを分離する。
/// 成功した場合は (行テキスト部分, pitches) を、失敗した場合は (元の行, None) を返す。
fn split_pitches_suffix(raw_line: &str) -> (&str, Option<Vec<f64>>) {
    if let Some(tab_pos) = raw_line.rfind('\t') {
        let suffix = &raw_line[tab_pos + 1..];
        if suffix.starts_with("{\"pitches\":") {
            if let Ok(obj) = serde_json::from_str::<serde_json::Value>(suffix) {
                if let Some(arr) = obj.get("pitches").and_then(|v| v.as_array()) {
                    let pitches: Option<Vec<f64>> = arr.iter().map(|v| v.as_f64()).collect();
                    if let Some(pitches) = pitches {
                        return (&raw_line[..tab_pos], Some(pitches));
                    }
                }
            }
        }
    }
    (raw_line, None)
}

/// テキスト行にpitchesサフィックスを付けた文字列を返す。
fn format_with_pitches(text: &str, pitches: &[f64]) -> String {
    let json = serde_json::json!({ "pitches": pitches });
    format!("{}\t{}", text, json)
}

/// 指定タブの履歴ファイルをロードし、行テキストとイントネーションデータを返す。
/// 各行末尾の `\t{"pitches":[...]}` サフィックスからイントネーションを復元する。
/// ファイルがなければ None を返す。
fn load_tab(tab_index: usize) -> Result<Option<(Vec<String>, LineIntonations)>> {
    let path = history_path_for_tab(tab_index);
    if !path.exists() {
        return Ok(None);
    }
    let file = fs::File::open(&path)?;
    let raw_lines: Vec<String> = std::io::BufReader::new(file)
        .lines()
        .map(|l| l.unwrap_or_default())
        .collect();
    if raw_lines.is_empty() {
        return Ok(Some((vec![String::new()], vec![None])));
    }
    let mut lines = Vec::with_capacity(raw_lines.len());
    let mut intonations = Vec::with_capacity(raw_lines.len());
    for raw in &raw_lines {
        let (text, pitches_opt) = split_pitches_suffix(raw);
        lines.push(text.to_owned());
        intonations.push(pitches_opt.map(|pitches| IntonationLineData {
            // query = Null は「history.txtから復元したpitches-only状態」を表すセンチネル値。
            // 再生・イントネーション編集時にAPIからaudio_queryを遅延取得して完全なデータに昇格させる。
            query: serde_json::Value::Null,
            mora_texts: Vec::new(),
            pitches,
            speaker_id: 0,
        }));
    }
    Ok(Some((lines, intonations)))
}

/// 起動時: 全タブの履歴ファイルをロードし、行データとイントネーションデータを返す。
/// タブのイントネーションデータは各行末尾の `\t{"pitches":[...]}` サフィックスから復元する。
/// tab1はhistory.txt、tab2はhistory2.txt … と連番で存在する分だけロードする。
/// ファイルが1つもなければ空行1つのタブを1つ返す。
pub fn load_all() -> Result<(AllTabLines, AllTabIntonations)> {
    let mut all_lines = Vec::new();
    let mut all_intonations = Vec::new();
    let mut tab_index = 0;
    loop {
        match load_tab(tab_index)? {
            Some((lines, intonations)) => {
                all_lines.push(lines);
                all_intonations.push(intonations);
                tab_index += 1;
            }
            None => {
                if tab_index == 0 {
                    // history.txtが存在しない新規起動は空行1つのタブを返す
                    all_lines.push(vec![String::new()]);
                    all_intonations.push(vec![None]);
                }
                break;
            }
        }
    }
    Ok((all_lines, all_intonations))
}

/// 終了時: 全タブの内容をそれぞれのhistoryファイルに上書き保存する。
/// イントネーションデータがある行は末尾に `\t{"pitches":[...]}` を付加する。
/// 現在のタブ数を超えて残っている余分なhistoryファイルは削除する。
pub fn save_all(
    all_tab_lines: &[Vec<String>],
    all_tab_intonations: &[Vec<Option<IntonationLineData>>],
) -> Result<()> {
    let dir = history_dir();
    fs::create_dir_all(&dir)?;

    for (i, lines) in all_tab_lines.iter().enumerate() {
        let path = history_path_for_tab(i);
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        let intonations = all_tab_intonations.get(i);
        for (j, line) in lines.iter().enumerate() {
            let pitches_opt = intonations
                .and_then(|v| v.get(j))
                .and_then(|opt| opt.as_ref())
                .filter(|d| !d.pitches.is_empty())
                .map(|d| d.pitches.as_slice());
            if let Some(pitches) = pitches_opt {
                writeln!(file, "{}", format_with_pitches(line, pitches))?;
            } else {
                writeln!(file, "{}", line)?;
            }
        }
    }

    // 余分なhistoryファイル（タブが閉じられた場合など）をディレクトリ全体をスキャンして削除する。
    // 連番でスキャンする方法ではgapがある場合に取りこぼすため、ディレクトリエントリを列挙する。
    // 削除失敗はベストエフォートとして無視する（データ保存は既に完了しているため）。
    let current_tabs = all_tab_lines.len();
    if let Ok(read_dir) = fs::read_dir(&dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_owned(),
                None => continue,
            };

            // tab 0 の history.txt は常に保持する
            if file_name == "history.txt" {
                continue;
            }

            // "history{N}.txt" 形式のファイル名をパースする
            if let Some(rest) = file_name.strip_prefix("history") {
                if let Some(num_str) = rest.strip_suffix(".txt") {
                    if let Ok(n) = num_str.parse::<usize>() {
                        // tab 1 → history2.txt なので N - 1 がタブインデックスになる
                        if n >= 2 {
                            let tab_index = n - 1;
                            if tab_index >= current_tabs {
                                let _ = fs::remove_file(&path);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// セッション状態ファイル（history.json）のパスを返す。
fn session_state_path() -> PathBuf {
    history_dir().join("history.json")
}

/// セッション状態（アクティブタブ・各タブのカーソル位置・折りたたみ状態）を history.json に保存する。
pub fn save_session_state(state: &SessionState) -> Result<()> {
    let dir = history_dir();
    fs::create_dir_all(&dir)?;
    let path = session_state_path();
    let json = serde_json::to_string_pretty(state)?;
    fs::write(&path, json)?;
    Ok(())
}

/// history.json からセッション状態を読み込む。
/// ファイルが存在しない場合や読み込みに失敗した場合はデフォルト値を返す。
pub fn load_session_state() -> SessionState {
    let path = session_state_path();
    if !path.exists() {
        return SessionState::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "tests/history.rs"]
mod tests;
