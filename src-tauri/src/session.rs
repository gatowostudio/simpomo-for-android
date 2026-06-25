//! 走行中セッションの永続化（ADR-0002 §3、deadline 永続化方式 / option B）。
//!
//! Android はアプリがバックグラウンドへ退くとプロセス/スレッドの実行を絞り、kill しうる。
//! `timer.rs` の状態はメモリ上にしか無いため、何もしないとプロセスが落ちた瞬間に走行中セッションが
//! 失われる（「裏に回しても時間が進む」というポモドーロの中核が壊れる）。
//!
//! そこで「タイマーの実行状態（`TimerState`）」と「その状態を記録した壁時計時刻（anchor）」を
//! `session.json` に保存する。再起動時に `now - anchor` の経過秒を 1 回の `tick` として与えれば、
//! プロセスが落ちていた間の経過も含めて状態を正しく復元できる（lib.rs: apply_loaded_settings）。
//!
//! anchor と remaining は**記録した瞬間の対**として保存する（同時刻に両方を採る）。前景の tick ループ
//! （`lib.rs: spawn_tick_loop`）も**同じ壁時計**で remaining を減らすため、メモリ上の現在値と
//! 「最後に保存した対 (remaining, anchor) + 壁時計差分」は同一系統で一致する。よって毎秒保存せずとも
//! 復元時に現在値を再構成でき、保存はコマンド時とフェーズ境界時のみで足りる（フラッシュ書込みを抑える）。
//! ※デスクトップ版は前景を `Instant`（単調時計）で駆動していたが、Android では `Instant` がサスペンド中に
//!   止まり永続 anchor（壁時計）とズレるため、前景・復元の両方を壁時計に統一した（ADR-0002 §3）。

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::timer::TimerState;

/// 永続化する走行セッション。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedSession {
    /// タイマーの実行状態（記録時点の対）。
    pub state: TimerState,
    /// この状態を記録した時点の壁時計（UNIX 秒）。復帰時に `now - anchor` で経過秒を求める。
    pub anchor_unix_secs: u64,
}

/// 現在の壁時計（UNIX 秒）。取得できない異常時は 0。0 は保存側（`persist_session`）で弾くため、
/// 復元時に anchor=0 が「巨大経過」へ化けることはない（経過計算は timer::restore_for_launch が saturating で行う）。
pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn session_path(app: &AppHandle) -> tauri::Result<PathBuf> {
    Ok(app.path().app_config_dir()?.join("session.json"))
}

/// 走行セッションを読み込む。無い / 壊れている場合は `None`（フォールバックで通常起動する）。
pub fn load(app: &AppHandle) -> Option<PersistedSession> {
    session_path(app)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
}

/// 走行セッションを `session.json` に保存する。
pub fn save(app: &AppHandle, session: &PersistedSession) -> Result<(), String> {
    let path = session_path(app).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(session).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timer::{Phase, Status};

    #[test]
    fn persisted_session_round_trips_json() {
        let s = PersistedSession {
            state: TimerState {
                phase: Phase::Break,
                status: Status::Running,
                remaining_secs: 120,
                set_index: 2,
            },
            anchor_unix_secs: 1_700_000_000,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"anchorUnixSecs\""));
        assert!(json.contains("\"remainingSecs\""));
        let back: PersistedSession = serde_json::from_str(&json).unwrap();
        assert_eq!(back.state, s.state);
        assert_eq!(back.anchor_unix_secs, s.anchor_unix_secs);
    }
}
