// 通知音(sounds.ts)と BGM(bgm.ts)で共有する単一の AudioContext と、Web Audio 共通の小道具。
// いずれかのユーザー操作（start / 試聴 など）の後に呼ばれる前提なので、resume は通常許可される。
let ctx: AudioContext | null = null;

export function getAudioContext(): AudioContext {
  if (!ctx) ctx = new AudioContext();
  if (ctx.state === "suspended") void ctx.resume();
  return ctx;
}

// 起動時自動スタート（#21）対策。ユーザー操作前に始まったセッションでは AudioContext が
// suspended のまま＝通知音/BGM が鳴らない（WebView の autoplay 制約）。最初の操作（クリック/キー）で
// 一度だけ resume する保険を張る。完全に無操作のままの再生はブラウザ仕様上保証できない。
let unlockArmed = false;
export function unlockAudioOnUserGesture(): void {
  if (unlockArmed) return;
  unlockArmed = true;
  const unlock = () => {
    getAudioContext(); // 生成 + suspended なら resume
    window.removeEventListener("pointerdown", unlock);
    window.removeEventListener("keydown", unlock);
  };
  window.addEventListener("pointerdown", unlock);
  window.addEventListener("keydown", unlock);
}

/** ゲインに渡す音量を 0..1 にクランプする。 */
export const clamp01 = (v: number): number => Math.max(0, Math.min(1, v));

/** exponentialRamp は 0 を取れないため、無音とみなす下限値。 */
export const SILENCE = 0.0001;
