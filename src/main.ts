import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";

// Android 版は単一画面（ADR-0001）。デスクトップ版の「ウィンドウラベルで main/settings を出し分ける」
// 分岐は廃止し、App だけをマウントする（設定は App 内のビューで開く）。
const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
