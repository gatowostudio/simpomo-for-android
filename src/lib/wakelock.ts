// 画面常時ON（#2 / keep-screen-on）。タイマー稼働中は画面を消灯させない（卓上で眺める用途のため）。
//
// Android WebView は W3C **Screen Wake Lock API**（`navigator.wakeLock`）をサポートするので、これを使う。
// 利点はフロントから ON/OFF を**動的に**切り替えられる点（稼働中だけ点灯、停止/一時停止でスリープに戻す）。
// native の `FLAG_KEEP_SCREEN_ON` は「前面の間ずっと点灯」になりがちで動的トグルに JS→native ブリッジが要る。
// ※**暫定実装・実機検証待ち**（issue #2）。Tauri WebView で wakeLock が実用か / Doze 下で保持されるかは未確認。
//   未対応や Doze 解放で実用に足りないと分かったら native `FLAG_KEEP_SCREEN_ON` フォールバックへ切り替える。
//   このモジュールは Android 固有で desktop 版 simpomo には無い（コア重複の対象外）。
//
// 仕様上の制約: ページが不可視（バックグラウンド/画面オフ）になると wake lock は OS により**自動解放**される。
// 復帰（visibilitychange → visible）時に「まだ ON を望んでいる」なら取り直す。そこで「望む状態(desired)」を
// 保持し、可視化のたびに desired を再適用する。
//
// 並行性: acquire/release は `await` をまたぐので、素朴に書くと「request 解決中に OFF を望まれる」「2 本の
// acquire が同時に実 lock を 2 個取る」等で desired と実 lock がズレる。これを避けるため、(1) 全操作を 1 本の
// Promise チェーンに直列化し、(2) request 解決直後に desired を再確認して取り消し、(3) release リスナは
// identity 一致時のみ参照を消す、の 3 点で守る。

let sentinel: WakeLockSentinel | null = null;
// 望む状態。setKeepScreenOn の最後の指示を覚え、自動解放後の再取得（可視化時）や取り消し判断に使う。
let desired = false;
// acquire/release を直列化するチェーン。await 境界での競合（孤児 lock / desired との不整合）を防ぐ。
let chain: Promise<void> = Promise.resolve();

const supported = typeof navigator !== "undefined" && "wakeLock" in navigator;
// 未対応環境では no-op に縮退する。沈黙縮退に気づけるよう、初回の ON 要求で 1 度だけ警告する（実機検証の手掛かり）。
let warnedUnsupported = false;

function enqueue(task: () => Promise<void>): void {
  chain = chain.then(task).catch((e) => {
    console.error("wake lock task failed", e);
  });
}

async function acquire(): Promise<void> {
  if (sentinel) return; // 直列化済みなので二重取得は起きないが、保持中なら何もしない。
  const s = await navigator.wakeLock.request("screen");
  // request 解決までに OFF を望まれていたら取り消す（点きっぱなし防止）。
  if (!desired) {
    try {
      await s.release();
    } catch {
      // 既に解放済みなら無視。
    }
    return;
  }
  sentinel = s;
  // OS 都合（画面オフ等）で解放されたら参照を捨てる。ただし**この sentinel が現役のときだけ**消す
  // （遅延発火した古いリスナが、後から取り直した別の lock を誤って消さないように）。
  s.addEventListener("release", () => {
    if (sentinel === s) sentinel = null;
  });
}

async function release(): Promise<void> {
  const held = sentinel;
  sentinel = null;
  if (!held) return;
  try {
    await held.release();
  } catch {
    // 既に解放済みなら無視してよい。
  }
}

/** 画面常時ONの ON/OFF を切り替える。タイマー稼働中は true、停止/一時停止で false を渡す。 */
export function setKeepScreenOn(on: boolean): void {
  desired = on;
  if (!supported) {
    if (on && !warnedUnsupported) {
      warnedUnsupported = true;
      console.warn("screen wake lock unsupported; screen may sleep during sessions");
    }
    return;
  }
  enqueue(on ? acquire : release);
}

// 可視化のたびに desired を再適用（不可視中に自動解放された lock を取り直す）。
if (supported && typeof document !== "undefined") {
  document.addEventListener("visibilitychange", () => {
    if (desired && document.visibilityState === "visible") enqueue(acquire);
  });
}
