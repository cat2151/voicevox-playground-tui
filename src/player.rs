use std::io::Cursor;

use rodio::{Decoder, OutputStream, Sink};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct PlayRequest {
    pub wav: Vec<u8>,
    pub source_text: String,
}

/// 別スレッドでrodioを起動し、WAV bytesを受け取り次第再生する。
/// 再生中に次のWAVが来た場合は前の再生を中断して新しいものを再生する。
pub fn spawn_player(mut rx: mpsc::Receiver<PlayRequest>) {
    std::thread::spawn(move || {
        let (_stream, stream_handle) =
            OutputStream::try_default().expect("audio output stream の取得に失敗した");
        let sink = Sink::try_new(&stream_handle).expect("Sink の生成に失敗した");

        while let Some(request) = rx.blocking_recv() {
            sink.stop(); // 再生中のものを即中断
            match Decoder::new(Cursor::new(request.wav.clone())) {
                Ok(source) => {
                    crate::mascot_render::sync_playback(&request.source_text, &request.wav);
                    sink.append(source);
                    sink.play();
                }
                Err(e) => {
                    eprintln!("[player error] {e}");
                }
            }
        }
    });
}
