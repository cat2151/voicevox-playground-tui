use super::*;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

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
        assert!(
            (3..7).contains(&t),
            "target {} は表示ウィンドウ[3,7)内にあるべき",
            t
        );
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

    // 現在行（cursor_cache_key）と隣接行をすべてキャッシュ済みにする
    {
        let mut c = cache.lock().unwrap();
        c.insert("line0".into(), vec![1, 2, 3]);
        c.insert("line1".into(), vec![0]);
        c.insert("line2".into(), vec![4, 5, 6]);
    }

    let handle = spawn_background_prefetch(
        "line1".into(),
        vec!["line0".into(), "line2".into()],
        Arc::clone(&cache),
        Arc::clone(&is_fetching),
        tx,
    );
    handle.await.unwrap();

    // すべてキャッシュ済みのためfetchリクエストは送信されない
    assert!(
        rx.try_recv().is_err(),
        "キャッシュ済み行へのリクエストは不要"
    );
}

#[tokio::test]
async fn background_prefetch_waits_until_is_fetching_false() {
    let (tx, mut rx) = mpsc::channel::<FetchRequest>(16);
    let cache: WavCache = Arc::new(Mutex::new(HashMap::new()));
    let is_fetching: IsFetching = Arc::new(AtomicBool::new(true));

    // cursor_cache_keyをcacheに入れておく（is_fetching待ちのみをテストする）
    cache.lock().unwrap().insert("line1".into(), vec![]);

    let is_fetching_clone = Arc::clone(&is_fetching);
    let _handle = spawn_background_prefetch(
        "line1".into(),
        vec!["line0".into(), "line2".into()],
        Arc::clone(&cache),
        Arc::clone(&is_fetching),
        tx,
    );

    // is_fetching=trueの間はリクエストを送らない（50ms以内に来ないことを確認）
    let result = tokio::time::timeout(Duration::from_millis(50), rx.recv()).await;
    assert!(
        result.is_err(),
        "is_fetching=trueの間はprefetchリクエストを送らないこと"
    );

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

    // cursor_cache_keyをcacheに入れておく（one-at-a-timeのみをテストする）
    cache.lock().unwrap().insert("line2".into(), vec![]);

    let cache_clone = Arc::clone(&cache);
    let handle = spawn_background_prefetch(
        "line2".into(),
        vec![
            "line1".into(),
            "line3".into(),
            "line0".into(),
            "line4".into(),
        ],
        Arc::clone(&cache),
        Arc::clone(&is_fetching),
        tx,
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
    assert!(
        result.is_err(),
        "1件目がキャッシュに入るまで2件目のリクエストは来ないこと"
    );

    // 1件目をキャッシュに入れる
    cache_clone
        .lock()
        .unwrap()
        .insert(req1.text.clone(), vec![1]);

    // 2件目のリクエストが来る
    let req2 = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("タイムアウト: 2件目のリクエストが来るはず")
        .expect("チャネルが閉じた");
    assert_ne!(req2.text, req1.text, "2件目は1件目と別の行");
    assert!(!req2.play_after);

    handle.abort();
}
