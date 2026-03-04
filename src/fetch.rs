//! VOICEVOXへの非同期fetchワーカー。
//! キャッシュキーは行インデックスではなく行文字列。
//! 同じ文字列なら同じwavが返るため、行の移動・編集後の巻き戻しでも正しく動く。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::voicevox;

/// キャッシュ型エイリアス: 行文字列 → WAV bytes
pub type WavCache = Arc<Mutex<HashMap<String, Vec<u8>>>>;

#[derive(Debug)]
pub struct FetchRequest {
    pub text:       String,
    pub play_after: bool,
}

pub fn spawn_worker(
    rx:      mpsc::Receiver<FetchRequest>,
    cache:   WavCache,
    play_tx: mpsc::Sender<Vec<u8>>,
) {
    tokio::spawn(worker_loop(rx, cache, play_tx));
}

async fn worker_loop(
    mut rx:  mpsc::Receiver<FetchRequest>,
    cache:   WavCache,
    play_tx: mpsc::Sender<Vec<u8>>,
) {
    while let Some(req) = rx.recv().await {
        if req.text.trim().is_empty() { continue; }

        let cached: Option<Vec<u8>> = {
            cache.lock().unwrap().get(&req.text).cloned()
        };
        if let Some(wav) = cached {
            if req.play_after { let _ = play_tx.send(wav).await; }
            continue;
        }

        match voicevox::synthesize_line(&req.text).await {
            Ok(wav) => {
                { cache.lock().unwrap().insert(req.text.clone(), wav.clone()); }
                if req.play_after { let _ = play_tx.send(wav).await; }
            }
            Err(e) => eprintln!("[fetch error] {e}"),
        }
    }
}
