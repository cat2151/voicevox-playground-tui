use std::fs;
use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::Result;

fn history_dir() -> PathBuf {
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

/// 指定タブの履歴ファイルをロードする。ファイルがなければ None を返す。
fn load_tab(tab_index: usize) -> Result<Option<Vec<String>>> {
    let path = history_path_for_tab(tab_index);
    if !path.exists() {
        return Ok(None);
    }
    let file = fs::File::open(&path)?;
    let lines: Vec<String> = std::io::BufReader::new(file)
        .lines()
        .map(|l| l.unwrap_or_default())
        .collect();
    Ok(Some(if lines.is_empty() { vec![String::new()] } else { lines }))
}

/// 起動時: 全タブの履歴ファイルをロードする。
/// tab1はhistory.txt、tab2はhistory2.txt … と連番で存在する分だけロードする。
/// ファイルが1つもなければ空行1つのタブを1つ返す。
pub fn load_all() -> Result<Vec<Vec<String>>> {
    let mut result = Vec::new();
    let mut tab_index = 0;
    loop {
        match load_tab(tab_index)? {
            Some(lines) => {
                result.push(lines);
                tab_index += 1;
            }
            None => {
                if tab_index == 0 {
                    // history.txtが存在しない新規起動は空行1つのタブを返す
                    result.push(vec![String::new()]);
                }
                break;
            }
        }
    }
    Ok(result)
}

/// 終了時: 全タブの内容をそれぞれのhistoryファイルに上書き保存する。
/// 現在のタブ数を超えて残っている余分なhistoryファイルは削除する。
pub fn save_all(all_tab_lines: &[Vec<String>]) -> Result<()> {
    let dir = history_dir();
    fs::create_dir_all(&dir)?;

    for (i, lines) in all_tab_lines.iter().enumerate() {
        let path = history_path_for_tab(i);
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        for line in lines {
            writeln!(file, "{}", line)?;
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
