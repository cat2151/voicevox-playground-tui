use std::fs;
use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::Result;

fn history_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("voicevox-playground-tui")
        .join("history.txt")
}

/// 起動時: 履歴ファイルを全行ロードする。ファイルがなければ空Vecを返す。
pub fn load() -> Result<Vec<String>> {
    let path = history_path();
    if !path.exists() {
        return Ok(vec![String::new()]);  // 新規起動は空行1つ
    }
    let file  = fs::File::open(&path)?;
    let lines: Vec<String> = std::io::BufReader::new(file)
        .lines()
        .map(|l| l.unwrap_or_default())
        .collect();
    Ok(if lines.is_empty() { vec![String::new()] } else { lines })
}

/// 終了時: 今セッションで追記された行だけを末尾に書き足す。
/// 既存ファイルのものと重複しないよう、ファイルの既存行数を記録しておく方法は
/// v1では行わず、「終了時にセッション全行を上書き保存」とシンプルに実装する。
pub fn append_new(lines: &[String]) -> Result<()> {
    let path = history_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)?;

    for line in lines {
        writeln!(file, "{}", line)?;
    }
    Ok(())
}
