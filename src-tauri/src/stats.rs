//! 完了数の簡易統計（#22）。完了したフォーカス（ポモドーロ）数と完了セット数を集計・永続化する。
//!
//! 設定とは性質が違う（ユーザーが編集する値ではなく、タイマー進行に伴い自動で増える記録）ので、
//! `settings.json` とは別ファイル `stats.json` に保存する。真実は Rust が持つ（ADR-0002）。
//! 集計の契機はタイマーのフェーズ境界イベント（timer.rs）。手動 skip はイベントを出さないため
//! 数えない（自分で飛ばしたぶんは実績に入らない、という自然な挙動）。

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::timer::TimerEvent;

/// 永続化する完了数。フロント（src/lib/stats.ts）の手書きミラーと camelCase で対応する。
/// `#[serde(default)]`: 将来フィールドが増えても旧 `stats.json` の欠損は 0 で補う。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Stats {
    /// 完了したフォーカス（作業）フェーズ数＝ポモドーロ数。
    pub completed_focus: u32,
    /// 完了したセット数（作業→休憩を 1 セットとして、休憩まで終えた回数）。
    pub completed_sets: u32,
}

impl Stats {
    /// フェーズ境界イベント列を集計へ反映する。変化があれば true を返す（保存/通知の要否判定用）。
    /// スリープ復帰などで複数境界が一度に来ても、各イベントを正しく数える。
    pub fn record(&mut self, events: &[TimerEvent]) -> bool {
        let mut changed = false;
        for event in events {
            match event {
                TimerEvent::WorkEnded => {
                    self.completed_focus = self.completed_focus.saturating_add(1);
                    changed = true;
                }
                TimerEvent::BreakEnded => {
                    self.completed_sets = self.completed_sets.saturating_add(1);
                    changed = true;
                }
                // セッション完了は「最後のセット完了（BreakEnded）」に内包されるので二重に数えない。
                TimerEvent::SessionFinished => {}
            }
        }
        changed
    }
}

fn stats_path(app: &AppHandle) -> tauri::Result<PathBuf> {
    Ok(app.path().app_config_dir()?.join("stats.json"))
}

/// 統計を読み込む。ファイルが無い / 壊れている場合は 0 から（起動を止めない）。
pub fn load(app: &AppHandle) -> Stats {
    stats_path(app)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 統計を `stats.json` に保存する。
pub fn save(app: &AppHandle, stats: &Stats) -> Result<(), String> {
    let path = stats_path(app).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(stats).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timer::TimerEvent::{BreakEnded, SessionFinished, WorkEnded};

    #[test]
    fn record_counts_focus_and_sets() {
        let mut s = Stats::default();
        assert!(s.record(&[WorkEnded]));
        assert_eq!(s.completed_focus, 1);
        assert_eq!(s.completed_sets, 0);
        assert!(s.record(&[BreakEnded]));
        assert_eq!(s.completed_focus, 1);
        assert_eq!(s.completed_sets, 1);
    }

    #[test]
    fn record_handles_catch_up_batch() {
        // スリープ復帰で複数境界が一度に来ても各イベントを数える。
        let mut s = Stats::default();
        s.record(&[WorkEnded, BreakEnded, WorkEnded, BreakEnded, SessionFinished]);
        assert_eq!(s.completed_focus, 2);
        assert_eq!(s.completed_sets, 2);
    }

    #[test]
    fn session_finished_alone_does_not_count() {
        let mut s = Stats::default();
        assert!(!s.record(&[SessionFinished]));
        assert_eq!(s, Stats::default());
    }

    #[test]
    fn record_empty_is_noop() {
        let mut s = Stats::default();
        assert!(!s.record(&[]));
        assert_eq!(s, Stats::default());
    }

    #[test]
    fn round_trips_with_camel_case() {
        // フロント stats.ts との camelCase 契約を固定する。
        let s = Stats {
            completed_focus: 12,
            completed_sets: 9,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"completedFocus\""));
        assert!(json.contains("\"completedSets\""));
        let back: Stats = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn missing_fields_default_to_zero() {
        let s: Stats = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Stats::default());
    }

    #[test]
    fn saturates_instead_of_overflowing() {
        // saturating_add の意図を固定（到達しない値だが、増分でパニックしない）。
        let mut s = Stats {
            completed_focus: u32::MAX,
            completed_sets: u32::MAX,
        };
        assert!(s.record(&[WorkEnded, BreakEnded]));
        assert_eq!(s.completed_focus, u32::MAX);
        assert_eq!(s.completed_sets, u32::MAX);
    }

    #[test]
    fn counts_match_timer_real_output() {
        // record が依存する timer の出力契約（イベント順・session は末尾）を、Timer を実走させて結合検証する。
        // 不変条件: 各セットで focus と sets が 1 ずつ増え、focus >= sets が常に成立する。
        use crate::timer::{Config, CycleSetting, Timer};
        let mut t = Timer::new(Config {
            work_secs: 2,
            break_secs: 2,
            cycles: CycleSetting::Finite(3),
        });
        t.start();
        let mut s = Stats::default();
        // 3 セット（作業2秒→休憩2秒）を秒進行で消化する。
        for _ in 0..12 {
            s.record(&t.tick(1));
            assert!(s.completed_focus >= s.completed_sets, "focus >= sets 不変条件");
        }
        assert_eq!(s.completed_focus, 3);
        assert_eq!(s.completed_sets, 3);
    }
}
