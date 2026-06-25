import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// https://vite.dev/config/
// simpomo はデスクトップ専用なので、テンプレ由来のモバイル/別ホスト(TAURI_DEV_HOST)分岐は持たない。
export default defineConfig({
  plugins: [svelte()],

  // Tauri は固定ポートを前提にするので strictPort で固定する
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Rust 側の変更は cargo が監視するので Vite の監視対象から外す
      ignored: ["**/src-tauri/**"],
    },
  },
});
