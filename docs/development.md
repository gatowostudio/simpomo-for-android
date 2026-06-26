# 開発手順 — simpomo-for-android

Tauri v2（Rust）mobile / **Android** + Svelte（Vite / TypeScript）構成の開発メモ。
タイマーコア・設定・音は デスクトップ版 [simpomo](https://github.com/gatowostudio/simpomo) から流用する
（流用方針は [`decisions/0001-android-stack-and-reuse.md`](decisions/0001-android-stack-and-reuse.md)）。

> 状態: **足場段階**。下記コマンドは Tauri v2 mobile の標準手順に基づく想定。実装着手で確定したら更新する。

## 前提ツール

- **Rust** ツールチェイン（`rustup` 安定版）+ `cargo`
- Rust の **Android ターゲット**:
  ```sh
  rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
  ```
- **Node.js**（LTS）+ **pnpm**
- **Android Studio**（SDK Manager 経由で）:
  - Android SDK（Platform + Build-Tools）
  - **NDK**（Side by side）
  - （実機が無ければ）Android Emulator + システムイメージ
- **JDK 17**
- 環境変数: `ANDROID_HOME`（SDK パス）、`NDK_HOME`（NDK パス）、`JAVA_HOME`
- 参照: https://v2.tauri.app/start/prerequisites/ ／ https://v2.tauri.app/develop/

## セットアップ

```sh
pnpm install                 # フロントエンド依存
pnpm tauri android init      # gen/android（Gradle プロジェクト）を生成
```

> `tauri android init` は `src-tauri/gen/android`（Gradle プロジェクト）を生成する。本リポは**これを追跡
> コミットする方針**（landscape 固定・immersive 全画面・署名設定の手当てをビルド/CI に確実に乗せるため。
> `.gitignore` は成果物・ローカル設定・スキーマ・署名資格情報だけ除外する）。一度きりの手当て手順は下の
> 「Android ターゲットの一度きりセットアップ」を参照。可能な範囲は Tauri 設定/プラグインで実現し、手編集は最小化する。

## 開発（実機 / エミュレータで起動）

```sh
pnpm tauri android dev       # Vite + Android アプリをデバッグ起動（ホットリロード）
```

実機の場合は USB デバッグを有効化して接続、エミュレータの場合は AVD を起動しておく。

## ビルド（配布物の生成）

```sh
pnpm tauri android build               # リリースビルド（既定で APK + AAB）
# 例: APK だけ / 特定 ABI だけに絞る場合
# pnpm tauri android build --apk --target aarch64
```

生成物は `src-tauri/gen/android/app/build/outputs/` 配下（`apk/` / `bundle/`）に出る。

## Android ターゲットの一度きりセットアップ（init 後の手当て）

`tauri android init` 後、横画面固定・全画面・署名を効かせるために生成物へ手当てを当て、**`gen/android` を
追跡コミットする**（成果物・ローカル設定・署名資格情報は `.gitignore` 済み）。これで CI は init も gradle の
正規表現 patch も不要になり、手当てがそのままビルドに乗る（脆い生成テンプレ依存を排除）。当てたら実機で確認する
（issue #2 / #3 / #6）。

一度きりの手順:

1. `pnpm tauri android init` で `gen/android` を生成。
2. **landscape 固定**: `gen/android/app/src/main/AndroidManifest.xml` の `<activity>` に
   `android:screenOrientation="landscape"` を付ける。
3. **immersive 全画面（システムバー非表示）**: Tauri 設定では出せないため Activity 側で行う。生成された
   `MainActivity.kt` で `WindowCompat.setDecorFitsSystemWindows(window, false)` ＋
   `WindowInsetsControllerCompat(window, ...).hide(systemBars())`（sticky immersive）を設定し、テーマを
   ノーアクションバー/透過バーにする。**生成テンプレートに合わせて要調整**（実機確認は issue #2）。
4. **release 署名**: `gen/android/app/build.gradle.kts` に `signingConfigs.release` を追加し、
   `rootProject.file("keystore.properties")` から `storeFile`/`keyAlias`/`keyPassword`/`storePassword` を
   読んで release buildType に適用する。`keystore.properties` 自体は `.gitignore` 済み（CI は Secrets から生成）。
5. 上記を入れた `gen/android` をコミット。**画面常時ON** はフロントの W3C Screen Wake Lock
   （`src/lib/wakelock.ts`）で実装済み（**暫定・実機検証待ち**。未対応/Doze 解放で実用に足りなければ native
   `window.addFlags(FLAG_KEEP_SCREEN_ON)` を MainActivity に追加してフォールバック）。
6. **通知権限（POST_NOTIFICATIONS, Android 13+）**: `tauri-plugin-notification` がマージ後マニフェストに権限を
   追加するはず。実行時要求はフロント実装済み（`ensureNotificationPermission`：設定 ON 時／有効状態の起動時に要求）。
   マージ後マニフェストに権限が在ること＋実機で許可ダイアログが出ることを確認する（issue #3）。

## 署名（リリース）

APK はインストールに署名が必須。**リリース用キーストアはリポジトリに絶対コミットしない**（公開リポ）。

1. キーストアを作成（リポジトリ外に保存）:
   ```sh
   keytool -genkey -v -keystore simpomo-release.jks -keyalg RSA -keysize 2048 -validity 10000 -alias simpomo
   ```
2. GitHub Secrets に登録（CI 用。`.github/workflows/android-release.yml` が参照する）:
   - `ANDROID_KEYSTORE_BASE64`（base64 化した1行。Git Bash: `base64 -w0 simpomo-release.jks` ／
     PowerShell: `[Convert]::ToBase64String([IO.File]::ReadAllBytes("simpomo-release.jks"))`。
     ※ `certutil -encode` は PEM ヘッダ/CRLF が混ざり `base64 -d` で壊れるので使わない）
   - `ANDROID_KEY_ALIAS` / `ANDROID_KEY_PASSWORD` / `ANDROID_STORE_PASSWORD`
3. ローカルで署名ビルドする場合は `gen/android/keystore.properties`（`.gitignore` 済み）に
   `storeFile` / `keyAlias` / `keyPassword` / `storePassword` を書く（上の手当て 4 の `signingConfigs` が読む）。

## リリース（GitHub Releases 配布）

- 配布は **GitHub Releases に APK を添付**する。Play ストア配布は当面想定しないので AAB は作らない
  （利用者は「提供元不明アプリ / 不明なアプリのインストール」を許可して APK を入れる）。
- CI: タグ `vX.Y.Z` を push すると `.github/workflows/android-release.yml` が（追跡済み `gen/android` を使って）
  署名付き APK をビルドし、Release を**下書き**で作る（確認後に手動 publish）。⚠️ このワークフローは
  **実 CI 実行で未検証**（issue #6）。前提として上記「一度きりセットアップ」と Secrets 登録が先に要る。
- バージョンは `package.json` を正とし、ワークフローの `verify` job がタグと一致を確認する。Android の
  `versionName` / `versionCode` は Tauri が `package.json` から導出する。

## テスト

```sh
cd src-tauri
cargo test                   # コアロジック（タイマー進行・サイクル遷移）の単体テスト（デスクトップ版から流用）
```

テスト方針は CLAUDE.md「テスト方針」を正とする（コアロジックのみ単体テスト、UI/音/全画面は実機で手動確認）。

加えて、push 前に **`pwsh scripts/check-core-drift.ps1`**（または `powershell -File ...`）を回し、desktop 版と
共有するミラーのドリフトが無いことを確認する（ローカルに `C:\Dev\simpomo` が在る前提。詳細は
`decisions/0003-core-duplication-sync.md`）。手動実行を忘れないよう、一度だけ
**`git config core.hooksPath scripts/hooks`** を設定すると、`scripts/hooks/pre-push` が push のたびに
検査を走らせ、ミラーがドリフトしていれば push をブロックする（desktop リポが無い環境では警告のみで通す）。

## 実機検証チェックリスト（toolchain 導入後）

エミュレータ/実機で手動確認する。特にバックグラウンド計時（ADR-0002 §3）は単体テストで担保できないので必須
（issue #4）。

- [ ] **表示**: 横画面固定・全画面（システムバー非表示）・回転しても崩れない（issue #2）。
- [ ] **画面常時ON**: 稼働中は画面が消えない／一時停止・停止で通常のスリープに戻る（`wakelock.ts`）。
- [ ] **音**: 通知音3種・BGM6種が鳴る。自動開始時は1タップで AudioContext が resume する。
- [ ] **通知**: OS 通知 ON で許可ダイアログ→許可。別アプリへ退いた瞬間にフェーズ境界通知が出る（issue #3）。
- [ ] **計時の正確さ（最重要・issue #4）**:
  - [ ] 稼働中に**画面オフ→点灯**（プロセス生存）で、残り時間が経過ぶん正しく減っている。
  - [ ] 稼働中に**アプリ kill→再起動**で、`session.json` から復元し経過ぶん減っている（必要なら境界も跨ぐ）。
  - [ ] 復帰時の catch-up では通知音が鳴らず統計も増えない（水増し防止）。
  - [ ] 一時停止状態で kill→再起動すると、残り時間そのままで停止状態が復元される。

## アイコン

デスクトップ版から流用した自作トマト `src-tauri/icons/app-icon.svg` を source に各サイズを生成する。
ベースのアイコン（`32x32.png` 等）は流用済みだが、Android の mipmap は `tauri android init` 時に
生成されるため、source を更新したら再生成する（issue #5）:

```sh
pnpm tauri icon src-tauri/icons/app-icon.svg     # android/ 用 mipmap も生成される
```

## ディレクトリ構成（想定）

```
simpomo-for-android/
├─ src/              # Svelte フロントエンド（UI）— 単一画面・横向きに作り直して流用
├─ src-tauri/        # Rust（タイマーコアを流用、デスクトップ専用は除去）
│  ├─ src/           # timer.rs（流用）, settings.rs（流用）, stats.rs（流用）など
│  └─ gen/android/   # tauri android init で生成（既定で gitignore）
├─ docs/             # 仕様・設計判断
└─ ...
```

## 公開リポジトリでの注意（機密の扱い）

- **リリース署名キーストア（`*.jks` / `*.keystore`）・`keystore.properties`・`.env` は絶対にコミットしない**（`.gitignore` 済み）。
- 鍵はリポジトリ外（端末ローカル / GitHub Secrets）に置き、ワークフローの env で渡す。
- 同梱する音は現状なし（Web Audio 合成）。将来音源を同梱する場合はロイヤリティフリー/自作のみ、出典を記録する。
