// 完了数の簡易統計（#22）への薄いブリッジ。集計と永続化は Rust（stats.rs）が権威。
//
// 型の同期契約: 下記 Stats は src-tauri/src/stats.rs の Stats（serde camelCase）の手書きミラー。
// Rust 側のフィールド名や rename を変えたら本ファイルも同期すること
// （stats.rs の round_trips_with_camel_case テストがキー名のドリフトを検出する）。
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Rust(lib.rs EVENT_STATS) と一致させる文字列契約。
const EVENT_STATS_CHANGED = "stats-changed";

/** Rust の Stats に対応する完了数。 */
export interface Stats {
  /** 完了したフォーカス（作業）フェーズ数＝ポモドーロ数。 */
  completedFocus: number;
  /** 完了したセット数（作業→休憩を 1 セットとして数えた回数）。 */
  completedSets: number;
}

export const getStats = (): Promise<Stats> => invoke("get_stats");

/** 統計を 0 に戻す（設定ウィンドウの Reset から）。 */
export const resetStats = (): Promise<void> => invoke("reset_stats");

/** 統計更新（境界到達 / リセット）を購読する。 */
export const onStatsChanged = (cb: (stats: Stats) => void): Promise<UnlistenFn> =>
  listen<Stats>(EVENT_STATS_CHANGED, (e) => cb(e.payload));
