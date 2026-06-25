# ADR-0001: Android 版の技術選定とコード流用方針

- ステータス: 採用
- 日付: 2026-06-25
- 決定者: 作者（gatowostudio / dekka）／project-init 対話

## コンテキスト

既存のデスクトップ版ポモドーロ [simpomo](https://github.com/gatowostudio/simpomo)（Tauri v2 + Svelte5 + Rust）の
**Android アプリ版**を、別リポジトリ（https://github.com/gatowostudio/simpomo-for-android）で作る。要件は次の通り:

- **横画面（landscape）固定・全画面（immersive）**で使うアプリ。
- 「各種設定は使えるものはそのまま持っていく」＝ 移植可能な設定/機能は流用する。
- リリースは **GitHub Releases**。

デスクトップ版はタイマー状態の真実を Rust コア（`timer.rs` の純粋状態機械、単体テスト済み）が持ち、UI は薄い Svelte 層、
音は Web Audio の合成音（同梱ファイルなし＝クロスプラットフォーム）という構成。一方で機能の多くはウィンドウ中心
（最前面・トレイ・位置/サイズ永続・二重起動防止・OS スタートアップ登録）で、これらは Android には存在しない。
よって「どの土台で作り、何を流用し、何を捨てるか」を着手前に確定する。

## 検討した選択肢

1. **Tauri v2 Android で流用** — 既存の Svelte UI と Rust コアをそのまま Android ターゲットに載せる。
   - 長所: タイマーコア（`timer.rs`）・設定モデル・音生成（Web Audio）を**コードごと再利用**でき、「設定をそのまま持っていく」
     要求に最短で応える。Tauri v2 は Android/iOS を公式サポート。APK/AAB を GitHub Releases で配布できる。
   - 短所: Android Studio + SDK/NDK + JDK + Rust Android ターゲットのツールチェインが必要。Tauri モバイルは比較的新しく、
     画面常時 ON・バックグラウンド計測・WebView の audio autoplay などモバイル固有の検証が要る。
2. **Native Kotlin / Jetpack Compose で作り直し** — ゼロから実装。
   - 長所: 最も Android らしい UX、省メモリ、フォアグラウンドサービスで裏でも確実に計測できる。
   - 短所: コード再利用ゼロ。タイマー状態機械と音合成を Kotlin で再実装。「設定を持っていく」が「同じ項目を作り直す」になる。
3. **Capacitor / PWA（フロントだけ流用）** — Svelte UI を Capacitor でラップ。
   - 長所: フロントは再利用できる。
   - 短所: Rust コアは使えず TS へ再実装が必要。WebView のバックグラウンド JS タイマー停止問題に別途対処要。
     結局 Tauri 流用の下位互換になりやすい。

## 決定

**選択肢 1（Tauri v2 Android で流用）** を採用する。あわせて次を決める:

- **流用するもの**: タイマーコア（`timer.rs` の状態機械）、設定モデル（`settings.rs` / `settings.ts`）、
  音生成（Web Audio: 通知音3種・BGM ノイズ6種）、背景色・作業/休憩時間・サイクル数などの設定ロジック、Svelte コンポーネント。
- **捨てる/置き換えるデスクトップ専用機能**: 表示位置（四隅 `corner`）、トレイ常駐（`skipTaskbar`）、ウィンドウ位置/サイズ永続
  （`tauri-plugin-window-state`）、最前面トグル（always-on-top）、二重起動防止（`tauri-plugin-single-instance`）、
  OS スタートアップ登録（`tauri-plugin-autostart`）、`layout.rs`。Rust 側は `#[cfg(desktop)]` 等でモバイルから切り分ける。
  通知（`tauri-plugin-notification`）は Android 対応のため**維持**。
- **画面構成は単一画面**: デスクトップ版 ADR-0002 は「main/settings を別ウィンドウ、単一ウィンドウ内切替は不採用」と決めたが、
  Android は単一ウィンドウが前提。本アプリでは **ADR-0002 の判断を反転し、設定をメイン画面内のビュー/ルートに統合**する。
- **方向・表示**: landscape 固定 + immersive 全画面。計測中は画面を常時 ON に保つ（keep-screen-on）。
- **配布**: `tauri android build` の APK/AAB を GitHub Releases へ。リリース署名キーストアはリポジトリ外で管理。

## 理由

要求の核心は「既存 simpomo の体験と設定を Android へ持っていく」こと。Tauri 流用なら、最も価値の高い資産である
**時計非依存で単体テスト済みのタイマーコア**と**クロスプラットフォームな Web Audio 音生成**をコードごと運べる。
Native Kotlin は UX で勝るが、移植要求に対して再実装コストが大きく、コアの作り直しは品質リスクにもなる。
Capacitor は Rust コアを捨てる時点で流用メリットが半減し、Tauri 流用の劣位互換になりやすい。
モバイル固有の懸念（バックグラウンド計測など）は MVP では「画面常時 ON のフォアグラウンド利用」を前提に回避し、
確実なバックグラウンド計測は後続課題として切り出す（spec オープン課題）。

## 結果と影響

- ポジティブな影響:
  - タイマーコア・設定・音をコードごと流用でき、実装と検証の総量が小さい。
  - Rust 単体テスト方針をそのまま継承できる（DoD と整合）。
  - 単一画面化でデスクトップの2ウィンドウ IPC 複雑性が減る。
- ネガティブな影響・トレードオフ:
  - Android ツールチェイン（Studio/SDK/NDK/JDK/Rust ターゲット）の構築が必要。
  - Tauri モバイル固有の検証項目が増える（landscape 固定・immersive・keep-screen-on・audio autoplay・バックグラウンド挙動）。
  - AndroidManifest 等の手編集が要ると `src-tauri/gen/android` の追跡方針を決め直す必要がある（development.md 参照）。
- 将来の見直しトリガー:
  - バックグラウンドでの確実な計測がフォアグラウンド前提で満たせないと判明し、かつ重要要件になった場合
    （フォアグラウンドサービス導入 or Native 化を再検討）。
  - Tauri モバイルが要求機能を満たせない致命的制約が出た場合。
