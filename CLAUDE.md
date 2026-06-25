# simpomo-for-android

## What this is
既存デスクトップ版ポモドーロ **[simpomo](https://github.com/gatowostudio/simpomo)**（Tauri v2 + Svelte5 + Rust）の
**Android アプリ版**。横画面（landscape）固定・全画面（immersive）で使う、据え置き/卓上想定のポモドーロタイマー。
タイマー状態の真実は Rust のコア（`timer.rs` の純粋状態機械）が持ち、UI は Svelte。音は Web Audio の合成音（同梱ファイルなし）。
形態: Android アプリ（Tauri v2 mobile で APK/AAB をビルドし GitHub Releases で配布）。

## Why it exists
デスクトップ版 simpomo の「邪魔にならず・見やすく・軽い」体験を、横向きに置いた Android 端末（タブレット/スマホ）でも使いたい。
既存のコア（タイマー状態機械・設定モデル・音生成）をコードごと流用し、デスクトップ専用のウィンドウ機能だけを外して、
モバイルの全画面1枚に作り直す。元アプリ同様、軽量・オフライン完結・公開（OSS）を維持する。

## Project Type
- ソフトウェア開発（既存アプリの新プラットフォーム移植）

## Stack
- **フレームワーク**: Tauri v2（**mobile / Android ターゲット**）
- **コアロジック**: Rust（`timer.rs` の純粋状態機械を**デスクトップ版からそのまま流用**・単体テスト済み）
- **UI**: Svelte5 + Vite + TypeScript（デスクトップ版の Svelte コンポーネントを**単一画面・横向きに作り直して流用**）
- **音**: Web Audio による合成音（通知音3種・BGM ノイズ6種）。同梱音源ファイルなし＝クロスプラットフォーム流用可
- **パッケージマネージャ**: pnpm（フロント）／ cargo（Rust）
- **配布**: GitHub Releases（APK / AAB）。リポジトリ: https://github.com/gatowostudio/simpomo-for-android
- **アプリ識別子（予定）**: `com.gatowostudio.simpomo`（productName: `simpomo`）。変更可。
- 動作環境: Android（phone / tablet 両対応）。**landscape 固定・immersive 全画面**。
- ビルド環境: Android Studio + Android SDK / NDK + JDK 17 + Rust の Android ターゲット。詳細は `docs/development.md`。

## Definition of Done
- デスクトップ版から「持っていく」と決めた全設定（下記 spec 参照）が Android 上で動作する。
- 横画面・全画面で、残り時間とフェーズが一目でわかるメイン画面＋アプリ内設定画面が動く。
- `tauri android build` で APK/AAB が生成でき、GitHub Releases に配布できる状態。
- 流用したコアロジック（タイマー進行・サイクル遷移）の Rust 単体テストが pass する。

### テスト方針
- **コアロジックのみ Rust 単体テスト**（デスクトップ版から流用）: カウントダウン、作業→休憩→次サイクルの遷移、
  サイクル数（0=手動 / 有限 N / 無限）、設定値の境界。
- UI・通知音・横画面/全画面・画面常時 ON などは実機（または Android エミュレータ）で手動確認。

### 成功指標
- 作者本人が横向きの端末で日常的に使い続けられること。
- 軽量さの維持（重量級依存を持ち込まない）。

## Don't
- **公開リポジトリ**。機密情報を一切コミットしない。特に **Android のリリース署名キーストア（`*.jks` / `*.keystore`）・
  その資格情報（`keystore.properties`）・`.env`** は絶対にコミットしない（`.gitignore` 済み。鍵は GitHub Secrets / 端末ローカルで管理）。
- **著作権のある音源を同梱しない**（現状は Web Audio 合成のため同梱音源なし）。
- デスクトップ専用機能を Android に無理に移植しない（下記）。Electron 等の重量級スタックも不採用。

### Android で「持っていかない」デスクトップ専用機能
表示位置（四隅 `corner`）／トレイ常駐（`skipTaskbar`）／ウィンドウ位置・サイズ永続（window-state）／
最前面トグル（always-on-top）／二重起動防止（single-instance）／OS スタートアップ登録（autostart プラグイン）／
`layout.rs`。これらはモバイル全画面では無意味なので除去（Rust 側は `#[cfg(desktop)]` 等で切り分け）。
**2ウィンドウ構成（main/settings 別ウィンドウ）→ 単一画面内の設定ビューに統合**（デスクトップ ADR-0002 の判断を Android 向けに反転）。

## External Services
- なし（オフライン完結）。
- 将来「更新を確認」を入れる場合のみ GitHub Releases API に手動接続（任意）。リリース署名キーストアはリポジトリ外で管理。

## Stakeholders
- 主に作者本人（gatowostudio / dekka）。
- 公開リポジトリのため、ソースを見る/使う不特定の利用者も想定。

## When working on...
- 機能仕様（持っていく設定・捨てる機能・Android 固有要件） → `docs/spec.md`
- アーキ判断（なぜ Tauri 流用か・単一画面化） → `docs/decisions/0001-android-stack-and-reuse.md`
- 環境構築・ビルド・リリース手順 → `docs/development.md`
- 状態の所在・設定ビュー方式・計時モデル → `docs/decisions/0002-state-ownership-and-timing.md`
- 設計判断の履歴 → `docs/decisions/`
- 元アプリの実装の正本 → `C:\Dev\simpomo`（Svelte: `src/`、Rust コア: `src-tauri/src/`）。流用元はここを参照。
- ドメイン用語が増えたら → `docs/glossary.md` を作って記録する（現状は不要なため未作成）。

### ⚠️ 2リポジトリ間のコア重複（保守時の必読事項）
本リポの以下は デスクトップ版 `C:\Dev\simpomo` と**バイト一致のコピー**: `src-tauri/src/timer.rs`,
`src-tauri/src/stats.rs`, `src/lib/{sounds,bgm,notify,timer,stats,audio,color}.ts`。
`settings.rs` / `settings.ts` は desktop 専用フィールド削除ぶんだけ分岐済み。
**コア（タイマー状態機械・統計・音生成・Rust↔TS の同期契約）を直すときは両リポに反映が必要**。
片方だけ直すと、`#[serde(default)]` により不整合が例外を出さず静かに進行する（android だけ新設定が効かない 等）。
リポ間ドリフトを検出する仕組みは無い。将来の見直しトリガー: コア変更が頻発するなら共有 crate 化 or
desktop リポへの Android ターゲット統合（1リポ化）を再検討（ADR-0001 / 0002 参照）。

## Commands
- フロント依存: `pnpm install`
- 型チェック: `pnpm check`（svelte-check。検証済み: 0 errors）
- フロントビルド: `pnpm build`（`dist/` を生成。`tauri build`/`generate_context!` が参照）
- Rust テスト: `cargo test --manifest-path src-tauri/Cargo.toml`（検証済み: 39 passed）
- Android 実機/エミュ起動: `pnpm tauri android dev` ※要 Android toolchain（未導入の環境では不可）
- Android ビルド: `pnpm tauri android build`（APK/AAB を生成）※同上

## Notes
- サイクル数 = 自動継続するサイクル数。0=1セットで停止し手動 start 待ち / 有限 N / 無限。詳細は spec 参照。
- **バックグラウンド計時（実装済み・deadline 永続化）**: 実行状態と壁時計 anchor を `session.json` に永続化し
  （`session.rs`）、再起動時に `tick(now - anchor)` で kill/Doze/画面オフ中の経過を取り戻す（ADR-0002 §3 / option B）。
  残る限界: 裏に居る間のリアルタイム通知音は保証されない（復帰時の状態は正しい）。要実機検証。
- `.claude/` は `.gitignore` 済み（公開リポに init 足場を含めない）。
