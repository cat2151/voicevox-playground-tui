//! VOICEVOXへの非同期fetchワーカー。
//! キャッシュキーは行インデックスではなく行文字列。
//! 同じ文字列なら同じwavが返るため、行の移動・編集後の巻き戻しでも正しく動く。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::voicevox;

/// キャッシュ型エイリアス: 行文字列 → WAV bytes
pub type WavCache = Arc<Mutex<HashMap<String, Vec<u8>>>>;

/// フェッチ中フラグ型エイリアス
pub type IsFetching = Arc<AtomicBool>;

#[derive(Debug)]
pub struct FetchRequest {
    pub text:       String,
    pub play_after: bool,
}

pub fn spawn_worker(
    rx:          mpsc::Receiver<FetchRequest>,
    cache:       WavCache,
    play_tx:     mpsc::Sender<Vec<u8>>,
    is_fetching: IsFetching,
) {
    tokio::spawn(worker_loop(rx, cache, play_tx, is_fetching));
}

async fn worker_loop(
    mut rx:      mpsc::Receiver<FetchRequest>,
    cache:       WavCache,
    play_tx:     mpsc::Sender<Vec<u8>>,
    is_fetching: IsFetching,
) {
    let mut current_handle: Option<tokio::task::JoinHandle<()>> = None;

    while let Some(req) = rx.recv().await {
        // 前のfetchをキャンセルして、fetchが常に単一となるようにする
        if let Some(handle) = current_handle.take() {
            handle.abort();
            is_fetching.store(false, Ordering::Relaxed);
        }

        if req.text.trim().is_empty() { continue; }

        let cached: Option<Vec<u8>> = {
            cache.lock().unwrap().get(&req.text).cloned()
        };
        if let Some(wav) = cached {
            if req.play_after { let _ = play_tx.send(wav).await; }
            continue;
        }

        is_fetching.store(true, Ordering::Relaxed);

        let cache_clone       = Arc::clone(&cache);
        let play_tx_clone     = play_tx.clone();
        let is_fetching_clone = Arc::clone(&is_fetching);

        current_handle = Some(tokio::spawn(async move {
            match voicevox::synthesize_line(&req.text).await {
                Ok(wav) => {
                    { cache_clone.lock().unwrap().insert(req.text.clone(), wav.clone()); }
                    if req.play_after { let _ = play_tx_clone.send(wav).await; }
                }
                Err(e) => eprintln!("[fetch error] {e}"),
            }
            // タスクがabortされた場合はここに到達しないため、
            // フラグのリセットはworker_loopのabort直後のstore(false)が担う
            is_fetching_clone.store(false, Ordering::Relaxed);
        }));
    }
}
