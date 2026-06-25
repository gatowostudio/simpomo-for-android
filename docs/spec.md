# Specification — simpomo-for-android

デスクトップ版 [simpomo](https://github.com/gatowostudio/simpomo) の Android 移植版の機能要件・非機能要件。
**横画面（landscape）固定・全画面（immersive）**で使うポモドーロタイマー。元アプリのコア（タイマー状態機械・設定モデル・
Web Audio 音生成）を流用し、デスクトップ専用のウィンドウ機能を外して単一画面に作り直す。配布は GitHub Releases（APK/AAB）。

技術選定とコード流用方針は [`decisions/0001-android-stack-and-reuse.md`](decisions/0001-android-stack-and-reuse.md) を正本とする。

## 機能要件

### コア機能（MVP）— デスクトップ版から「持っていく」設定

- [ ] **常駐タイマー表示（横画面・全画面）**: 残り時間と現在フェーズ（作業 / 休憩）が一目でわかる。横向き全画面で大きく表示。
- [ ] **作業/休憩時間のカスタム**: 既定 作業25分 → 休憩5分。設定で各時間を変更可能（流用: `workSecs` / `breakSecs`）。
- [ ] **サイクル数の設定（自動継続するサイクル数）**: 流用: `cyclesInfinite` / `cyclesCount`。
      - **0（既定）**: 作業→休憩を1セット実行したら停止（次は start まで待機）。
      - **有限 N**: N セットを自動連続実行して停止。
      - **無限**: 止めるまで自動継続。
- [ ] **通知音の選択**: 作業終了 / 休憩終了 / 完了 の音をそれぞれ選択＋音量（流用: `workEndSound` / `breakEndSound` /
      `sessionEndSound` / `volume`。Web Audio 合成音、同梱ファイルなし）。
- [ ] **フォーカス BGM**: ホワイト / ピンク / ブラウンノイズ・雨・焚き火 ＋音量（流用: `focusBgm` / `bgmVolume`。休憩中は停止）。
- [ ] **背景色のカスタマイズ**: 作業中 / 休憩中の背景色（流用: `focusBgColor` / `breakBgColor`）。音が無くても色でフェーズが分かる。
- [ ] **起動時自動スタート**: 起動時にタイマーを自動開始するか（流用: `autostartTimer`）。※OS スタートアップ登録とは別物。
- [ ] **OS 通知**: フェーズ境界を Android 通知で知らせる（流用: `osNotifications` + `tauri-plugin-notification`）。
- [ ] **完了数の統計**: 完了フォーカス / セット数の集計・表示・リセット（流用: stats）。
- [ ] **操作**: Start / Pause / Reset（先頭へ）/ Skip（次フェーズへ）。
- [ ] **設定画面（アプリ内）**: 上記設定を変更・永続化する UI。**単一画面内のビュー/ルート**として実装（別ウィンドウにしない）。

### Android 固有要件（新規）

- [ ] **画面方向の固定**: landscape（横）に固定。
- [ ] **全画面表示**: immersive（システムバー非表示）。
- [ ] **画面常時 ON（keep-screen-on）**: 計測中はスリープさせない。
- [ ] **配布**: `tauri android build` で APK/AAB を生成し GitHub Releases に配布。

### 拡張機能（MVP 後）

- [ ] 「更新を確認」（GitHub Releases の最新版チェック）。デスクトップ版と異なり Android は手動 APK 更新になる点に注意。
- [ ] 確実なバックグラウンド計測（フォアグラウンドサービス等）。

## Android で「持っていかない」デスクトップ専用機能

いずれもモバイル全画面では無意味なため除去（理由は ADR-0001）:
表示位置（四隅 `corner`）／トレイ常駐（`skipTaskbar`）／ウィンドウ位置・サイズ永続（window-state）／
最前面トグル（always-on-top）／二重起動防止（single-instance）／OS スタートアップ登録（autostart プラグイン）／`layout.rs`。

## 非機能要件

- **パフォーマンス / 軽量性**: 重量級依存を持ち込まない。非稼働中（Idle/Paused）の CPU を最小に保つ
      （デスクトップ版は tick スレッドを park して非稼働中 CPU ゼロ。流用方針）。
- **可用性**: オフライン完結。ネットワーク不要で動作。
- **セキュリティ / 公開対応**: 公開リポジトリ。リリース署名キーストア・その資格情報・`.env` をコミットしない。同梱音源なし。
- **アクセシビリティ**: 横画面で残り時間が判読できるコントラスト/フォントサイズ（デスクトップ版は `clamp()` でスケール）。

## 制約

- Android（phone / tablet 両対応）。landscape 固定・immersive 全画面。
- ビルドに Android Studio + SDK / NDK + JDK 17 + Rust の Android ターゲットが必要（`docs/development.md`）。
- タイマー状態の真実は Rust コア（`timer.rs`）。UI は表示と操作のみ（デスクトップ版の所有権モデルを流用）。

## 画面一覧

- **メイン（タイマー）画面**: 残り時間・フェーズ・操作（Start/Pause/Reset/Skip）。横画面全画面。
- **設定ビュー**: 上記設定。メイン画面内の切替（ルート/オーバーレイ）として実装。**別ウィンドウにしない**（ADR-0001）。

## オープン課題

- [x] **バックグラウンド計測**: deadline 永続化（ADR-0002 §3 / option B）を採用・実装済み。実行状態と壁時計 anchor を
      `session.json` に永続化し、再起動時に `tick(now - anchor)` で kill/Doze/画面オフ中の経過を取り戻す。
      残る限界: 裏に居る間のリアルタイム通知音は JS 実行制限により保証されない（復帰時の状態は正しい）。要実機検証。
- [ ] **WebView の audio autoplay**: Android WebView は最初のユーザー操作まで音を出せない可能性。デスクトップ版の
      `unlockAudioOnUserGesture`（最初の操作で AudioContext を resume）を流用して対処。起動時自動スタート時の初回音は要検証。
- [ ] **画面方向固定・全画面・keep-screen-on の実装手段**: AndroidManifest（`screenOrientation`）/ Tauri 設定 / 小さなプラグイン
      のどれで実現するか。`src-tauri/gen/android` を手編集する場合は追跡方針を決める（development.md）。
- [ ] **リリース署名**: APK インストールには署名が必須。リリース用キーストアを作成し、リポジトリ外（GitHub Secrets / 端末ローカル）で管理。
      Play ストア配布は当面想定せず、GitHub Releases の APK 直接配布（要「提供元不明アプリ」許可）。
- [ ] **アイコン**: デスクトップ版の自作トマトアイコン（`app-icon.svg`）から Android 用アイコンを `tauri icon` で生成。
- [ ] **通知権限**: Android 13+ は通知に実行時権限（`POST_NOTIFICATIONS`）が必要。フローを実装。
