<script lang="ts">
  import { onMount } from "svelte";
  import * as timer from "./lib/timer";
  import {
    getSettings,
    onSettingsChanged,
    SECS_PER_MINUTE,
    DEFAULT_FOCUS_BG,
    DEFAULT_BREAK_BG,
    type AppSettings,
  } from "./lib/settings";
  import { playSound, soundsForEvents, type SoundId } from "./lib/sounds";
  import { setBgm, stopBgm, type BgmId } from "./lib/bgm";
  import { unlockAudioOnUserGesture } from "./lib/audio";
  import { notifyIfHidden, ensureNotificationPermission } from "./lib/notify";
  import { textColorFor } from "./lib/color";
  import Settings from "./Settings.svelte";

  let snap = $state<timer.TimerSnapshot | null>(null);
  // 単一画面（ADR-0001）。歯車でタイマー表示と設定ビューを切り替える（別ウィンドウにしない）。
  let view = $state<"timer" | "settings">("timer");

  let workEndSound = $state<SoundId>("chime");
  let breakEndSound = $state<SoundId>("ding");
  let sessionEndSound = $state<SoundId>("fanfare");
  let volume = $state(70);
  let focusBgm = $state<BgmId>("none");
  let bgmVolume = $state(25);
  let osNotifications = $state(false);
  let focusBgColor = $state(DEFAULT_FOCUS_BG);
  let breakBgColor = $state(DEFAULT_BREAK_BG);

  const isRunning = $derived(snap?.status === "running");
  const isBreak = $derived(snap?.phase === "break");
  const phaseLabel = $derived(isBreak ? "BREAK" : "FOCUS");

  // 背景色をフェーズで切替（音が無くても色で分かる）。文字色は背景の明るさから自動でコントラスト。
  const bgColor = $derived(isBreak ? breakBgColor : focusBgColor);
  const fgColor = $derived(textColorFor(bgColor));

  // セット表示: 「現在/総数」。無限は ∞。
  const setLabel = $derived.by(() => {
    if (!snap) return "";
    const current = snap.setIndex + 1;
    return snap.totalSets === null ? `${current} / ∞` : `${current} / ${snap.totalSets}`;
  });

  const clock = $derived(snap ? formatClock(snap.remainingSecs) : "--:--");

  function formatClock(secs: number): string {
    const m = Math.floor(secs / SECS_PER_MINUTE)
      .toString()
      .padStart(2, "0");
    const s = (secs % SECS_PER_MINUTE).toString().padStart(2, "0");
    return `${m}:${s}`;
  }

  onMount(() => {
    // Android WebView は最初のユーザー操作まで音を出せないことがある。最初の操作で AudioContext を resume する。
    unlockAudioOnUserGesture();

    const unlisteners: Array<() => void> = [];
    // disposed: listen の Promise が解決する前に unmount された場合に listener を取りこぼさない。
    let disposed = false;
    const track = (p: Promise<() => void>) => {
      p.then((u) => (disposed ? u() : unlisteners.push(u)));
    };

    // 先に listener を張ってから初期 snapshot を取得し、初期化中の更新を取りこぼさない。
    track(timer.onSnapshot((s) => (snap = s)));
    timer.getSnapshot().then((s) => {
      if (snap === null) snap = s;
    });

    // 通知音/BGM/背景色の選択・音量を読み込み、設定変更（settings-changed）に追従する。
    const applySoundSettings = (s: AppSettings) => {
      workEndSound = s.workEndSound;
      breakEndSound = s.breakEndSound;
      sessionEndSound = s.sessionEndSound;
      volume = s.volume;
      focusBgm = s.focusBgm;
      bgmVolume = s.bgmVolume;
      osNotifications = s.osNotifications;
      focusBgColor = s.focusBgColor;
      breakBgColor = s.breakBgColor;
    };
    getSettings()
      .then((s) => {
        applySoundSettings(s);
        // 通知が有効なら、起動時のうちに権限を確保しておく。
        if (s.osNotifications) void ensureNotificationPermission();
      })
      .catch(() => {});
    track(onSettingsChanged(applySoundSettings));

    // フェーズ境界で通知音を鳴らす。完了時は完了音、catch-up（複数境界）は種類ごと 1 回に畳む。
    // 手動 skip は Rust 側でイベントを出さないので鳴らない。
    track(
      timer.onTimerEvents((events) => {
        for (const id of soundsForEvents(events, {
          work: workEndSound,
          break: breakEndSound,
          session: sessionEndSound,
        })) {
          playSound(id, volume / 100);
        }
        // バックグラウンドへ退いている間は OS 通知でも知らせる（#20。表示中は何もしない）。
        if (osNotifications) void notifyIfHidden(events);
      }),
    );

    return () => {
      disposed = true;
      unlisteners.forEach((u) => u());
    };
  });

  // フォーカス（作業フェーズ・稼働中）のみ BGM を流す。休憩/一時停止/停止では止める。
  // focusActive は $derived の真偽値なので、毎秒の snapshot 更新では値が変わらず effect は
  // 再実行されない（フェーズや BGM 設定が変わった縁でのみ作用する＝edge 駆動）。
  const focusActive = $derived(snap?.status === "running" && snap?.phase === "work");
  $effect(() => {
    // 設定ビュー表示中は BGM 制御を Settings の試聴へ委ねる（BGM は bgm.ts の単一インスタンスを
    // 共有するため、両者が奪い合うと止め合いになる）。タイマー表示へ戻った縁でここが再評価され、
    // 本来のフォーカス BGM 状態を再適用する（設定を開閉しても BGM が無音のままにならない）。
    if (view !== "timer") return;
    if (focusActive && focusBgm !== "none") setBgm(focusBgm, bgmVolume / 100);
    else stopBgm();
  });

  // 状態更新は onSnapshot（emit）の単一経路。コマンドは結果を代入しない。
  async function toggleStartPause() {
    await (isRunning ? timer.pause() : timer.start());
  }
  async function onReset() {
    await timer.reset();
  }
  async function onSkip() {
    await timer.skip();
  }
</script>

{#if view === "settings"}
  <Settings onClose={() => (view = "timer")} />
{:else}
  <main class="timer" style="background-color: {bgColor}; color: {fgColor};">
    <button class="gear" title="Settings" aria-label="Settings" onclick={() => (view = "settings")}
      >⚙</button
    >

    <div class="display">
      <div class="phase">{phaseLabel}</div>
      <div class="clock">{clock}</div>
      <div class="sets">{setLabel}</div>
    </div>

    <div class="controls">
      <button onclick={onReset} title="Reset" aria-label="Reset">⟲</button>
      <button class="primary" onclick={toggleStartPause}>
        {isRunning ? "Pause" : "Start"}
      </button>
      <button onclick={onSkip} title="Skip" aria-label="Skip">⏭</button>
    </div>
  </main>
{/if}

<style>
  /*
    全画面 landscape タイマー（ADR-0001）。残り時間を画面いっぱいに大きく表示する。
    サイズは vmin（landscape では画面高さ＝小さい辺）に比例させ、端末サイズに追従する。
  */
  .timer {
    position: relative;
    height: 100%;
    width: 100%;
    box-sizing: border-box;
    overflow: hidden;
    padding: max(env(safe-area-inset-top), 0.6em) max(env(safe-area-inset-right), 1em)
      max(env(safe-area-inset-bottom), 0.6em) max(env(safe-area-inset-left), 1em);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.4em;
    font-size: clamp(16px, 11vmin, 110px);
    user-select: none;
    -webkit-user-select: none;
    -webkit-tap-highlight-color: transparent;
    touch-action: manipulation;
    transition:
      background-color 0.3s,
      color 0.3s;
  }

  .display {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.05em;
  }

  .phase {
    font-size: 0.55em;
    letter-spacing: 0.2em;
    opacity: 0.7;
  }
  .clock {
    font-size: 2.6em;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    line-height: 1.02;
  }
  .sets {
    font-size: 0.5em;
    opacity: 0.6;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 0.5em;
    margin-top: 0.2em;
  }
  .controls button {
    font: inherit;
    font-size: 0.5em;
    color: inherit;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 0.5em;
    padding: 0.45em 0.7em;
    cursor: pointer;
    min-width: 2.2em;
  }
  .controls button:active {
    background: rgba(255, 255, 255, 0.2);
  }
  .controls .primary {
    min-width: 6em;
    font-weight: 600;
  }

  .gear {
    position: absolute;
    top: max(env(safe-area-inset-top), 0.5em);
    right: max(env(safe-area-inset-right), 0.6em);
    background: none;
    border: none;
    cursor: pointer;
    color: inherit;
    opacity: 0.4;
    line-height: 1;
    padding: 0.2em;
    font-size: 0.5em;
  }
  .gear:active {
    opacity: 0.85;
  }
</style>
