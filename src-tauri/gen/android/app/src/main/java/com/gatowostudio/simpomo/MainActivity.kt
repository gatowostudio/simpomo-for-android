package com.gatowostudio.simpomo

// ⚠️ 手当て済みファイル（#2 immersive 全画面）。Tauri 生成テンプレは「TauriActivity を継承する数行」のみで、
//    下記 import / onCreate / onWindowFocusChanged / hideSystemBars は**すべて手で追加**したもの。
//    gen/android を追跡コミットしているのはこの手当てをビルド/CI に確実に乗せるため（理由・運用 → ADR-0004）。
//    **gen/android 追跡後は `tauri android init` を再実行しない**（この手当てが静かに上書きされる）。
//    Tauri/プラグイン更新時は ADR-0004 の「再 init→差分照合」手順に従うこと。
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // immersive 全画面（#2）: ステータス/ナビゲーションバーを隠す。卓上で残り時間を大きく見せるため。
    hideSystemBars()
  }

  override fun onWindowFocusChanged(hasFocus: Boolean) {
    super.onWindowFocusChanged(hasFocus)
    // #2: ダイアログ/最近のアプリ等から戻るとバーが復活するので、フォーカス回復時に隠し直す。
    if (hasFocus) hideSystemBars()
  }

  // #2: システムバーを隠し、画面端スワイプで一時的に出せる（sticky immersive）挙動にする。
  private fun hideSystemBars() {
    val controller = WindowInsetsControllerCompat(window, window.decorView)
    controller.hide(WindowInsetsCompat.Type.systemBars())
    controller.systemBarsBehavior =
      WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
  }
}
