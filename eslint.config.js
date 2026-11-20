// ESLint 9 Flat Config。
//
// 目标：与 `cargo clippy -D warnings` 对齐的零容忍策略——CI 里 `eslint . --max-warnings 0`
// 直接失败。规则取舍只保留能抓住真实缺陷的项，不引入需要额外维护的格式类规则
// （格式交给 Prettier，避免两套工具互相打架）。
import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";

export default tseslint.config(
  {
    ignores: [
      "dist/**",
      "coverage/**",
      "node_modules/**",
      // Rust 侧与构建产物不由 ESLint 处理
      "src-tauri/**",
      "scripts/**",
    ],
  },
  {
    files: ["**/*.{ts,tsx}"],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: {
      ecmaVersion: 2022,
      sourceType: "module",
      globals: {
        ...globals.browser,
        ...globals.es2021,
      },
    },
    plugins: {
      "react-hooks": reactHooks,
      "react-refresh": reactRefresh,
    },
    rules: {
      // 只启用 React Hooks 的两条经典规则。
      // 不整体铺开 `react-hooks.configs.recommended`：v7 起该预设包含 React Compiler
      // 的「纯度/记忆化」规则集，本项目未启用 React Compiler，强行开启会要求把现有
      // 的 derived state 与 useMemo 全量重构，属于风格迁移而非缺陷修复。
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "error",
      // Fast Refresh 只影响开发体验，且 `ui/*` 组件文件天然会同时导出多个组件；
      // 交给开发时观察，不进 CI 门禁。
      "react-refresh/only-export-components": "off",
      // 未使用变量一律报错，`_` 前缀显式声明「有意忽略」
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_", caughtErrorsIgnorePattern: "^_" },
      ],
      // 误用 any 会绕过类型检查，但不阻断（第三方类型缺失时仍有必要）
      "@typescript-eslint/no-explicit-any": "warn",
      // 日志统一走 toast/控制台；console.log 不进入生产代码
      "no-console": ["warn", { allow: ["warn", "error"] }],
      eqeqeq: ["error", "smart"],
      "no-var": "error",
      "prefer-const": "error",
    },
  },
  {
    // 测试文件与 Node 侧脚本可以使用 Node 全局量与 Vitest 全局 API
    files: ["**/*.{test,spec}.{ts,tsx}", "src/test/**/*.{ts,tsx}", "*.config.ts"],
    languageOptions: {
      globals: {
        ...globals.node,
      },
    },
    rules: {
      "no-console": "off",
    },
  },
);
