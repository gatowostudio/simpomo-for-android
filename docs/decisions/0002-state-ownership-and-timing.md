# ADR-0002: 状態の所在・設定ビュー方式・計時モデル

- ステータス: 採用（状態所有権 / 設定ビュー / バックグラウンド計時 = **deadline 永続化 option B**）
- 日付: 2026-06-25
- 決定者: 作者（gatowostudio / dekka）／参謀システム（devil 指摘起点）

> 本 ADR は、コード内コメントが参照する「ADR-0002」の正本（本リポジトリ版）。
> デスクトップ版 simpomo の ADR-0002（frontend-and-state-ownership）の判断のうち、Android 版でも
> 有効なものを引き継ぎ、無効/未決のものを明記する。技術選定の全体像は ADR-0001 を参照。

## 1. タイマー状態は Rust が権威（authoritative）を持つ — 採用（継承）

タイマーの状態機械（フェーズ / 残り時間 / サイクル番号 / サイクル数 0=手動・有限 N・無限）は Rust
（`timer.rs` の純粋な状態遷移ロジック）が真実を持つ。フロント（Svelte/WebView）は薄い表示層で、操作を
`invoke` で送り、状態を `timer-snapshot` イベントで受け取る。値の検証も Rust が権威
（`AppSettings::sanitized`）で、保存前に必ず正規化する。フロントの clamp は UX 用で信頼境界ではない。

実時間の駆動は Rust 側のバックグラウンドスレッド（`lib.rs: spawn_tick_loop`）が単調時計（`Instant` 差分）で
行い、非稼働中は条件変数で park して CPU を消費しない。この所有権モデルは Android でも有効なので継承する。

## 2. 設定は「単一画面内のビュー」— 採用（デスクトップ ADR-0002 の反転）

デスクトップ版は「メイン / 設定を別ウィンドウ」にしていたが、Android は単一ウィンドウが前提（ADR-0001）。
本アプリでは設定をメイン画面内のビュー（`App.svelte` の `view` 切替 + `Settings.svelte` の `onClose`）に
統合する。コード内コメントの「設定ビュー」はこの方式を指す（デスクトップ版の「別ウィンドウ」ではない）。

## 3. 計時モデル — **deadline 永続化（option B）を採用**

### 背景（参謀システム devil の [FUNDAMENTAL] 指摘）

`timer.rs` の状態機械は「現在状態 + 経過秒 → 次状態」の tick モデルで、これ自体はデスクトップから継承する。
しかし走行中タイマーの状態は**メモリ上の `Timer` にしか無く**、Android はバックグラウンドで OS が
WebView/スレッドの実行を絞り、一定時間後にプロセスごと kill しうる。kill されると `std::thread` と
`Instant` の基準が消え、復帰時に catch-up する材料も失われる（単調時計はプロセスローカルで再起動を跨げない）。
結果、何もしなければ「画面を消す / 別アプリへ行っても時間が進む」というポモドーロの中核が壊れる。

### 決定（2026-06-25 ユーザー判断）: option B = deadline 永続化

検討した A=前面限定 / B=deadline 永続化 / C=Native フォアグラウンドサービス のうち、**B を採用**。
A は中核体験を縮退させ、C は ADR-0001 の「Tauri 流用で軽く」を超える（Kotlin 層）。B は Tauri の枠内で
中核を守れる中庸。

実装（本リポジトリ）:

- **状態機械は tick モデルのまま**保つ（`timer.rs` は時計非依存で単体テスト可能、を維持）。
- 上位レイヤで**実行状態（`TimerState`）と壁時計 anchor（UNIX 秒）の対**を `session.json` に永続化する
  （`session.rs`）。保存はコマンド時とフェーズ境界時のみ（毎秒保存しない＝フラッシュ書込みを抑える。
  最後に保存した対 (remaining, anchor) と壁時計差分から現在値を一意に再構成できるため）。
- 再起動時（`lib.rs: apply_loaded_settings`）に `session.json` を読み、Running だった場合は
  `Timer::restore(config, state)` で復元してから `tick(now - anchor)` で**プロセス停止中の経過を取り戻す**。
  これにより kill / Doze / 画面オフを跨いでも走行中セッションが復元される。
- 復帰の catch-up イベント（過去のフェーズ境界）は通知音を鳴らさず統計にも数えない（水増し防止）。
- **前景の tick ループも壁時計（UNIX 秒）駆動に統一**した（`spawn_tick_loop`）。デスクトップ版は `Instant`
  （単調時計）だったが、Android では `Instant` がサスペンド中に止まり、バックグラウンド/画面オフ（プロセスは
  生存）から復帰したときに経過を取りこぼす。前景・復元の両方を壁時計に揃えることで、kill されていなくても
  復帰時にその場で経過を取り戻せる（永続 anchor と同一系統）。
- 統計閾値 `STATS_MAX_LIVE_GAP_SECS` は 5→15 秒に緩め、前景ジャンク（回転/GC）は数え、分単位の
  バックグラウンド不在は除外する。
- 起動分岐の核は純粋関数 `Timer::restore_for_launch` に切り出して単体テスト（Running catch-up / Paused /
  Idle+autostart / セッション無し / anchor=0 フォールバック / 時計巻き戻し）。`restore` は remaining と
  set_index を設定へクランプ。`persist_session` は anchor=0（壁時計取得失敗）を保存しない。

### 残る限界・トレードオフ（要実機検証）

- **壁時計駆動の代償**: 端末の時刻変更 / NTP の**前方**ジャンプは経過に乗る（走行中タイマーが進む/完了しうる）。
  巻き戻りは saturating で 0。ポモドーロは精度非クリティカルかつ NTP ステップは稀なので許容する。
- バックグラウンドで JS（WebView）が絞られると `timer-events` 自体が発火しないため、**裏に居る間のリアルタイム
  通知音**は保証されない（復帰時の状態・残り時間は正しいが、境界の瞬間の音は鳴らないことがある）。確実な裏通知が
  要件化したら C（Foreground Service）を再検討する。
- **未検証**: 上記はいずれも実機/エミュでの確認が必要（Android toolchain 導入後）。特に「kill→再起動」と
  「画面オフ→点灯（プロセス生存）」の両方で残り時間が壁時計通りかを実測すること。

## 関連メモ（同種のモバイル前提リスク）

- `notify.ts` の前景/背景判定は `document.visibilityState` を使う（Tauri の `isVisible()` は Android で
  常に false のため）。ただしバックグラウンドで JS が絞られると timer-events 自体が発火しないことがあり、
  確実な通知は上記 (B)/(C) の決定に依存する。
- `update.ts` の GitHub API 直叩きは、モバイルのキャリア NAT 配下で IP 共有によりレート制限に当たりやすい。
  将来「更新を確認」をモバイルで簡素化（Releases ページを開くだけ等）する余地がある。
