/// <reference types="vitest" />
// Vitest 配置：复用 Vite 的 alias / 插件（`@` → `src`），只补测试专属选项。
// 独立成文件而不是塞进 vite.config.ts，避免把测试依赖带进生产构建的类型检查。
import { defineConfig, mergeConfig } from "vitest/config";
import viteConfig from "./vite.config";

export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      // 设置页与 api 封装都依赖 DOM
      environment: "jsdom",
      // 允许 `describe/it/expect` 免 import（tsconfig 已引入 vitest/globals）
      globals: true,
      setupFiles: ["./src/test/setup.ts"],
      include: ["src/**/*.{test,spec}.{ts,tsx}"],
      // 组件测试不关心 CSS 变量，跳过样式解析以提速
      css: false,
      clearMocks: true,
      restoreMocks: true,
      coverage: {
        provider: "v8",
        reporter: ["text", "lcov"],
        include: ["src/lib/**/*.{ts,tsx}", "src/store/**/*.{ts,tsx}", "src/hooks/**/*.{ts,tsx}"],
        exclude: ["src/lib/types.ts", "**/*.test.{ts,tsx}"],
      },
    },
  }),
);
