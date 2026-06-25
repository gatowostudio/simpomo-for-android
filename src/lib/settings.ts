// 設定の永続化（#5）への薄いブリッジ。
//
// 型の同期契約: 下記の型・enum 文字列は src-tauri/src/settings.rs（AppSettings）の手書きミラー。
// Rust 側のフィールド名・serde rename（camelCase / lowercase）・enum の variant を変えたら本ファイルも
// 必ず同期すること。ドリフトは Rust 側テスト（settings.rs: json_round_trips / missing_fields_*）が部分的に検出する。
// ※ SoundId は sounds.ts、BgmId は bgm.ts を TS 側の正本とし、ここは re-export（追加手順は各ファイル参照）。
//
// Android 版: デスクトップ専用の corner（ウィンドウ位置）/ skipTaskbar（トレイ常駐）/ openSettings
// （別ウィンドウ表示）は削除した。設定は単一画面内のビューで開く（ADR-0001）。
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { SoundId } from "./sounds";
import type { BgmId } from "./bgm";

export type { SoundId, BgmId };

/** 設定変更イベント名。Rust(lib.rs EVENT_SETTINGS) と一致させる。 */
const EVENT_SETTINGS_CHANGED = "settings-changed";

// 時間換算・値域の定数（Rust settings.rs の MIN/MAX_PHASE_SECS, MAX_CYCLES と対応させる）。
export const SECS_PER_MINUTE = 60;
export const MIN_PHASE_MIN = 1;
export const MAX_PHASE_MIN = 180;
export const MAX_CYCLES = 99;

export const minutesToSecs = (min: number): number =>
  Math.round(min) * SECS_PER_MINUTE;
export const secsToMinutes = (secs: number): number =>
  Math.round(secs / SECS_PER_MINUTE);

export interface AppSettings {
  workSecs: number;
  breakSecs: number;
  cyclesInfinite: boolean;
  /** 0 = 1 セットで停止（既定）。cyclesInfinite が false のとき有効。 */
  cyclesCount: number;
  /** 起動時に自動でタイマーを開始するか（#21。既定 false）。 */
  autostartTimer: boolean;
  workEndSound: SoundId;
  breakEndSound: SoundId;
  sessionEndSound: SoundId;
  /** 通知音の音量（0〜100）。 */
  volume: number;
  /** フェーズ境界を OS 通知でも知らせるか（#20。既定 false）。 */
  osNotifications: boolean;
  /** フォーカス中に流す BGM（休憩中は止まる）。 */
  focusBgm: BgmId;
  /** BGM の音量（0〜100）。 */
  bgmVolume: number;
  /** 作業中の背景色（#rrggbb）。 */
  focusBgColor: string;
  /** 休憩中の背景色（#rrggbb）。 */
  breakBgColor: string;
}

export const getSettings = (): Promise<AppSettings> => invoke("get_settings");

export const saveSettings = (settings: AppSettings): Promise<void> =>
  invoke("save_settings", { settings });

/** 設定変更の通知を購読する（#6 の通知音などが使う）。 */
export const onSettingsChanged = (
  cb: (settings: AppSettings) => void,
): Promise<UnlistenFn> =>
  listen<AppSettings>(EVENT_SETTINGS_CHANGED, (e) => cb(e.payload));

// ラベルは Record で全 variant の網羅を型強制する（enum に variant を足したら
// ここがコンパイルエラーになり、同期漏れを防ぐ）。表示順は定義順。
const SOUND_LABELS: Record<SoundId, string> = {
  none: "None",
  beep: "Beep",
  chime: "Chime",
  ding: "Ding",
  blip: "Blip",
  fanfare: "Fanfare",
};

const BGM_LABELS: Record<BgmId, string> = {
  none: "None",
  white: "White noise",
  pink: "Pink noise",
  brown: "Brown noise",
  rain: "Rain",
  campfire: "Campfire",
};

/** 音量の上限（Rust settings.rs MAX_VOLUME と対応）。 */
export const MAX_VOLUME = 100;

/** 背景色の既定（Rust settings.rs DEFAULT_FOCUS_BG / DEFAULT_BREAK_BG と一致させる）。 */
export const DEFAULT_FOCUS_BG = "#1c1c1e";
export const DEFAULT_BREAK_BG = "#f0efe9";

export const SOUND_OPTIONS = (Object.keys(SOUND_LABELS) as SoundId[]).map(
  (value) => ({ value, label: SOUND_LABELS[value] }),
);
export const BGM_OPTIONS = (Object.keys(BGM_LABELS) as BgmId[]).map((value) => ({
  value,
  label: BGM_LABELS[value],
}));
