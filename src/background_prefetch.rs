//! 現在行のcacheが獲得できたあと、裏で、cacheのない行を探索してcacheを取得する機能。
//!
//! fetch_and_play()が完了（is_fetchingがfalseになり、かつ現在行がcacheに入る）したあと、
//! 表示範囲内のcacheのない行を近い順に1行ずつfetchする。
//! カーソル移動や再生操作が来たとき（spawn_background_prefetchの戻り値をabort()）でキャンセルできる。

use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::fetch::{FetchRequest, IsFetching, WavCache};

/// バックグラウンドprefetchタスクを起動する。
/// 返されたJoinHandleをabort()することで中断できる。
///
/// - `cursor_cache_key`: 現在行のキャッシュキー（通常行はテキスト、イントネーション編集済み行は"intonation:{speaker_id}:{query_json}"）
/// - `target_texts`:     prefetch対象テキストのリスト（カーソル位置から近い順）
pub fn spawn_background_prefetch(
    cursor_cache_key: String,
    target_texts: Vec<String>,
    cache: WavCache,
    is_fetching: IsFetching,
    fetch_tx: mpsc::Sender<FetchRequest>,
) -> JoinHandle<()> {
    tokio::spawn(run_background_prefetch(
        cursor_cache_key,
        target_texts,
        cache,
        is_fetching,
        fetch_tx,
    ))
}

async fn run_background_prefetch(
    cursor_cache_key: String,
    target_texts: Vec<String>,
    cache: WavCache,
    is_fetching: IsFetching,
    fetch_tx: mpsc::Sender<FetchRequest>,
) {
    // 現在行のfetchが完了するまで待機する
    wait_for_fetch_complete(&is_fetching).await;

    // is_fetchingがfalseになっても、fetch_and_play()がキューに積まれた直後など
    // 現在行のcacheがまだ用意されていない場合がある。
    // cacheに格納されるまで追加で待機する。
    if !cursor_cache_key.trim().is_empty() && !cache.lock().unwrap().contains_key(&cursor_cache_key)
    {
        wait_for_cached(&cache, &cursor_cache_key, Duration::from_secs(30)).await;
    }

    for text in target_texts {
        if text.trim().is_empty() {
            continue;
        }
        if cache.lock().unwrap().contains_key(&text) {
            continue;
        }

        // prefetchリクエストを1件送信
        if fetch_tx
            .send(FetchRequest {
                text: text.clone(),
                play_after: false,
            })
            .await
            .is_err()
        {
            break;
        }

        // cacheに格納されるまで待機（中断された場合はタイムアウトで次へ）
        wait_for_cached(&cache, &text, Duration::from_secs(30)).await;
    }
}

/// is_fetchingがfalseになるまで待機する（最大30秒）
async fn wait_for_fetch_complete(is_fetching: &IsFetching) {
    let timeout = Duration::from_secs(30);
    let interval = Duration::from_millis(100);
    let iterations = (timeout.as_millis() / interval.as_millis()) as u32;
    for _ in 0..iterations {
        if !is_fetching.load(Ordering::Relaxed) {
            return;
        }
        tokio::time::sleep(interval).await;
    }
}

/// textがcacheに格納されるまで待機する（タイムアウト付き）
async fn wait_for_cached(cache: &WavCache, text: &str, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if cache.lock().unwrap().contains_key(text) {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// カーソル位置から近い順に表示ウィンドウ内の行インデックスを返す
pub fn compute_prefetch_targets(
    cursor: usize,
    visible_lines: usize,
    lines: &[String],
) -> Vec<usize> {
    let len = lines.len();
    if len == 0 {
        return vec![];
    }

    let half = visible_lines / 2;
    let win_start = cursor.saturating_sub(half);
    let win_end = (win_start + visible_lines).min(len);
    let win_start = win_end.saturating_sub(visible_lines);

    let mut targets: Vec<usize> = Vec::new();
    for d in 1..=visible_lines as i32 {
        for &delta in &[d, -d] {
            let idx = cursor as i32 + delta;
            if idx >= win_start as i32 && idx < win_end as i32 {
                targets.push(idx as usize);
            }
        }
    }
    targets.dedup();
    targets
}

#[cfg(test)]
#[path = "tests/background_prefetch.rs"]
mod tests;
