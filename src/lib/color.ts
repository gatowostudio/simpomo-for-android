// 背景色から読みやすい文字色（黒/白）を選ぶ小ユーティリティ。入力は #rrggbb 前提。
// Rust 側(settings.rs)が色を sanitize するので通常は妥当な hex が来るが、念のため検証する。
const LUMINANCE_THRESHOLD = 150; // 0..255。これより明るい背景なら黒文字にする。
const DARK_TEXT = "#1a1a1a";
const LIGHT_TEXT = "#f0f0f0";

/** #rrggbb の背景色に対して読みやすい文字色を返す。不正な値は明色にフォールバック。 */
export function textColorFor(hex: string): string {
  const m = /^#([0-9a-fA-F]{6})$/.exec(hex);
  if (!m) return LIGHT_TEXT;
  const n = parseInt(m[1], 16);
  const r = (n >> 16) & 0xff;
  const g = (n >> 8) & 0xff;
  const b = n & 0xff;
  // BT.601 の重み付け（簡易輝度）。厳密な WCAG ではないが黒/白の二択には十分。
  const luminance = 0.299 * r + 0.587 * g + 0.114 * b;
  return luminance > LUMINANCE_THRESHOLD ? DARK_TEXT : LIGHT_TEXT;
}
