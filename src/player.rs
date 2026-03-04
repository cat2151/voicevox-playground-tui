use std::io::Cursor;

use rodio::{Decoder, OutputStream, Sink};
use tokio::sync::mpsc;

/// 別スレッドでrodioを起動し、WAV bytesを受け取り次第再生する。
/// 再生中に次のWAVが来た場合は前の再生を中断して新しいものを再生する。
pub fn spawn_player(mut rx: mpsc::Receiver<Vec<u8>>) {
    std::thread::spawn(move || {
        let (_stream, stream_handle) = OutputStream::try_default()
            .expect("audio output stream の取得に失敗した");
        let sink = Sink::try_new(&stream_handle)
            .expect("Sink の生成に失敗した");

        while let Some(wav) = rx.blocking_recv() {
            sink.stop();  // 再生中のものを即中断
            match Decoder::new(Cursor::new(wav)) {
                Ok(source) => {
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
