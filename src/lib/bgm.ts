// フォーカス用 BGM（#3）。音源ファイルは同梱せず、Web Audio で合成したアンビエントをループ再生する。
// 完全自作のためライセンス問題なし・容量ゼロ（CLAUDE.md の Don't / 軽量方針、通知音 sounds.ts と同方針）。
// rain / campfire は録音ではなく合成による近似。
//
// BgmId は src-tauri/src/settings.rs の BgmId enum（serde lowercase）と対応。種類を増やすときは
// ① 下の BgmId 型 ② BUILDERS ③ settings.ts の BGM_LABELS ④ settings.rs の enum を同期する
// （BGM_OPTIONS は BGM_LABELS から自動生成で不要）。
import { getAudioContext, clamp01, SILENCE } from "./audio";

export type BgmId = "none" | "white" | "pink" | "brown" | "rain" | "campfire";

/** ループ用ノイズの長さ。 */
const NOISE_SECONDS = 6;
/**
 * BGM 全体のゲイン上限。スライダ 0..100 はここ(0..BGM_CEIL)へ写像する。
 * BGM は「集中の邪魔をしない」控えめな音量が前提なので低めに抑える。
 */
const BGM_CEIL = 0.28;
/** ループ継ぎ目のクロスフェード長（クリックノイズ回避）。 */
const SEAM_FADE_SECONDS = 0.05;

// 焚き火クラックル（パチパチ）のパラメータ。
const CRACKLE_MIN_GAP_MS = 70;
const CRACKLE_GAP_JITTER_MS = 380;
const CRACKLE_HIGHPASS_HZ = 1500;
const CRACKLE_MIN_DUR = 0.02;
const CRACKLE_DUR_JITTER = 0.05;

type NoiseKind = "white" | "pink" | "brown";

// --- 状態（モジュール内シングルトン。1 つの BGM を再生する） ---
let current: BgmId = "none";
let master: GainNode | null = null;
let nodes: AudioScheduledSourceNode[] = [];
let crackleTimer: ReturnType<typeof setTimeout> | undefined;
// ノイズバッファはコストが高いので種類ごとにキャッシュして再利用する（フォーカス再開の度に作り直さない）。
const bufferCache = new Map<NoiseKind, AudioBuffer>();

// --- ノイズ生成 ---
const FILLERS: Record<NoiseKind, (d: Float32Array) => void> = {
  white(d) {
    for (let i = 0; i < d.length; i++) d[i] = Math.random() * 2 - 1;
  },
  pink(d) {
    // Paul Kellet のピンクノイズ近似。
    let b0 = 0, b1 = 0, b2 = 0, b3 = 0, b4 = 0, b5 = 0, b6 = 0;
    for (let i = 0; i < d.length; i++) {
      const w = Math.random() * 2 - 1;
      b0 = 0.99886 * b0 + w * 0.0555179;
      b1 = 0.99332 * b1 + w * 0.0750759;
      b2 = 0.969 * b2 + w * 0.153852;
      b3 = 0.8665 * b3 + w * 0.3104856;
      b4 = 0.55 * b4 + w * 0.5329522;
      b5 = -0.7616 * b5 - w * 0.016898;
      d[i] = (b0 + b1 + b2 + b3 + b4 + b5 + b6 + w * 0.5362) * 0.11;
      b6 = w * 0.115926;
    }
  },
  brown(d) {
    // ランダムウォーク（低域に偏った深いノイズ）。
    let last = 0;
    for (let i = 0; i < d.length; i++) {
      const w = Math.random() * 2 - 1;
      last = (last + 0.02 * w) / 1.02;
      d[i] = last * 3.5;
    }
  },
};

/**
 * ループ用ノイズバッファを返す（種類ごとにキャッシュ）。
 * 左右チャンネルを別々に生成してステレオに広げ（平坦さを和らげる）、末尾を少し余分に生成して
 * 先頭へクロスフェードすることでループ継ぎ目のクリックを消す。
 */
function getNoiseBuffer(ctx: AudioContext, kind: NoiseKind): AudioBuffer {
  const cached = bufferCache.get(kind);
  if (cached) return cached;

  const n = Math.floor(ctx.sampleRate * NOISE_SECONDS);
  const fade = Math.floor(ctx.sampleRate * SEAM_FADE_SECONDS);
  const buf = ctx.createBuffer(2, n, ctx.sampleRate);
  for (let ch = 0; ch < 2; ch++) {
    const tmp = new Float32Array(n + fade);
    FILLERS[kind](tmp); // チャンネルごとに独立した乱数 → 左右が相関せず広がりが出る
    // 末尾の延長 [n, n+fade) を先頭 [0, fade) へクロスフェードしループ継ぎ目を連続に。
    for (let i = 0; i < fade; i++) {
      const w = i / fade;
      tmp[i] = tmp[i] * w + tmp[n + i] * (1 - w);
    }
    buf.getChannelData(ch).set(tmp.subarray(0, n));
  }
  bufferCache.set(kind, buf);
  return buf;
}

/** ゆっくりした揺らぎ（呼吸するような動き）を master gain に重ね、機械的な平坦さを和らげる。 */
function addBreathing(ctx: AudioContext, out: GainNode, baseGain: number): void {
  const lfo = ctx.createOscillator();
  lfo.type = "sine";
  lfo.frequency.value = 0.11; // 約 9 秒周期
  const depth = ctx.createGain();
  depth.gain.value = baseGain * 0.18; // ±18% 程度の控えめな揺れ
  lfo.connect(depth).connect(out.gain);
  lfo.start();
  nodes.push(lfo);
}

/** ループノイズ + 任意フィルタ + ゲインを out へ繋いで再生し、停止できるよう記録する。 */
function addNoise(
  ctx: AudioContext,
  out: GainNode,
  kind: NoiseKind,
  gain: number,
  filter?: { type: BiquadFilterType; freq: number; q?: number },
): void {
  const src = ctx.createBufferSource();
  src.buffer = getNoiseBuffer(ctx, kind);
  src.loop = true;
  const g = ctx.createGain();
  g.gain.value = gain;
  let head: AudioNode = src;
  if (filter) {
    const bq = ctx.createBiquadFilter();
    bq.type = filter.type;
    bq.frequency.value = filter.freq;
    if (filter.q !== undefined) bq.Q.value = filter.q;
    src.connect(bq);
    head = bq;
  }
  head.connect(g).connect(out);
  src.start();
  nodes.push(src);
}

/** 焚き火のパチパチ音: 短いノイズバーストを不規則な間隔でスケジュールし続ける。 */
function scheduleCrackle(ctx: AudioContext, out: GainNode): void {
  const pop = () => {
    const dur = CRACKLE_MIN_DUR + Math.random() * CRACKLE_DUR_JITTER;
    const burst = ctx.createBuffer(1, Math.floor(ctx.sampleRate * dur), ctx.sampleRate);
    FILLERS.white(burst.getChannelData(0));
    const src = ctx.createBufferSource();
    src.buffer = burst;
    const hp = ctx.createBiquadFilter();
    hp.type = "highpass";
    hp.frequency.value = CRACKLE_HIGHPASS_HZ;
    const g = ctx.createGain();
    const t0 = ctx.currentTime;
    g.gain.setValueAtTime(SILENCE, t0);
    g.gain.exponentialRampToValueAtTime(0.25 + Math.random() * 0.35, t0 + 0.005);
    g.gain.exponentialRampToValueAtTime(SILENCE, t0 + dur);
    src.connect(hp).connect(g).connect(out);
    src.start(t0);
    src.stop(t0 + dur + 0.02);
    crackleTimer = setTimeout(pop, CRACKLE_MIN_GAP_MS + Math.random() * CRACKLE_GAP_JITTER_MS);
  };
  crackleTimer = setTimeout(pop, 250);
}

const BUILDERS: Record<Exclude<BgmId, "none">, (ctx: AudioContext, out: GainNode) => void> = {
  white: (ctx, out) => addNoise(ctx, out, "white", 0.28),
  pink: (ctx, out) => addNoise(ctx, out, "pink", 0.45),
  brown: (ctx, out) => addNoise(ctx, out, "brown", 0.6),
  // 弱めの雨（テントに当たる感じ）: 本体を抑えめ・やわらかめにし、細かい当たりを少し残す。
  rain: (ctx, out) => {
    addNoise(ctx, out, "white", 0.28, { type: "lowpass", freq: 1800 }); // やわらかい雨の本体
    addNoise(ctx, out, "white", 0.1, { type: "bandpass", freq: 4500, q: 0.8 }); // ぱらぱら当たる粒
  },
  campfire: (ctx, out) => {
    addNoise(ctx, out, "brown", 0.55, { type: "lowpass", freq: 500 }); // 低いゴーという炎
    scheduleCrackle(ctx, out); // パチパチ
  },
};

/** BGM を停止し、ノード・タイマーを片付ける（バッファキャッシュは残す）。 */
export function stopBgm(): void {
  if (crackleTimer) {
    clearTimeout(crackleTimer);
    crackleTimer = undefined;
  }
  for (const n of nodes) {
    try {
      n.stop();
    } catch {
      /* 既に停止済み */
    }
    try {
      n.disconnect();
    } catch {
      /* noop */
    }
  }
  nodes = [];
  if (master) {
    try {
      master.disconnect();
    } catch {
      /* noop */
    }
    master = null;
  }
  current = "none";
}

/**
 * 指定の BGM を音量 volume(0..1) で再生する。
 * 同じ BGM が既に鳴っているときは音量だけ更新（毎回呼ばれても鳴り直さない＝冪等）。
 * 音が出せない環境でもタイマー本体を止めないよう、失敗は握りつぶす（sounds.ts と同方針）。
 */
export function setBgm(id: BgmId, volume: number): void {
  if (id === "none") {
    stopBgm();
    return;
  }
  // 取得のたびに suspended なら resume を試みる（OS スリープ等からの復帰も兼ねる）。
  const ctx = getAudioContext();
  if (id === current && master) {
    master.gain.value = clamp01(volume) * BGM_CEIL;
    return;
  }
  stopBgm();
  try {
    const baseGain = clamp01(volume) * BGM_CEIL;
    master = ctx.createGain();
    master.gain.value = baseGain;
    master.connect(ctx.destination);
    BUILDERS[id](ctx, master);
    addBreathing(ctx, master, baseGain);
    current = id;
  } catch (e) {
    console.error("failed to start bgm", e);
    stopBgm();
  }
}
