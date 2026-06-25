// Rust コア（タイマー状態機械）への薄いブリッジ（ADR-0002: フロントは表示層）。
// 操作は invoke で送り、状態は timer-snapshot イベントの単一経路で受け取る。tick の駆動は Rust 側。
//
// 型の同期契約: 下記の型は src-tauri/src/timer.rs の手書きミラー。Rust 側のフィールド名や
// serde rename（camelCase）を変えたら本ファイルも必ず同期すること。型生成（ts-rs/tauri-specta 等）は
// 現状未導入で手動同期。Rust 側の snapshot_serializes_to_camel_case_keys テストがキー名のドリフトを検出する。
// （切替の目安: 手動同期する型が増えて辛くなったら tauri-specta 導入を再検討。）
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// イベント名・コマンド名は1箇所に集約（タイポは静かに購読/呼び出し失敗になるため）。
// Rust(lib.rs) と一致させる文字列契約。
const EVENT_SNAPSHOT = "timer-snapshot";
const EVENT_TIMER_EVENTS = "timer-events";

export type Phase = "work" | "break";
export type Status = "idle" | "running" | "paused";

/** Rust の TimerSnapshot（serde camelCase）に対応する表示用スナップショット。 */
export interface TimerSnapshot {
  phase: Phase;
  status: Status;
  remainingSecs: number;
  /** 0 始まりの現在セット番号。表示時は +1 する。 */
  setIndex: number;
  /** 走らせるセット総数。無限のときは null。 */
  totalSets: number | null;
  workSecs: number;
  breakSecs: number;
}

/** フェーズ境界の出来事（#6 通知音で使う）。 */
export type TimerEvent = "workEnded" | "breakEnded" | "sessionFinished";

// 操作コマンドは状態を返さない。更新は onSnapshot（emit）の単一経路で受け取る。
export const start = (): Promise<void> => invoke("timer_start");
export const pause = (): Promise<void> => invoke("timer_pause");
export const reset = (): Promise<void> => invoke("timer_reset");
export const skip = (): Promise<void> => invoke("timer_skip");

/** 初期表示用に現在のスナップショットを取得する（純粋なクエリ、emit しない）。 */
export const getSnapshot = (): Promise<TimerSnapshot> => invoke("timer_snapshot");

/** 状態更新（稼働中は毎秒 / 操作時）を購読する。 */
export const onSnapshot = (
  cb: (snapshot: TimerSnapshot) => void,
): Promise<UnlistenFn> =>
  listen<TimerSnapshot>(EVENT_SNAPSHOT, (e) => cb(e.payload));

/** フェーズ境界イベントを購読する（#6 で使用）。 */
export const onTimerEvents = (
  cb: (events: TimerEvent[]) => void,
): Promise<UnlistenFn> =>
  listen<TimerEvent[]>(EVENT_TIMER_EVENTS, (e) => cb(e.payload));
