// Vitest 全局初始化：注入 jest-dom 断言、每个用例后清理 DOM 与 Tauri 运行时标记。
import "@testing-library/jest-dom/vitest";
import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

afterEach(() => {
  cleanup();
  // `isTauriRuntime()` 依赖该标记，测试之间必须复位，否则用例相互污染
  delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
});
