//! イントネーション編集モードの操作。
//!
//! # モード遷移
//! Normal →(v)→ Intonation →(Esc/Enter)→ Normal
//!
//! # キーバインド（Intonationモード）
//! - h/←  : 選択モーラを左に移動（数値入力中は無効）
//! - l/→  : 選択モーラを右に移動（数値入力中は無効）
//! - k/↑  : 選択モーラの pitch を +0.1（1秒デバウンスで再生。数値入力中は無効）
//! - j/↓  : 選択モーラの pitch を -0.1（1秒デバウンスで再生。数値入力中は無効）
//! - a-z（※h/j/k/l を除く）: mora[0]-[25] の pitch を +0.1（1秒デバウンスで再生）
//! - A-Z（※H/J/K/L を除く）: mora[0]-[25] の pitch を -0.1（1秒デバウンスで再生）
//!         （アルファベット順で 0-25 番目のモーラを指定。h/j/k/l はカーソル移動・現在モーラ編集用に予約）
//! - i   : pitch を入力開始時の初期値にリセットして再生（数値入力中は無効）
//! - 0-9 : 数値直接入力サブモードへ（バッファに追記）
//! - .   : 小数点（バッファ空なら"0."として開始、重複不可）
//! - BS  : 数値バッファを1文字削除
//! - Space : 現在のintonationで即時再生
//! - Enter : 数値入力中なら確定→再生、そうでなければイントネーション確定してNormalへ
//! - Esc   : 数値入力中ならキャンセル、そうでなければイントネーション確定してNormalへ

use std::time::{Duration, Instant};

use crate::player::PlayRequest;
use crate::{tag, ui, voicevox};

use super::{App, IntonationLineData, Mode};

impl App {
    /// v: Intonationモードへ遷移する。
    /// 現在行のaudio_queryをAPIから取得（または既存データをロード）し、mora/pitch情報を初期化する。
    pub async fn enter_intonation_mode(&mut self) {
        self.reset_pending_prefixes();
        let idx = self.cursor;
        if idx >= self.lines.len() { return; }
        let line = self.lines[idx].clone();
        if line.trim().is_empty() { return; }

        // タグ解析してセグメント情報を取得する
        let mut segments = tag::parse_line(&line);
        // セグメントが存在しない場合は何もしない
        if segments.is_empty() {
            return;
        }
        // 複数セグメント（行中で話者/スタイルが切り替わる行）は現在の実装では扱えないためエラーにする
        if segments.len() != 1 {
            self.status_msg = String::from(
                "[intonation] 複数の話者/スタイルが含まれる行はイントネーション編集できません",
            );
            return;
        }
        let (text, ctx) = segments.swap_remove(0);
        let speaker_id = ctx.speaker_id;

        // 行ごとのイントネーションデータがあればそれを使う（前回編集を引き継ぐ）
        if let Some(Some(data)) = self.line_intonations.get(idx) {
            let data = data.clone();
            if data.query.is_null() {
                // pitches-only（ファイルからロードした直後）: resolve_pitches_onlyでaudio_queryを取得・適用する
                self.status_msg = String::from("[audio_query 取得中...]");
                match self.resolve_pitches_only(idx, &data).await {
                    Some(resolved) => {
                        self.intonation_speaker_id = resolved.speaker_id;
                        self.intonation_mora_texts = resolved.mora_texts;
                        self.intonation_pitches    = resolved.pitches;
                        self.intonation_query      = resolved.query;
                    }
                    None => {
                        self.status_msg = String::from("[intonation] audio_query の取得またはモーラ抽出に失敗");
                        return;
                    }
                }
            } else {
                self.intonation_speaker_id = data.speaker_id;
                self.intonation_mora_texts = data.mora_texts;
                self.intonation_pitches    = data.pitches;
                self.intonation_query      = data.query;
            }
        } else {
            // APIからaudio_queryを取得する
            self.status_msg = String::from("[audio_query 取得中...]");
            match voicevox::get_audio_query(&text, speaker_id).await {
                Ok(query) => {
                    let (mora_texts, pitches) = voicevox::extract_mora_data(&query);
                    if mora_texts.is_empty() {
                        self.status_msg = String::from("[intonation] モーラが取得できなかった");
                        return;
                    }
                    self.intonation_speaker_id = speaker_id;
                    self.intonation_mora_texts = mora_texts;
                    self.intonation_pitches    = pitches;
                    self.intonation_query      = query;
                }
                Err(e) => {
                    self.status_msg = format!("[audio_query error] {}", e);
                    return;
                }
            }
        }

        self.intonation_cursor   = 0;
        self.intonation_num_buf  = String::new();
        self.intonation_debounce = None;
        self.intonation_initial_pitches = self.intonation_pitches.clone();
        self.mode                = Mode::Intonation;
        self.status_msg          = String::from("-- INTONATION --");
    }

    /// h/←・l/→: 選択モーラを左右に移動する（delta=-1で左、+1で右）。
    pub fn intonation_move_cursor(&mut self, delta: i32) {
        let len = self.intonation_pitches.len();
        if len == 0 { return; }
        let new_cursor = (self.intonation_cursor as i32 + delta)
            .clamp(0, len as i32 - 1) as usize;
        self.intonation_cursor = new_cursor;
    }

    /// j/↓・k/↑: 現在選択中のモーラのpitchをdelta分増減する。
    pub fn intonation_adjust_current_pitch(&mut self, delta: f64) {
        let mora_idx = self.intonation_cursor;
        self.intonation_adjust_pitch(mora_idx, delta);
    }

    /// a-z/A-Z: 指定モーラのpitchをdelta分増減し、デバウンスタイマーをセットする。
    pub fn intonation_adjust_pitch(&mut self, mora_idx: usize, delta: f64) {
        if mora_idx >= self.intonation_pitches.len() { return; }
        self.intonation_cursor = mora_idx;
        let new_pitch = (self.intonation_pitches[mora_idx] + delta).clamp(0.0, 20.0);
        // 小数点1桁に丸める（浮動小数点誤差対策）
        self.intonation_pitches[mora_idx] = (new_pitch * 10.0).round() / 10.0;
        voicevox::set_mora_pitches(&mut self.intonation_query, &self.intonation_pitches);
        self.intonation_debounce = Some(Instant::now() + Duration::from_secs(1));
        self.status_msg = format!(
            "[♬] mora {} pitch {:.1}",
            mora_idx, self.intonation_pitches[mora_idx]
        );
    }

    /// 数値直接入力でのpitch確定: バッファをf64に変換して選択モーラに適用し再生する。
    pub async fn intonation_confirm_num_input(&mut self) {
        if let Ok(pitch) = self.intonation_num_buf.parse::<f64>() {
            let mora_idx = self.intonation_cursor;
            if mora_idx < self.intonation_pitches.len() {
                let clamped = pitch.clamp(0.0, 20.0);
                self.intonation_pitches[mora_idx] = (clamped * 10.0).round() / 10.0;
                voicevox::set_mora_pitches(&mut self.intonation_query, &self.intonation_pitches);
                self.status_msg = format!(
                    "[♬] mora {} pitch {:.1}",
                    mora_idx, self.intonation_pitches[mora_idx]
                );
                self.play_with_intonation_query().await;
            }
        }
        self.intonation_num_buf.clear();
    }

    /// ESC/Enter（数値入力なし）: イントネーションを確定してNormalモードへ戻る。
    pub async fn intonation_confirm(&mut self) {
        // 行インデックスごとにイントネーションデータを保存する。
        // ただし、入力前にデータがなく（None）かつpitchを変更していない場合は保存しない。
        // これにより、未編集のままESCで抜けてもキャッシュマークがつかない。
        let saved = if self.cursor < self.line_intonations.len() {
            let had_prior_data = self.line_intonations[self.cursor].is_some();
            let pitches_changed = self.intonation_pitches != self.intonation_initial_pitches;
            if pitches_changed || had_prior_data {
                self.line_intonations[self.cursor] = Some(IntonationLineData {
                    query:      self.intonation_query.clone(),
                    mora_texts: self.intonation_mora_texts.clone(),
                    pitches:    self.intonation_pitches.clone(),
                    speaker_id: self.intonation_speaker_id,
                });
                true
            } else {
                false
            }
        } else {
            false
        };
        self.intonation_debounce = None;
        self.mode       = Mode::Normal;
        self.status_msg = if saved {
            format!("[♬ intonation saved] line {}", self.cursor + 1)
        } else {
            String::from("[♬ intonation] no changes")
        };
        // 確定時に中間的な pitch 編集で蓄積した不要なキャッシュエントリを削除する
        self.evict_intonation_cache();
        // 確定と同時に再生する
        self.play_with_intonation_query().await;
    }

    /// マウスクリックでpitchを設定する。
    /// クリック位置のx座標からモーラ列を、y座標からpitch値を決定する。
    pub async fn intonation_handle_mouse_down(&mut self, col: u16, row: u16) {
        let gh = self.intonation_graph_h;
        let gx = self.intonation_graph_x;
        let gy = self.intonation_graph_y;
        let pitch_top = self.intonation_graph_pitch_top;

        if gh == 0 { return; }
        // グラフ描画エリア外のクリックは無視する
        if row < gy || row >= gy + gh { return; }
        if col < gx { return; }

        // クリックされたモーラ列を特定する
        let mut mora_idx: Option<usize> = None;
        for (i, (&x_start, &w)) in self.intonation_mora_col_x.iter()
            .zip(self.intonation_mora_col_w.iter())
            .enumerate()
        {
            if col >= x_start && col < x_start + w {
                mora_idx = Some(i);
                break;
            }
        }
        let Some(mora_idx) = mora_idx else { return; };
        if mora_idx >= self.intonation_pitches.len() { return; }

        // クリック行からpitch値を計算する（上端行 = pitch_top、以下0.1ずつ減少）
        let rel_row = row - gy;
        let new_pitch = pitch_top - rel_row as f64 * ui::PITCH_PER_ROW;
        let new_pitch = new_pitch.clamp(0.0, 20.0);
        let new_pitch = (new_pitch * 10.0).round() / 10.0;

        self.intonation_cursor = mora_idx;
        self.intonation_pitches[mora_idx] = new_pitch;
        // 数値入力サブモード中にクリックした場合はバッファをクリアして終了する
        self.intonation_num_buf.clear();
        voicevox::set_mora_pitches(&mut self.intonation_query, &self.intonation_pitches);
        self.intonation_debounce = Some(Instant::now() + Duration::from_secs(1));
        self.status_msg = format!(
            "[♬] mora {} pitch {:.1}",
            mora_idx, self.intonation_pitches[mora_idx]
        );
    }

    /// i: pitch値を入力開始時の初期値にリセットして再生する。
    pub async fn intonation_reset_to_initial(&mut self) {
        self.intonation_pitches = self.intonation_initial_pitches.clone();
        voicevox::set_mora_pitches(&mut self.intonation_query, &self.intonation_pitches);
        self.intonation_num_buf.clear();
        self.intonation_debounce = None;
        self.status_msg = String::from("[♬] pitch を初期値にリセット");
        self.play_with_intonation_query().await;
    }

    /// Space: 現在のintonation_queryで即時再生する（デバウンスなし）。
    pub async fn intonation_play_now(&mut self) {
        self.intonation_debounce = None;
        self.play_with_intonation_query().await;
    }

    /// デバウンス期限が過ぎていたら再生する（tui.rsのイベントループから呼ぶ）。
    pub async fn intonation_play_if_debounced(&mut self) {
        if let Some(until) = self.intonation_debounce {
            if Instant::now() >= until {
                self.intonation_debounce = None;
                self.play_with_intonation_query().await;
            }
        }
    }

    /// 現在のintonation_queryを使って再生する。
    /// キャッシュにWAVが存在する場合は即時再生し、ない場合はバックグラウンドで合成して再生する。
    pub(super) async fn play_with_intonation_query(&mut self) {
        let speaker_id = self.intonation_speaker_id;
        let source_text = self.lines.get(self.cursor)
            .map(|line| line.trim_start().to_owned())
            .unwrap_or_default();
        if let Some(cache_key) = Self::intonation_cache_key(speaker_id, &self.intonation_query) {
            let cached = { self.cache.lock().unwrap().get(&cache_key).cloned() };
            if let Some(wav) = cached {
                // in-flight の合成タスクがあれば abort して遅延再生を防ぐ
                if let Some(h) = self.intonation_play_handle.take() {
                    h.abort();
                }
                let _ = self.play_tx.send(PlayRequest {
                    wav,
                    source_text: source_text.clone(),
                }).await;
                self.status_msg = String::from("[♬ cached]");
                return;
            }
        }
        self.spawn_intonation_play(self.intonation_query.clone(), speaker_id, source_text);
    }
}
