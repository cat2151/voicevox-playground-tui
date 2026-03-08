//! `--clipboard` CLIモード: クリップボードの内容を行ごとに読み上げて終了する。
//! history.txtには追加しない。

use anyhow::Result;
use rodio::{Decoder, OutputStream, Sink};
use std::io::Cursor;

use crate::voicevox;

/// クリップボードの内容を行ごとにVOICEVOXで合成し、順番に再生して終了する。
pub async fn run() -> Result<()> {
    let text = read_clipboard()?;

    let (_stream, stream_handle) = OutputStream::try_default()?;
    let sink = Sink::try_new(&stream_handle)?;

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match voicevox::synthesize_line(line).await {
            Ok(wav) if !wav.is_empty() => {
                match Decoder::new(Cursor::new(wav)) {
                    Ok(source) => sink.append(source),
                    Err(e) => eprintln!("[clipboard] decode error: {e}"),
                }
            }
            Ok(_) => {}
            Err(e) => eprintln!("[clipboard] synthesis failed: {e}"),
        }
    }

    sink.sleep_until_end();
    Ok(())
}

fn read_clipboard() -> Result<String> {
    let mut cb = arboard::Clipboard::new()?;
    let text = cb.get_text()?;
    Ok(text)
}
