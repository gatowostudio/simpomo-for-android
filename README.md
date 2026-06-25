# simpomo for Android

横画面（landscape）・全画面で使う、シンプルで軽量なポモドーロタイマー。
デスクトップ版 [simpomo](https://github.com/gatowostudio/simpomo) の Android 移植版です。

作業→休憩を1セットとして繰り返し、フェーズで背景色が変わるので、音が無くても今が作業中か休憩中か一目で分かります。
卓上スタンドなどに横向きに置いて使うことを想定しています。

## ダウンロード

最新の APK → **[GitHub Releases（latest）](https://github.com/gatowostudio/simpomo-for-android/releases/latest)**

- APK を端末にインストールするには「提供元不明のアプリ / 不明なアプリのインストール」を許可する必要があります。
- Play ストアでは配布していません（GitHub Releases の APK 直接配布）。

ソースからのビルドは [`docs/development.md`](docs/development.md) を参照してください。

## 使い方

| 操作 | 説明 |
|------|------|
| **Start / Pause** | 計測の開始・一時停止 |
| **⟲** | 最初（作業フェーズの先頭）へリセット |
| **⏭** | 現在のフェーズを飛ばして次へ |
| **⚙** | 設定を開く（同じ画面内で切り替え） |

### サイクル数（自動継続するセット数）

- **0（既定）**: 作業→休憩を1セット実行したら停止（次は Start を押すまで待機）
- **有限 N**: N セットを自動で連続実行して停止
- **無限（Loop forever）**: 止めるまで自動で継続

## 設定（⚙）

変更は自動保存されます。デスクトップ版から「持っていける」設定を移植しています:

- 作業 / 休憩の時間（分）、サイクル数 / 無限ループ
- 起動時にタイマー自動開始
- 作業中 / 休憩中の背景色
- 通知音（作業終了 / 休憩終了 / 完了）と音量 — Web Audio による合成音（同梱ファイルなし）
- フォーカス中の BGM（ホワイト / ピンク / ブラウンノイズ・雨・焚き火）と音量
- フェーズ境界の **OS 通知**（任意・既定 OFF）
- **完了数の統計**（完了したフォーカス / セット数。リセット可）
- 「更新を確認」— GitHub Releases の最新版があるか手動チェック

> デスクトップ版にあった「表示位置（四隅）/ トレイ常駐 / 最前面 / ウィンドウサイズ / OS スタートアップ登録」は、
> 全画面モバイルでは意味がないため移植していません（[`docs/decisions/0001-android-stack-and-reuse.md`](docs/decisions/0001-android-stack-and-reuse.md)）。

## 通信について

オフライン完結です。唯一、設定の「更新を確認」を押したときだけ GitHub の Releases API に接続します。
自動アップデートや常時通信はありません。

## ドキュメント

- 仕様: [`docs/spec.md`](docs/spec.md)
- アーキ判断（なぜ Tauri 流用 / 単一画面化）: [`docs/decisions/0001-android-stack-and-reuse.md`](docs/decisions/0001-android-stack-and-reuse.md)
- 環境構築・ビルド・リリース手順: [`docs/development.md`](docs/development.md)

## Stack

Tauri v2（Rust / mobile・Android）+ Svelte 5（Vite / TypeScript）。
タイマーの状態機械は Rust（`src-tauri/src/timer.rs`）が権威を持ち、UI は単一画面の Svelte。
音は Web Audio の合成音でクロスプラットフォーム。

## License

MIT — [`LICENSE`](LICENSE)
