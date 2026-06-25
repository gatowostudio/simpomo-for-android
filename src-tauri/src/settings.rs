//! アプリ設定の永続化（#5）。
//!
//! 設定の「真実」はアプリ設定ディレクトリの `settings.json`（ADR-0002）。起動時に読み込んで
//! タイマーに適用し、設定ビューが保存すると本ファイルを更新して反映する。
//! store プラグインは入れず、軽量に serde_json でファイル入出力する。
//!
//! Android 版ではデスクトップ専用フィールド（corner=ウィンドウ位置 / skip_taskbar=トレイ常駐）を
//! 削除した。全画面単一画面のため位置やタスクバーの概念が無い（ADR-0001）。
//!
//! 値の検証は Rust が権威（ADR-0002）: `sanitized()` が下限・上限・整合を保証する純粋関数で、
//! 保存前・適用前に必ず通す。フロントの clamp は UX 用であって信頼境界ではない。

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::timer::{Config, CycleSetting};

/// フェーズ時間の下限（1 分）・上限（180 分）。設定 UI も同じ範囲を提示する。
pub const MIN_PHASE_SECS: u32 = 60;
pub const MAX_PHASE_SECS: u32 = 180 * 60;
/// サイクル数の上限。
pub const MAX_CYCLES: u32 = 99;
/// 音量の上限（0〜100）。
pub const MAX_VOLUME: u8 = 100;
/// 背景色の既定（#rrggbb）。
pub const DEFAULT_FOCUS_BG: &str = "#1c1c1e";
pub const DEFAULT_BREAK_BG: &str = "#f0efe9";

/// `#rrggbb` 形式かどうか。
fn is_hex_color(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 7 && b[0] == b'#' && b[1..].iter().all(u8::is_ascii_hexdigit)
}

/// 不正な色文字列は既定色へ落とす。
fn sanitize_color(s: &str, default: &str) -> String {
    if is_hex_color(s) {
        s.to_string()
    } else {
        default.to_string()
    }
}

/// 通知音プリセット。再生はフロント（src/lib/sounds.ts、Web Audio 合成）が行い、Rust は識別子を持つだけ。
/// フロントの SoundId 型と serde lowercase で対応する手書きミラー。
/// variant を増やすときは src/lib/sounds.ts 冒頭の同期手順に従う（TS が正本）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SoundId {
    None,
    Beep,
    Chime,
    Ding,
    Blip,
    Fanfare,
}

/// フォーカス中に流す BGM プリセット。再生はフロント（src/lib/bgm.ts、Web Audio 合成）が行う。
/// フロントの BgmId 型と serde lowercase で対応する手書きミラー。
/// variant を増やすときは src/lib/bgm.ts 冒頭の同期手順に従う（TS が正本）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BgmId {
    None,
    White,
    Pink,
    Brown,
    Rain,
    Campfire,
}

/// 永続化するアプリ設定。フロント（src/lib/settings.ts）の手書きミラーと camelCase で対応する。
///
/// `#[serde(default)]`: 将来フィールドが増えても（#6 の通知音など）、旧 `settings.json` に
/// 欠けたフィールドは `Default` から補われる。これが無いと欠損フィールドで deserialize が失敗し、
/// `load` のフォールバックで全設定が既定に戻ってしまう。
// String フィールドを含むため Copy は付けない（Clone のみ）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    /// 作業フェーズ秒数（UI では分で扱う）。
    pub work_secs: u32,
    /// 休憩フェーズ秒数。
    pub break_secs: u32,
    /// 無限に繰り返すか。true のとき `cycles_count` は無視され、保存時 0 に正規化される。
    pub cycles_infinite: bool,
    /// 自動継続するセット数。0 = 1 セットで停止（既定）。`cycles_infinite` が false のとき有効。
    pub cycles_count: u32,
    /// 起動時に自動でタイマーを開始するか（#21。既定 false）。
    pub autostart_timer: bool,
    /// 作業フェーズ終了時の通知音。
    pub work_end_sound: SoundId,
    /// 休憩フェーズ終了時の通知音。
    pub break_end_sound: SoundId,
    /// セッション完走（N セット完了）時の通知音。
    pub session_end_sound: SoundId,
    /// 通知音の音量（0〜100）。
    pub volume: u8,
    /// バックグラウンドに退いている（不可視の）間、フェーズ境界を OS 通知でも知らせるか（#20）。
    /// 既定 false（オプトイン）。「邪魔にならない」方針に従い、欲しい人だけ設定で有効化する。
    pub os_notifications: bool,
    /// フォーカス中に流す BGM（休憩中は止める）。
    pub focus_bgm: BgmId,
    /// BGM の音量（0〜100）。
    pub bgm_volume: u8,
    /// 作業中の背景色（#rrggbb）。音が無くても色でフェーズが分かるようにする。
    pub focus_bg_color: String,
    /// 休憩中の背景色（#rrggbb）。
    pub break_bg_color: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        let c = Config::default();
        Self {
            work_secs: c.work_secs,
            break_secs: c.break_secs,
            cycles_infinite: false,
            cycles_count: 0,
            autostart_timer: false,
            work_end_sound: SoundId::Chime,
            break_end_sound: SoundId::Ding,
            session_end_sound: SoundId::Fanfare,
            volume: 70,
            os_notifications: false,
            focus_bgm: BgmId::None,
            bgm_volume: 25,
            focus_bg_color: DEFAULT_FOCUS_BG.to_string(),
            break_bg_color: DEFAULT_BREAK_BG.to_string(),
        }
    }
}

impl AppSettings {
    /// 値域を保証し整合を取った設定を返す（Rust が値検証の権威）。
    /// 保存前・適用前に必ず通す。フロントを信頼しないための不変条件。
    pub fn sanitized(self) -> Self {
        Self {
            work_secs: self.work_secs.clamp(MIN_PHASE_SECS, MAX_PHASE_SECS),
            break_secs: self.break_secs.clamp(MIN_PHASE_SECS, MAX_PHASE_SECS),
            // 無限のときは回数を 0 に正規化し、無意味な値を永続化しない。
            cycles_count: if self.cycles_infinite {
                0
            } else {
                self.cycles_count.min(MAX_CYCLES)
            },
            volume: self.volume.min(MAX_VOLUME),
            bgm_volume: self.bgm_volume.min(MAX_VOLUME),
            // 不正な色（#rrggbb 以外）は既定色へ戻す（Rust が値検証の権威）。
            focus_bg_color: sanitize_color(&self.focus_bg_color, DEFAULT_FOCUS_BG),
            break_bg_color: sanitize_color(&self.break_bg_color, DEFAULT_BREAK_BG),
            // 上記以外（各 bool トグル・各 sound・focus_bgm 等、値域の概念が無いもの）は素通し。
            // ここを列挙台帳にすると新フィールド追加のたびにドリフトするので、性質で説明する。
            ..self
        }
    }

    /// タイマーのコア設定へ変換する。
    pub fn to_config(&self) -> Config {
        Config {
            work_secs: self.work_secs,
            break_secs: self.break_secs,
            cycles: if self.cycles_infinite {
                CycleSetting::Infinite
            } else {
                CycleSetting::Finite(self.cycles_count)
            },
        }
    }
}

fn settings_path(app: &AppHandle) -> tauri::Result<PathBuf> {
    Ok(app.path().app_config_dir()?.join("settings.json"))
}

/// 設定を読み込む。ファイルが無い / 壊れている場合は既定値を返す（起動を止めない）。
pub fn load(app: &AppHandle) -> AppSettings {
    settings_path(app)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 設定を `settings.json` に保存する。
pub fn save(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let path = settings_path(app).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_25_5_and_finite_zero() {
        let s = AppSettings::default();
        assert_eq!(s.work_secs, 1500);
        assert_eq!(s.break_secs, 300);
        assert!(!s.cycles_infinite);
        assert_eq!(s.cycles_count, 0);
        assert_eq!(s.focus_bgm, BgmId::None);
    }

    #[test]
    fn to_config_maps_finite_cycles() {
        let s = AppSettings {
            cycles_infinite: false,
            cycles_count: 4,
            ..Default::default()
        };
        assert_eq!(s.to_config().cycles, CycleSetting::Finite(4));
    }

    #[test]
    fn to_config_infinite_ignores_count() {
        let s = AppSettings {
            cycles_infinite: true,
            cycles_count: 9,
            ..Default::default()
        };
        assert_eq!(s.to_config().cycles, CycleSetting::Infinite);
    }

    #[test]
    fn sanitized_clamps_phase_and_normalizes_infinite() {
        let s = AppSettings {
            work_secs: 0,
            break_secs: 999_999,
            cycles_infinite: true,
            cycles_count: 50,
            volume: 250,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(s.work_secs, MIN_PHASE_SECS);
        assert_eq!(s.break_secs, MAX_PHASE_SECS);
        assert_eq!(s.cycles_count, 0); // 無限のとき 0 に正規化
        assert_eq!(s.volume, MAX_VOLUME); // 音量は 100 にクランプ
    }

    #[test]
    fn sanitized_fixes_invalid_colors() {
        let s = AppSettings {
            focus_bg_color: "not-a-color".to_string(),
            break_bg_color: "#abcdef".to_string(),
            ..Default::default()
        }
        .sanitized();
        assert_eq!(s.focus_bg_color, DEFAULT_FOCUS_BG); // 不正→既定
        assert_eq!(s.break_bg_color, "#abcdef"); // 正しい hex は保持
    }

    #[test]
    fn sanitized_caps_cycles_count() {
        let s = AppSettings {
            cycles_infinite: false,
            cycles_count: 1000,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(s.cycles_count, MAX_CYCLES);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults_not_full_reset() {
        // #6 でフィールドが増えても、旧 JSON(一部欠損)が全既定化しないことを固定する。
        let json = r#"{"workSecs": 1800}"#;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.work_secs, 1800); // 指定値は保持
        assert_eq!(s.break_secs, 300); // 欠損は既定
        assert_eq!(s.focus_bg_color, "#1c1c1e"); // 欠損は既定
    }

    #[test]
    fn json_round_trips_with_camel_case() {
        // フロント settings.ts との camelCase 契約を固定する。
        let s = AppSettings {
            work_secs: 1800,
            break_secs: 600,
            cycles_infinite: true,
            cycles_count: 0,
            autostart_timer: true,
            work_end_sound: SoundId::Beep,
            break_end_sound: SoundId::None,
            session_end_sound: SoundId::Fanfare,
            volume: 55,
            os_notifications: false,
            focus_bgm: BgmId::Rain,
            bgm_volume: 35,
            focus_bg_color: "#000000".to_string(),
            break_bg_color: "#ffffff".to_string(),
        };
        let json = serde_json::to_string(&s).unwrap();
        for key in [
            "workSecs",
            "breakSecs",
            "cyclesInfinite",
            "cyclesCount",
            "autostartTimer",
            "workEndSound",
            "breakEndSound",
            "sessionEndSound",
            "volume",
            "osNotifications",
            "focusBgm",
            "bgmVolume",
            "focusBgColor",
            "breakBgColor",
        ] {
            assert!(json.contains(&format!("\"{key}\"")), "missing key: {key}");
        }
        assert!(json.contains("\"beep\"")); // SoundId は lowercase
        assert!(json.contains("\"rain\"")); // BgmId は lowercase
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }
}
