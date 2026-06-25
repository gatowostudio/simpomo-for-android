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

> `tauri android init` は `src-tauri/gen/android` を生成する（既定で `.gitignore` 済み）。
> **landscape 固定・immersive 全画面・通知権限**などで AndroidManifest を手編集して保持したくなったら、
> `src-tauri/gen/android` の該当ファイルだけ追跡対象に戻す（`.gitignore` のコメント参照）。
> 可能なら Tauri の設定/プラグイン側で実現し、生成物の手編集は最小化する方針。

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

## 署名（リリース）

APK はインストールに署名が必須。**リリース用キーストアはリポジトリに絶対コミットしない**（公開リポ）。

1. キーストアを作成（リポジトリ外に保存）:
   ```sh
   keytool -genkey -v -keystore simpomo-release.jks -keyalg RSA -keysize 2048 -validity 10000 -alias simpomo
   ```
2. 資格情報は `keystore.properties`（`.gitignore` 済み）や環境変数 / GitHub Secrets で渡す。鍵そのものはファイルに残さない。
3. Gradle の署名設定で release ビルドに適用する（詳細は実装時に確定）。

## リリース（GitHub Releases 配布）

- 配布は **GitHub Releases に APK（必要なら AAB）を添付**する。Play ストア配布は当面想定しない
  （利用者は「提供元不明アプリ / 不明なアプリのインストール」を許可して APK を入れる）。
- CI（GitHub Actions）でタグ push → 署名付き APK をビルドして Release 添付、という流れを想定するが、
  **ワークフローはまだ作らない**（project-init では生成しない方針）。キーストアは Secrets に置く。
- バージョンは `package.json` を正とし、Android の `versionName` / `versionCode` を揃える運用にする（実装時に確定）。

## テスト

```sh
cd src-tauri
cargo test                   # コアロジック（タイマー進行・サイクル遷移）の単体テスト（デスクトップ版から流用）
```

テスト方針は CLAUDE.md「テスト方針」を正とする（コアロジックのみ単体テスト、UI/音/全画面は実機で手動確認）。

## アイコン

デスクトップ版の自作トマト `app-icon.svg` を source に Android 用アイコンを生成:

```sh
pnpm tauri icon path/to/app-icon.svg     # android/ 用も生成される
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
