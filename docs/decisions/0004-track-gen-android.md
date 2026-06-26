# ADR-0004: 生成 Android プロジェクト（gen/android）を追跡コミットする

- ステータス: 採用
- 日付: 2026-06-26
- 決定者: 作者（gatowostudio / dekka）／参謀システム（issue #2 / #6、devil 指摘起点）
- 関連: ADR-0001（Tauri 流用・2リポ）, ADR-0003（コア重複の同期戦略）

## 背景

`tauri android init` が生成する `src-tauri/gen/android`（Gradle プロジェクト）に、Android 固有の手当てを当てる必要がある:
- **landscape 固定**（`AndroidManifest.xml`）
- **immersive 全画面**（`MainActivity.kt`）
- **release 署名**（`build.gradle.kts` の `signingConfigs`）
- **POST_NOTIFICATIONS**（`AndroidManifest.xml`）

これらは Tauri の設定/プラグインでは出せず、生成物の編集が要る。問題は「生成物をリポジトリでどう扱うか」。

## 決定

**`gen/android` を追跡コミットする**（ソース一式。ビルド成果物・ローカル設定・署名資格情報・Tauri が毎ビルド
再生成する派生物は `.gitignore` で除外）。実際にステージされるのは 42 ファイル程度（manifest / MainActivity /
build.gradle.kts / res・mipmap / gradle wrapper / buildSrc 等）。除外の主担は **Tauri 生成の `gen/android/app/.gitignore`**
（`assets/`・`generated/`・`tauri.build.gradle.kts`・`tauri.properties` 等を無視）＋ 本リポ `.gitignore`
（`build/`・`.gradle/`・`local.properties`・`keystore.properties` 等）。

理由: 手当てを**そのままビルドと CI に乗せられる**。CI は `tauri android init` も「生成 `build.gradle.kts` を
正規表現で patch する」脆い処理も不要になり、`keystore.properties` を Secrets から書くだけで署名 APK を出せる
（`.github/workflows/android-release.yml`）。実機で debug / release(minified) 双方の起動・横画面・immersive・
署名（apksigner で V2 確認）を検証済み。

## 検討した代替案

- **(A) 追跡しない＋CI で毎回 init し正規表現 patch**: 生成テンプレ依存で脆く、landscape/immersive を当て損なうと
  **DoD 非準拠の APK**（縦・バー有り）が出る。却下（当初案。devil 指摘で反転）。
- **(B) オーバーレイ追跡（手当てした少数ファイルだけ追跡し、CI で `tauri android init` 後に上書きコピー）**:
  生成ツリー全体を抱えずに済み、再生成陳腐化の面積が小さい。正規表現 patch でなくファイル差し替えなので脆くない。
  ただし CI に「init→オーバーレイ適用」の段が増え、オーバーレイ対象の取りこぼし（新たに手当てが要るファイルが
  増えた時）を検知する仕組みが別途要る。**有力だが、(C) を実機で検証済みのため今回は採らない**（将来 Tauri 更新で
  再生成衝突が頻発したら (B) へ移行を再検討）。
- **(C) 生成ツリーを追跡（採用）**: 最も単純で、手当てが確実にビルドに乗る。代償＝生成物をリポジトリに抱え、
  再 init で手当てと競合しうる（下記リスク）。

## リスクと運用ルール（再生成ドリフト）

ADR-0003 のコア重複と**同型の沈黙ドリフト**が Android 側にも生じる: Tauri/プラグインを更新すると生成テンプレが
変わるのに、追跡済みコピーは古いまま固まる。検知の自動機構は今は無い（ADR-0003 のような専用スクリプトは未整備）。
これを承知の上で、軽量な運用ルールで凌ぐ:

1. **gen/android を追跡している間は `tauri android init` を不用意に再実行しない**（手当てが静かに上書きされる）。
2. **Tauri / `tauri-plugin-*` / `@tauri-apps/cli` を更新したら**、scratch ディレクトリで `tauri android init` し直し、
   生成物を committed `gen/android` と **diff して差分を照合・取り込む**（特に `buildSrc`・gradle wrapper・
   `app/.gitignore`・`generated/` 周りの版差）。手当て（manifest / MainActivity / build.gradle.kts）は維持する。
3. 手当てしたファイルには **issue マーカー（`#2`/`#3`/`#6`）** を残し、`MainActivity.kt` 冒頭に手当てバナーを置く
   （後任が「生成のどこを触ったか」を grep で辿れるように）。
4. 再現レシピは `docs/development.md`「Android ターゲットの手当て」を参照（本 ADR と一致させる）。

確実な強制が要るようになったら、(a) 上記 diff を CI 化（fresh init 出力と committed の比較を警告）、または
代替案 (B) のオーバーレイ方式へ移行する。それまでは**手順＋規律**で運用する。

## 限界

- 再生成ドリフトの検知は手動（上記運用ルール頼み）。沈黙ドリフトを完全には防げない（ADR-0003 と同じ割り切り）。
- 署名は CI 実走（実鍵・Secrets・タグ push）が未検証（issue #6）。ローカルではテストキーで署名・起動を確認済み。
