pub mod session;
pub mod settings;
pub mod stats;
pub mod timer;

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter, State};

use settings::AppSettings;
use stats::Stats;

use timer::{Config, Status, Timer, TimerEvent, TimerSnapshot};

/// タイマー状態と、tick 駆動スレッドを起こすための条件変数、完了数の統計をまとめた共有状態。
///
/// 並行性の不変条件: 各 Mutex を保持している間は panic しうる処理を呼ばない
/// （Timer/Stats 操作は純粋で panic せず、save はエラーをログして握りつぶす）。よって Mutex は
/// poison せず、`lock().unwrap()` は安全。`timer` と `stats` を**同時に保持する経路は無い**
/// （tick スレッドは stats を触る前に timer ロックを手放す）ので、ロック順による deadlock は生じない。
struct Shared {
    timer: Mutex<Timer>,
    /// 稼働状態の変化を tick スレッドへ通知する。start で park 中のスレッドを起こす。
    wake: Condvar,
    /// 完了数の簡易統計（#22）。tick の境界イベントで増え、stats.json に永続化する。
    stats: Mutex<Stats>,
}

type SharedState = Arc<Shared>;

// イベント名はフロント（timer.ts / settings.ts）と一致させる文字列契約。
// 単純な英数字 + ハイフンにする（":" や "/" を含む名前はトラブルの元になりうる）。
const EVENT_SNAPSHOT: &str = "timer-snapshot";
const EVENT_TIMER_EVENTS: &str = "timer-events";
/// 設定変更を通知する（snapshot に乗らない設定＝通知音/BGM/背景色を App が購読して反映する）。
const EVENT_SETTINGS: &str = "settings-changed";
/// 完了数の統計更新を通知する（#22。設定ビューが購読してライブ表示する）。
const EVENT_STATS: &str = "stats-changed";

// snapshot / フェーズ境界イベントはブロードキャストで送る（フロントの listen() に確実に届く）。
fn emit_snapshot(app: &AppHandle, snapshot: TimerSnapshot) {
    // 送信失敗（アプリ終了時などリスナー不在のタイミング）は致命ではないので握りつぶす。
    let _ = app.emit(EVENT_SNAPSHOT, snapshot);
}

fn emit_events(app: &AppHandle, events: &[TimerEvent]) {
    if !events.is_empty() {
        let _ = app.emit(EVENT_TIMER_EVENTS, events);
    }
}

/// 走行セッションを永続化する（ADR-0002 §3 / deadline 永続化）。記録時点の実行状態と壁時計 anchor の
/// 対を保存し、プロセスが kill/Doze で落ちても再起動時にフェーズ進行を復元できるようにする。
/// timer ロックを保持したまま呼ぶ（記録する状態と anchor の対を同時刻で確定するため）。
/// 保存失敗（ストレージ満杯/権限等）は致命ではないのでログに留める。
fn persist_session(app: &AppHandle, timer: &Timer) {
    let anchor = session::now_unix_secs();
    // 壁時計取得失敗（anchor=0）時は保存しない。anchor=0 を書くと、復元時に `now - 0` が
    // 巨大な経過に化けてセッションが勝手に飛ぶため（過小 anchor → 過大 elapsed の反転を断つ）。
    if anchor == 0 {
        return;
    }
    let session = session::PersistedSession {
        state: timer.state(),
        anchor_unix_secs: anchor,
    };
    if let Err(e) = session::save(app, &session) {
        eprintln!("simpomo: failed to persist session: {e}");
    }
}

/// フェーズ境界イベントを完了数の統計へ反映し、変化があれば永続化して通知する（#22）。
/// `timer` ロックを手放した状態で呼ぶ（呼び出し側が drop 済み）。`stats` ロックは集計の一瞬だけ
/// 保持してコピーを取り、ファイル I/O と emit はロック外で行う（ボタン操作を I/O で待たせない）。
fn record_stats(app: &AppHandle, state: &SharedState, events: &[TimerEvent]) {
    let updated = {
        let mut stats = state.stats.lock().unwrap();
        stats.record(events).then_some(*stats)
    };
    if let Some(stats) = updated {
        // 保存失敗（ストレージ満杯/権限等）は致命ではないが、表示と永続が乖離するのでログは残す。
        if let Err(e) = stats::save(app, &stats) {
            eprintln!("simpomo: failed to persist stats: {e}");
        }
        let _ = app.emit(EVENT_STATS, stats);
    }
}

// コマンドは状態を「戻り値」では返さない。更新は必ず emit(EVENT_SNAPSHOT) の単一経路で流す
// （戻り値と emit の二重経路だと到着順でフロント表示が巻き戻るレースが起きるため）。
// mutate と emit は同一ロック区間で行い、tick スレッドの emit と順序が入れ替わらないようにする。

#[tauri::command]
fn timer_snapshot(state: State<'_, SharedState>) -> TimerSnapshot {
    state.timer.lock().unwrap().snapshot()
}

#[tauri::command]
fn timer_start(app: AppHandle, state: State<'_, SharedState>) {
    {
        let mut timer = state.timer.lock().unwrap();
        timer.start();
        emit_snapshot(&app, timer.snapshot());
        persist_session(&app, &timer);
    }
    state.wake.notify_all(); // park 中の tick スレッドを起こす
}

#[tauri::command]
fn timer_pause(app: AppHandle, state: State<'_, SharedState>) {
    {
        let mut timer = state.timer.lock().unwrap();
        timer.pause();
        emit_snapshot(&app, timer.snapshot());
        persist_session(&app, &timer);
    }
    state.wake.notify_all();
}

#[tauri::command]
fn timer_reset(app: AppHandle, state: State<'_, SharedState>) {
    {
        let mut timer = state.timer.lock().unwrap();
        timer.reset();
        emit_snapshot(&app, timer.snapshot());
        persist_session(&app, &timer);
    }
    state.wake.notify_all();
}

#[tauri::command]
fn timer_skip(app: AppHandle, state: State<'_, SharedState>) {
    {
        let mut timer = state.timer.lock().unwrap();
        // 手動 skip はフェーズ境界イベントを出さない（自分で送ったのに通知音が鳴る違和感を避ける）。
        // 状態変化は snapshot で反映する。時間切れの遷移（tick）だけが通知音を鳴らす。
        let _ = timer.skip();
        emit_snapshot(&app, timer.snapshot());
        persist_session(&app, &timer);
    }
    state.wake.notify_all();
}

/// 現在の永続化設定を返す（設定ビューの初期表示用）。
#[tauri::command]
fn get_settings(app: AppHandle) -> AppSettings {
    settings::load(&app)
}

/// 完了数の統計を返す（設定ビューの表示用、#22）。
#[tauri::command]
fn get_stats(state: State<'_, SharedState>) -> Stats {
    *state.stats.lock().unwrap()
}

/// 完了数の統計を 0 に戻す（設定ビューの Reset から。ユーザー操作で実行、#22）。
#[tauri::command]
fn reset_stats(app: AppHandle, state: State<'_, SharedState>) -> Result<(), String> {
    let stats = {
        let mut s = state.stats.lock().unwrap();
        *s = Stats::default();
        *s
    };
    stats::save(&app, &stats)?;
    let _ = app.emit(EVENT_STATS, stats);
    Ok(())
}

/// 設定を保存し、タイマーへ反映する（設定ビューから呼ぶ）。
///
/// デスクトップ版にあったウィンドウ位置（corner）/タスクバー（skip_taskbar）の適用は Android では
/// 不要なため削除した。設定値の検証は Rust が権威（保存値=実効値になるよう先に正規化する）。
#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<'_, SharedState>,
    settings: AppSettings,
) -> Result<(), String> {
    let settings = settings.sanitized();
    settings::save(&app, &settings)?;
    // タイマーへ反映（Idle なら新時間で reset、稼働中はセッション維持し次フェーズ以降）。
    {
        let mut timer = state.timer.lock().unwrap();
        timer.set_config(settings.to_config());
        emit_snapshot(&app, timer.snapshot());
        // set_config は Idle のとき reset しうる（remaining が変わる）ので、対を更新して永続化する。
        persist_session(&app, &timer);
    }
    state.wake.notify_all();
    // 設定変更を通知（snapshot に乗らない設定＝通知音/BGM/背景色を App が購読して反映）。
    let _ = app.emit(EVENT_SETTINGS, settings);
    Ok(())
}

/// 起動時に永続化設定・統計・走行セッションを読み込み、タイマーへ適用する。
fn apply_loaded_settings(app: &AppHandle, state: &SharedState) {
    let loaded = settings::load(app);
    // 完了数の統計を stats.json から読み直す（#22）。
    *state.stats.lock().unwrap() = stats::load(app);
    let config = loaded.to_config();
    // 前回の走行セッション（ADR-0002 §3 / deadline 永続化）。Running は壁時計で catch-up、
    // Paused は復元、それ以外は autostart 判定。分岐の核は timer::restore_for_launch（純粋・テスト済み）。
    let persisted = session::load(app).map(|p| (p.state, p.anchor_unix_secs));
    let now = session::now_unix_secs();
    {
        let mut timer = state.timer.lock().unwrap();
        *timer = Timer::restore_for_launch(persisted, config, loaded.autostart_timer, now);
        // 復元/初期化後の状態を、新しい anchor で再永続化して以降の整合を保つ。
        persist_session(app, &timer);
    }
}

/// tick の間隔は通常 1 秒（下記ループの wait_timeout）。これを大きく超える経過は
/// バックグラウンド復帰（プロセスは生きていたが実行を絞られていた）を意味し、その間ユーザーは
/// 作業していない。この値を超える catch-up バッチ（=非ライブ）は:
/// - 統計(#22)に数えない（水増し防止）
/// - **境界の通知音/通知も emit しない**（#4。数分前に過ぎた境界の音を復帰時に鳴らさない。
///   kill/Doze 復帰の restore 側が境界イベントを破棄するのと挙動を統一する）
/// 状態(snapshot)は常に最新へ更新する（残り時間・フェーズは正しく追いつく）。
/// 値はモバイルの前景ジャンク（画面回転・GC で数秒スタックしうる）を吸収しつつ、分単位の
/// バックグラウンド不在は除外する妥協点。
const MAX_LIVE_GAP_SECS: u32 = 15;

/// この tick の経過が「ライブ」（通常の毎秒進行）か、バックグラウンド一括 catch-up かを判定する。
/// 純粋関数として切り出し、閾値契約（`<=` か `<` か・値）の取り違えを単体テストで固定する（#4）。
fn is_live_gap(elapsed_secs: u32) -> bool {
    elapsed_secs <= MAX_LIVE_GAP_SECS
}

/// 実時間の駆動（ADR-0002 §3）。Rust 側のバックグラウンドスレッドが **壁時計**（UNIX 秒）の差分で
/// 実経過秒を算出して `tick` に渡す。デスクトップ版は `Instant`（単調時計）だったが、Android では
/// `Instant`(CLOCK_MONOTONIC) がサスペンド中に止まりバックグラウンド/画面オフの経過を取りこぼす。
/// 永続化 anchor と同じ壁時計に統一することで、前景駆動と復元（restore catch-up）が一貫し、
/// バックグラウンドから復帰した（kill されていない）プロセスもその場で経過を取り戻せる。
/// 代償として時刻変更/NTP 前方ジャンプは経過に乗る（巻き戻りは saturating で 0）。
///
/// アイドル時 CPU を最小化するため、非稼働中（Idle/Paused）は条件変数で park し、
/// CPU を消費しない。start の `notify_all` で起き、稼働中のみ毎秒 tick して emit する。
fn spawn_tick_loop(app: AppHandle, state: SharedState) {
    std::thread::spawn(move || {
        let mut last_unix = session::now_unix_secs();
        let mut timer = state.timer.lock().unwrap();
        loop {
            // 非稼働中は wake されるまで park（この間ロックは解放され CPU はゼロ）。
            while timer.snapshot().status != Status::Running {
                timer = state.wake.wait(timer).unwrap();
                // 稼働再開の起点を壁時計で切り直す（park 中の経過を取り込んで過剰進行するのを防ぐ）。
                last_unix = session::now_unix_secs();
            }
            // 稼働中: 最大 1 秒待つ。pause/skip 等の通知で早く起きる。
            let (guard, _timeout) = state
                .wake
                .wait_timeout(timer, Duration::from_secs(1))
                .unwrap();
            timer = guard;
            if timer.snapshot().status != Status::Running {
                continue; // 待機中に停止された → park へ戻る
            }
            let now_unix = session::now_unix_secs();
            // 壁時計差分。巻き戻り（NTP/手動）は 0、巨大ジャンプ（バックグラウンド復帰等）は u32 飽和。
            // 端数は秒精度のため自然に次回へ持ち越る（elapsed=0 のとき last_unix を進めない）。
            let elapsed = now_unix.saturating_sub(last_unix).min(u32::MAX as u64) as u32;
            if elapsed == 0 {
                continue;
            }
            last_unix = now_unix;
            let events = timer.tick(elapsed);
            let snapshot = timer.snapshot();
            // ライブ tick（通常 1 秒前後）か、バックグラウンド復帰の一括 catch-up かで境界イベントの
            // 扱いを分ける（#4）。非ライブの境界は数分前に過ぎたものなので、音/通知も統計も出さない
            // （kill/Doze 復帰の restore 抑制と統一）。状態(snapshot)はどちらでも最新へ更新する。
            let live = is_live_gap(elapsed);
            // ロック保持中に emit し、コマンド由来の emit と順序が入れ替わらないようにする。
            if live {
                emit_events(&app, &events);
            }
            emit_snapshot(&app, snapshot);
            // フェーズ境界で remaining が新フェーズ長に切り替わった対を永続化する（ADR-0002 §3）。
            // 境界間の通常 tick では保存しない（最後に保存した対 + 壁時計差分で現在値を再構成できるため）。
            // ※非ライブ catch-up でも境界を跨いだなら状態は変わるので保存する（音は出さないが状態は確定）。
            if !events.is_empty() {
                persist_session(&app, &timer);
            }
            // 完了数の集計（#22）。境界を跨いだライブ tick のときだけ。復帰の巨大な
            // catch-up は実際に作業していないので実績に数えない。
            // 保存(I/O)は timer ロックを手放してから行い、ボタン操作を待たせない。
            if live && !events.is_empty() {
                drop(timer);
                record_stats(&app, &state, &events);
                timer = state.timer.lock().unwrap();
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state: SharedState = Arc::new(Shared {
        timer: Mutex::new(Timer::new(Config::default())),
        wake: Condvar::new(),
        // 起動時に stats.json から読み直す（apply_loaded_settings）。
        stats: Mutex::new(Stats::default()),
    });

    tauri::Builder::default()
        // 「更新を確認」でリリースページを既定ブラウザで開く。
        .plugin(tauri_plugin_opener::init())
        // フェーズ境界を Android 通知で知らせる（#20。送信はフロントから行う）。
        .plugin(tauri_plugin_notification::init())
        .manage(state.clone())
        .setup(move |app| {
            let handle = app.handle();
            apply_loaded_settings(handle, &state);
            spawn_tick_loop(handle.clone(), state.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            timer_snapshot,
            timer_start,
            timer_pause,
            timer_reset,
            timer_skip,
            get_settings,
            save_settings,
            get_stats,
            reset_stats
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_gap_threshold() {
        // ライブ（毎秒進行・前景ジャンク）: 閾値以下は境界 emit/統計を通す。
        assert!(is_live_gap(0));
        assert!(is_live_gap(MAX_LIVE_GAP_SECS));
        // 非ライブ（バックグラウンド一括 catch-up）: 閾値超は境界 emit/統計を抑制する（#4）。
        assert!(!is_live_gap(MAX_LIVE_GAP_SECS + 1));
    }
}
