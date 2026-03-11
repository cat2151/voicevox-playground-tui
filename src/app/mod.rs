//! アプリケーション状態と状態遷移ロジック。

mod help;
mod insert_mode;
mod intonation_mode;
mod normal_mode;
mod tab_ops;
mod utils;

pub use help::{HelpAction, HELP_ENTRIES};

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tui_textarea::TextArea;

use crate::background_prefetch;
use crate::fetch::{FetchRequest, IsFetching, WavCache};
use crate::player;

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Insert,
    /// キーボードによる簡易イントネーション編集モード
    Intonation,
    /// コロンコマンド入力モード（例: :tabnew）
    Command,
    /// hキーで開くヘルプメニューモード
    Help,
}

/// 行ごとのイントネーション編集データ（行インデックスに対応して保持する）。
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct IntonationLineData {
    /// 合成に使うaudio_query JSON（pitch値が編集済み）
    pub query:      serde_json::Value,
    /// モーラ表示テキスト一覧
    pub mora_texts: Vec<String>,
    /// 現在のpitch値一覧
    pub pitches:    Vec<f64>,
    /// 合成に使うspeaker_id
    pub speaker_id: u32,
}

/// アップデート実行方法の選択結果
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateAction {
    /// 表でアップデート（端末にビルドログを表示）
    Foreground,
}

pub struct App {
    pub lines:         Vec<String>,
    pub cursor:        usize,
    pub textarea:      TextArea<'static>,
    pub mode:          Mode,
    /// キャッシュキー = 行文字列（インデックスではない）
    pub cache:         WavCache,
    pub status_msg:    String,
    pub fetch_tx:      mpsc::Sender<FetchRequest>,
    pub play_tx:       mpsc::Sender<Vec<u8>>,
    pub visible_lines: usize,
    pub pending_d:     bool,
    /// "z"キー待機中（zm/zrのプレフィックス）
    pub pending_z:     bool,
    /// "g"キー待機中（gt/gTのプレフィックス）
    pub pending_g:     bool,
    /// `"`キー待機中（レジスタ指定のプレフィックス）
    pub pending_quote: bool,
    /// `"+`入力済み（クリップボードペーストのプレフィックス）
    pub pending_clipboard: bool,
    pub yank_buf:      Option<String>,
    /// 折りたたみ中かどうか（行頭spaceのある行を非表示にする）
    pub folded:        bool,
    /// fetchワーカーがAPI呼び出し中かどうか
    pub is_fetching:   IsFetching,
    /// アップデートが利用可能かどうか（バックグラウンドチェックがセットする）
    pub update_available: Arc<AtomicBool>,
    /// ユーザーが選択したアップデート実行方法
    pub update_action: Option<UpdateAction>,
    /// バックグラウンドprefetchタスクのハンドル（カーソル移動時にキャンセル）
    bg_prefetch_handle: Option<JoinHandle<()>>,
    /// NormalモードでESCを押した際に"q:quit"ヒントをハイライト表示する期限
    pub esc_hint_until: Option<Instant>,
    /// 最後にオートセーブを実行した時刻
    pub last_autosave: Instant,
    /// 端末ウィンドウにフォーカスがあるかどうか（FocusLost/FocusGainedで更新）
    pub focused: bool,
    /// Normalモードの数値プレフィックスバッファ（例: "10j" の "10" 部分）
    pub count_buf: String,
    /// タブごとの (lines, line_intonations, cursor, folded) を保存するリスト（アクティブタブ含む全タブ）
    pub tabs:           Vec<(Vec<String>, Vec<Option<IntonationLineData>>, usize, bool)>,
    /// 現在アクティブなタブのインデックス（0始まり）
    pub active_tab:     usize,
    /// コマンドモード（":tabnew" など）の入力バッファ
    pub command_buf:    String,
    /// ヘルプモードで入力中のキーバッファ（前方一致ハイライト・完全一致実行に使う）
    pub help_key_buf:   String,
    // ── イントネーション編集 ──────────────────────────────────────────────────────
    /// 行インデックスごとのイントネーション編集データ（lines と同じ長さで同期される）
    pub line_intonations:      Vec<Option<IntonationLineData>>,
    /// イントネーション編集セッション中のspeaker_id
    pub intonation_speaker_id: u32,
    /// イントネーション編集セッション中のモーラ表示テキスト一覧
    pub intonation_mora_texts: Vec<String>,
    /// イントネーション編集セッション中のpitch値一覧（編集可能）
    pub intonation_pitches:    Vec<f64>,
    /// イントネーション編集セッション中のaudio_query JSON（pitch値適用済み）
    pub intonation_query:      serde_json::Value,
    /// 現在選択中のモーラインデックス（a-z/A-Zキーで更新）
    pub intonation_cursor:     usize,
    /// 数値直接入力バッファ（非空のとき数値入力サブモード）
    pub intonation_num_buf:    String,
    /// イントネーション編集セッション開始時のpitch値スナップショット（iキーで初期化に使う）
    pub intonation_initial_pitches: Vec<f64>,
    /// a-zA-Zキーによる再生デバウンス期限（1秒）
    pub intonation_debounce:   Option<Instant>,
    /// イントネーション合成再生タスクのハンドル（新規再生時にabortして上書き）
    pub intonation_play_handle: Option<JoinHandle<()>>,
    // ── イントネーション擬似折れ線グラフ（マウスイベント処理用） ──────────────────
    /// グラフ描画エリアの左上x座標（絶対座標）
    pub intonation_graph_x:         u16,
    /// グラフ描画エリアの左上y座標（絶対座標）
    pub intonation_graph_y:         u16,
    /// グラフ描画エリアの高さ（行数）
    pub intonation_graph_h:         u16,
    /// グラフの先頭行（row 0）に対応するpitch値
    pub intonation_graph_pitch_top: f64,
    /// 各モーラ列の開始x座標（絶対座標）
    pub intonation_mora_col_x:      Vec<u16>,
    /// 各モーラ列の幅（端末列数）
    pub intonation_mora_col_w:      Vec<u16>,
}

impl App {
    /// 複数タブの初期内容を指定してアプリを生成する。
    /// `all_lines[0]` がタブ1（history.txt）、`all_lines[1]` がタブ2（history2.txt）… に対応する。
    /// `all_intonations` は対応するタブのイントネーションデータ（存在しなければ空 Vec でよい）。
    pub fn new_with_tabs(
        all_lines:       Vec<Vec<String>>,
        all_intonations: Vec<Vec<Option<IntonationLineData>>>,
    ) -> Self {
        let mut all_lines = all_lines;
        if all_lines.is_empty() {
            all_lines.push(vec![String::new()]);
        }
        let mut all_intonations = all_intonations;

        // 最初のタブの内容でアプリを初期化する
        let first_lines = utils::compress_trailing_empty(all_lines.remove(0));
        let first_intonations = if all_intonations.is_empty() { vec![] } else { all_intonations.remove(0) };
        let mut app = Self::new_with_intonations(first_lines, first_intonations);

        // 残りのタブをtabsに追加する（タブ0のスロットは既に確保済み）
        for (i, extra_lines) in all_lines.into_iter().enumerate() {
            let extra_lines = utils::compress_trailing_empty(extra_lines);
            let extra_cursor = if extra_lines.is_empty() { 0 } else { extra_lines.len() - 1 };
            // 対応するイントネーションデータがあれば使い、なければ全Noneで埋める
            let mut extra_intonations: Vec<Option<IntonationLineData>> = vec![None; extra_lines.len()];
            if let Some(loaded) = all_intonations.get(i) {
                for (j, slot) in loaded.iter().enumerate() {
                    if j < extra_intonations.len() {
                        extra_intonations[j] = slot.clone();
                    }
                }
            }
            app.tabs.push((extra_lines, extra_intonations, extra_cursor, false));
        }

        app
    }

    /// `new` と同様だが、初期イントネーションデータも受け取る。
    fn new_with_intonations(lines: Vec<String>, intonations: Vec<Option<IntonationLineData>>) -> Self {
        let mut app = Self::new(lines);
        // 渡されたイントネーションデータを行数に合わせてマージする
        for (i, slot) in intonations.into_iter().enumerate() {
            if i < app.line_intonations.len() {
                app.line_intonations[i] = slot;
            }
        }
        app
    }

    pub fn new(lines: Vec<String>) -> Self {
        let lines = utils::compress_trailing_empty(lines);
        let line_intonations = vec![None; lines.len()];
        let cache: WavCache = Arc::new(Mutex::new(HashMap::new()));

        let (play_tx, play_rx) = mpsc::channel::<Vec<u8>>(8);
        player::spawn_player(play_rx);

        let is_fetching: IsFetching = Arc::new(AtomicBool::new(false));
        let (fetch_tx, fetch_rx) = mpsc::channel::<FetchRequest>(64);
        crate::fetch::spawn_worker(fetch_rx, Arc::clone(&cache), play_tx.clone(), Arc::clone(&is_fetching));

        let cursor = if lines.is_empty() { 0 } else { lines.len() - 1 };
        // tabs[0] のlinesはプレースホルダー（実際のlinesはself.linesに保持される）。
        // タブ切り替え時にmem::swapでlinesを交換するため、初期値は空vecで良い。
        let tabs = vec![(vec![], vec![], 0usize, false)];
        Self {
            lines, cursor,
            line_intonations,
            textarea:      TextArea::default(),
            mode:          Mode::Normal,
            cache,
            status_msg:    String::from("ready"),
            fetch_tx, play_tx,
            visible_lines: 24,
            pending_d:     false,
            pending_z:     false,
            pending_g:     false,
            pending_quote: false,
            pending_clipboard: false,
            yank_buf:      None,
            folded:        false,
            is_fetching,
            update_available: Arc::new(AtomicBool::new(false)),
            update_action: None,
            bg_prefetch_handle: None,
            esc_hint_until: None,
            last_autosave: Instant::now(),
            focused: true,
            count_buf: String::new(),
            tabs,
            active_tab:    0,
            command_buf:   String::new(),
            help_key_buf:  String::new(),
            intonation_speaker_id: 0,
            intonation_mora_texts: Vec::new(),
            intonation_pitches:    Vec::new(),
            intonation_initial_pitches: Vec::new(),
            intonation_query:      serde_json::Value::Null,
            intonation_cursor:     0,
            intonation_num_buf:    String::new(),
            intonation_debounce:   None,
            intonation_play_handle: None,
            intonation_graph_x:         0,
            intonation_graph_y:         0,
            intonation_graph_h:         0,
            intonation_graph_pitch_top: 0.0,
            intonation_mora_col_x:      Vec::new(),
            intonation_mora_col_w:      Vec::new(),
        }
    }

    pub async fn init(&mut self) {
        let idx = self.cursor;
        self.fetch_and_play(idx).await;
        self.restart_background_prefetch();
    }

    // ── 共通ヘルパー ──────────────────────────────────────────────────────────

    /// 折りたたみ状態を考慮した表示行インデックスのリストを返す。
    pub fn visible_line_indices(&self) -> Vec<usize> {
        if self.folded {
            (0..self.lines.len())
                .filter(|&i| !self.lines[i].starts_with(' '))
                .collect()
        } else {
            (0..self.lines.len()).collect()
        }
    }

    /// 表示行リスト内でのカーソル位置を返す（非表示行の場合は最近傍の表示行位置）。
    pub fn vis_cursor_pos(&self) -> usize {
        utils::nearest_vis_pos(self.cursor, &self.visible_line_indices())
    }

    /// 折りたたみ時にカーソルが非表示行にある場合、最も近い表示行に移動する。
    fn normalize_cursor_for_fold(&mut self) {
        if !self.folded { return; }
        let visible = self.visible_line_indices();
        if visible.is_empty() || visible.contains(&self.cursor) { return; }
        if let Some(&c) = visible.get(utils::nearest_vis_pos(self.cursor, &visible)) {
            self.cursor = c;
        }
    }

    /// すべての pending プレフィックスフラグをリセットする。
    /// キーハンドラおよびアクションメソッドの冒頭で呼ぶ共通ヘルパー。
    pub fn reset_pending_prefixes(&mut self) {
        self.pending_d = false;
        self.pending_z = false;
        self.pending_g = false;
        self.pending_quote = false;
        self.pending_clipboard = false;
        self.count_buf.clear();
    }

    // ── 内部ヘルパー ──────────────────────────────────────────────────────────

    /// イントネーションキャッシュキーを生成する。
    /// シリアライズに失敗した場合は None を返す（キャッシュをスキップする）。
    pub(crate) fn intonation_cache_key(speaker_id: u32, query: &serde_json::Value) -> Option<String> {
        serde_json::to_string(query).ok().map(|q| format!("intonation:{}:{}", speaker_id, q))
    }

    async fn fetch_and_play(&mut self, index: usize) {
        if index >= self.lines.len() || self.lines[index].trim().is_empty() { return; }
        let text = self.lines[index].clone();

        // イントネーション編集済みの場合はキャッシュを確認し、あれば即再生、なければ合成してキャッシュに保存する
        if let Some(data) = self.line_intonations.get(index).and_then(|d| d.as_ref()).cloned() {
            // query が Null の場合は history.txt から復元した pitches-only 状態を示す。
            // この場合は audio_query をAPIから遅延取得し、完全なIntonationLineDataに昇格させてから再生する。
            let data = if data.query.is_null() {
                match self.resolve_pitches_only(index, &data).await {
                    Some(resolved) => resolved,
                    None => {
                        // API取得に失敗した場合は通常の合成にフォールスルー
                        let cached = { self.cache.lock().unwrap().get(&text).cloned() };
                        if let Some(wav) = cached {
                            let _ = self.play_tx.send(wav).await;
                            self.status_msg = format!("[♪ cached] line {}", index + 1);
                        } else {
                            let _ = self.fetch_tx.send(FetchRequest { text, play_after: true }).await;
                            self.status_msg = format!("[fetching...] line {}", index + 1);
                        }
                        return;
                    }
                }
            } else {
                data
            };

            if let Some(cache_key) = Self::intonation_cache_key(data.speaker_id, &data.query) {
                let cached = { self.cache.lock().unwrap().get(&cache_key).cloned() };
                if let Some(wav) = cached {
                    let _ = self.play_tx.send(wav).await;
                    self.status_msg = format!("[♬ cached] line {}", index + 1);
                    return;
                }
            }
            self.spawn_intonation_play(data.query, data.speaker_id);
            self.status_msg = format!("[♬ intonation] line {}", index + 1);
            return;
        }

        let cached = { self.cache.lock().unwrap().get(&text).cloned() };
        if let Some(wav) = cached {
            let _ = self.play_tx.send(wav).await;
            self.status_msg = format!("[♪ cached] line {}", index + 1);
        } else {
            let _ = self.fetch_tx.send(FetchRequest { text, play_after: true }).await;
            self.status_msg = format!("[fetching...] line {}", index + 1);
        }
    }

    /// pitches-onlyのIntonationLineData（queryがNull）に対してaudio_queryをAPIから取得し、
    /// 保存済みpitchesを適用してline_intonationsを更新する。
    /// pitches適用後にqueryから再抽出することでモーラ数との整合性を保つ。
    /// 成功した場合は解決済みのIntonationLineDataを返す。
    pub(super) async fn resolve_pitches_only(
        &mut self,
        index: usize,
        data: &IntonationLineData,
    ) -> Option<IntonationLineData> {
        let line = self.lines.get(index)?.clone();
        if line.trim().is_empty() { return None; }
        let mut segments = crate::tag::parse_line(&line);
        if segments.len() != 1 { return None; }
        let (seg_text, ctx) = segments.swap_remove(0);
        let speaker_id = ctx.speaker_id;
        match crate::voicevox::get_audio_query(&seg_text, speaker_id).await {
            Ok(mut query) => {
                // 保存済みpitchesを適用した後、queryから再抽出して長さをモーラ数に揃える
                crate::voicevox::set_mora_pitches(&mut query, &data.pitches);
                let (mora_texts, applied_pitches) = crate::voicevox::extract_mora_data(&query);
                if mora_texts.is_empty() { return None; }
                let resolved = IntonationLineData {
                    query: query.clone(),
                    mora_texts,
                    pitches: applied_pitches,
                    speaker_id,
                };
                if index < self.line_intonations.len() {
                    self.line_intonations[index] = Some(resolved.clone());
                }
                Some(resolved)
            }
            Err(_) => None,
        }
    }

    /// イントネーションqueryを使って合成・再生するタスクを起動する。
    /// 前回のタスクがあればabortしてから新しいタスクを起動する（並列実行を防ぐ）。
    /// 合成結果はWavCacheに保存し、次回以降の再生でキャッシュから即時再生できるようにする。
    pub(super) fn spawn_intonation_play(&mut self, query: serde_json::Value, speaker_id: u32) {
        if let Some(h) = self.intonation_play_handle.take() {
            h.abort();
        }
        let play_tx = self.play_tx.clone();
        let cache = Arc::clone(&self.cache);
        let cache_key = Self::intonation_cache_key(speaker_id, &query);
        self.intonation_play_handle = Some(tokio::spawn(async move {
            if let Ok(wav) = crate::voicevox::synthesize_with_query(&query, speaker_id).await {
                if let Some(key) = cache_key {
                    cache.lock().unwrap().insert(key, wav.clone());
                }
                let _ = play_tx.send(wav).await;
            }
        }));
    }

    /// イントネーションキャッシュの古いエントリをすべて削除する。
    /// イントネーション確定時に呼び出し、中間的な pitch 編集で蓄積した不要エントリを解放する。
    pub(super) fn evict_intonation_cache(&mut self) {
        self.cache.lock().unwrap().retain(|k, _| !k.starts_with("intonation:"));
    }

    /// 現在行のfetch完了後、表示範囲内のcacheのない行を裏で1行ずつfetchする。
    /// 前回のタスクがあればキャンセルしてから新たに起動する。
    fn restart_background_prefetch(&mut self) {
        if let Some(h) = self.bg_prefetch_handle.take() {
            h.abort();
        }
        // カーソル行がイントネーション編集済みの場合はイントネーション用のキャッシュキーを使う。
        // 通常の行テキストをキーとすると、イントネーション合成結果がキャッシュされないため
        // wait_for_cachedが30秒タイムアウトするまで他の行のprefetchが始まらない。
        let cursor_cache_key = {
            let intonation_key = self.line_intonations
                .get(self.cursor)
                .and_then(|d| d.as_ref())
                .filter(|d| !d.query.is_null())
                .and_then(|d| Self::intonation_cache_key(d.speaker_id, &d.query));
            intonation_key.unwrap_or_else(|| self.lines.get(self.cursor).cloned().unwrap_or_default())
        };
        // 折りたたみ時は表示行のみをprefetch対象とする
        let target_texts: Vec<String> = if self.folded {
            let visible_indices = self.visible_line_indices();
            let visible_texts: Vec<String> = visible_indices.iter().map(|&i| self.lines[i].clone()).collect();
            let vis_cursor = utils::nearest_vis_pos(self.cursor, &visible_indices);
            background_prefetch::compute_prefetch_targets(vis_cursor, self.visible_lines, &visible_texts)
                .into_iter()
                .map(|idx| visible_texts[idx].clone())
                .collect()
        } else {
            // 全行ではなく表示ウィンドウ内の対象行のみをcloneして渡す
            background_prefetch::compute_prefetch_targets(
                self.cursor, self.visible_lines, &self.lines,
            )
            .into_iter()
            .map(|idx| self.lines[idx].clone())
            .collect()
        };
        self.bg_prefetch_handle = Some(background_prefetch::spawn_background_prefetch(
            cursor_cache_key,
            target_texts,
            Arc::clone(&self.cache),
            Arc::clone(&self.is_fetching),
            self.fetch_tx.clone(),
        ));
    }
}
