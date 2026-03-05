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
/// - `cursor_text`:  現在行のテキスト（再生fetch完了待ちに使う）
/// - `target_texts`: prefetch対象テキストのリスト（カーソル位置から近い順）
pub fn spawn_background_prefetch(
    cursor_text:  String,
    target_texts: Vec<String>,
    cache:        WavCache,
    is_fetching:  IsFetching,
    fetch_tx:     mpsc::Sender<FetchRequest>,
) -> JoinHandle<()> {
    tokio::spawn(run_background_prefetch(
        cursor_text, target_texts, cache, is_fetching, fetch_tx,
    ))
}

async fn run_background_prefetch(
    cursor_text:  String,
    target_texts: Vec<String>,
    cache:        WavCache,
    is_fetching:  IsFetching,
    fetch_tx:     mpsc::Sender<FetchRequest>,
) {
    // 現在行のfetchが完了するまで待機する
    wait_for_fetch_complete(&is_fetching).await;

    // is_fetchingがfalseになっても、fetch_and_play()がキューに積まれた直後など
    // 現在行のcacheがまだ用意されていない場合がある。
    // cacheに格納されるまで追加で待機する。
    if !cursor_text.trim().is_empty() && !cache.lock().unwrap().contains_key(&cursor_text) {
        wait_for_cached(&cache, &cursor_text, Duration::from_secs(30)).await;
    }

    for text in target_texts {
        if text.trim().is_empty() { continue; }
        if cache.lock().unwrap().contains_key(&text) { continue; }

        // prefetchリクエストを1件送信
        if fetch_tx
            .send(FetchRequest { text: text.clone(), play_after: false })
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
        if !is_fetching.load(Ordering::Relaxed) { return; }
        tokio::time::sleep(interval).await;
    }
}

/// textがcacheに格納されるまで待機する（タイムアウト付き）
async fn wait_for_cached(cache: &WavCache, text: &str, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if cache.lock().unwrap().contains_key(text) { return; }
        if tokio::time::Instant::now() >= deadline { return; }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// カーソル位置から近い順に表示ウィンドウ内の行インデックスを返す
pub fn compute_prefetch_targets(cursor: usize, visible_lines: usize, lines: &[String]) -> Vec<usize> {
    let len = lines.len();
    if len == 0 { return vec![]; }

    let half      = visible_lines / 2;
    let win_start = cursor.saturating_sub(half);
    let win_end   = (win_start + visible_lines).min(len);
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
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::sync::atomic::AtomicBool;

    #[test]
    fn compute_prefetch_targets_empty() {
        let targets = compute_prefetch_targets(0, 24, &[]);
        assert!(targets.is_empty());
    }

    #[test]
    fn compute_prefetch_targets_excludes_cursor() {
        let lines: Vec<String> = (0..10).map(|i| format!("line{}", i)).collect();
        let targets = compute_prefetch_targets(5, 4, &lines);
        assert!(!targets.contains(&5), "カーソル行自身はtargetsに含まれない");
    }

    #[test]
    fn compute_prefetch_targets_within_window() {
        let lines: Vec<String> = (0..10).map(|i| format!("line{}", i)).collect();
        // cursor=5, visible_lines=4: half=2, win_start=3, win_end=7, window=[3,7)
        let targets = compute_prefetch_targets(5, 4, &lines);
        for &t in &targets {
            assert!(t >= 3 && t < 7, "target {} は表示ウィンドウ[3,7)内にあるべき", t);
        }
    }

    #[test]
    fn compute_prefetch_targets_no_duplicates() {
        let lines: Vec<String> = (0..20).map(|i| format!("line{}", i)).collect();
        let targets = compute_prefetch_targets(10, 6, &lines);
        let mut sorted = targets.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(targets.len(), sorted.len(), "targetsに重複がないこと");
    }

    #[test]
    fn compute_prefetch_targets_at_start() {
        let lines: Vec<String> = (0..5).map(|i| format!("line{}", i)).collect();
        let targets = compute_prefetch_targets(0, 4, &lines);
        assert!(!targets.contains(&0), "カーソル行(0)はtargetsに含まれない");
        for &t in &targets {
            assert!(t > 0, "インデックス0以外の行のみ");
        }
    }

    #[tokio::test]
    async fn background_prefetch_skips_all_cached_lines() {
        let (tx, mut rx) = mpsc::channel::<FetchRequest>(16);
        let cache: WavCache = Arc::new(Mutex::new(HashMap::new()));
        let is_fetching: IsFetching = Arc::new(AtomicBool::new(false));

        // 現在行（cursor_text）と隣接行をすべてキャッシュ済みにする
        {
            let mut c = cache.lock().unwrap();
            c.insert("line0".into(), vec![1, 2, 3]);
            c.insert("line1".into(), vec![0]);
            c.insert("line2".into(), vec![4, 5, 6]);
        }

        let handle = spawn_background_prefetch(
            "line1".into(),
            vec!["line0".into(), "line2".into()],
            Arc::clone(&cache), Arc::clone(&is_fetching), tx,
        );
        handle.await.unwrap();

        // すべてキャッシュ済みのためfetchリクエストは送信されない
        assert!(rx.try_recv().is_err(), "キャッシュ済み行へのリクエストは不要");
    }

    #[tokio::test]
    async fn background_prefetch_waits_until_is_fetching_false() {
        let (tx, mut rx) = mpsc::channel::<FetchRequest>(16);
        let cache: WavCache = Arc::new(Mutex::new(HashMap::new()));
        let is_fetching: IsFetching = Arc::new(AtomicBool::new(true));

        // cursor_textをcacheに入れておく（is_fetching待ちのみをテストする）
        cache.lock().unwrap().insert("line1".into(), vec![]);

        let is_fetching_clone = Arc::clone(&is_fetching);
        let _handle = spawn_background_prefetch(
            "line1".into(),
            vec!["line0".into(), "line2".into()],
            Arc::clone(&cache), Arc::clone(&is_fetching), tx,
        );

        // is_fetching=trueの間はリクエストを送らない（50ms以内に来ないことを確認）
        let result = tokio::time::timeout(Duration::from_millis(50), rx.recv()).await;
        assert!(result.is_err(), "is_fetching=trueの間はprefetchリクエストを送らないこと");

        // is_fetching=falseにするとリクエストが来る
        is_fetching_clone.store(false, Ordering::Relaxed);
        let req = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("タイムアウト: is_fetching=false後にリクエストが来るはず")
            .expect("チャネルが閉じた");
        assert!(req.text == "line0" || req.text == "line2");
        assert!(!req.play_after);
    }

    #[tokio::test]
    async fn background_prefetch_sends_one_request_at_a_time() {
        let (tx, mut rx) = mpsc::channel::<FetchRequest>(16);
        let cache: WavCache = Arc::new(Mutex::new(HashMap::new()));
        let is_fetching: IsFetching = Arc::new(AtomicBool::new(false));

        // cursor_textをcacheに入れておく（one-at-a-timeのみをテストする）
        cache.lock().unwrap().insert("line2".into(), vec![]);

        let cache_clone = Arc::clone(&cache);
        let handle = spawn_background_prefetch(
            "line2".into(),
            vec!["line1".into(), "line3".into(), "line0".into(), "line4".into()],
            Arc::clone(&cache), Arc::clone(&is_fetching), tx,
        );

        // 1件目のリクエストを受け取る
        let req1 = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("タイムアウト: 1件目のリクエストが来るはず")
            .expect("チャネルが閉じた");
        assert_ne!(req1.text, "line2", "カーソル行はprefetchしない");
        assert!(!req1.play_after);

        // 1件目がキャッシュに入るまで2件目は来ない（50ms以内に来ないことを確認）
        let result = tokio::time::timeout(Duration::from_millis(50), rx.recv()).await;
        assert!(result.is_err(), "1件目がキャッシュに入るまで2件目のリクエストは来ないこと");

        // 1件目をキャッシュに入れる
        cache_clone.lock().unwrap().insert(req1.text.clone(), vec![1]);

        // 2件目のリクエストが来る
        let req2 = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("タイムアウト: 2件目のリクエストが来るはず")
            .expect("チャネルが閉じた");
        assert_ne!(req2.text, req1.text, "2件目は1件目と別の行");
        assert!(!req2.play_after);

        handle.abort();
    }
}

