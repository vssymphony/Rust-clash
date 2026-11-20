<!--
提交前请确认：
1. 标题遵循 Conventional Commits，如：feat(tun): 支持 wintun 缺失时降级
2. 已填写下方全部章节，不适用请写「不适用」并说明原因
3. CI（ci.yml 七个 job）全绿后再请求评审
-->

## 变更说明

<!-- 这个 PR 做了什么、为什么做。关联 issue 请写 Fixes #123 -->

## 变更类型

- [ ] feat：新功能
- [ ] fix：缺陷修复
- [ ] refactor / perf：重构或性能优化（无行为变化）
- [ ] docs：仅文档
- [ ] test：仅测试
- [ ] chore / ci：构建、依赖或工作流

## 影响范围

- [ ] 内核进程管理（`core`）
- [ ] 系统代理 / TUN（`proxy` / `tun`）
- [ ] 订阅与节点（`sub` / `nodes`）
- [ ] 前端界面（`ui`）
- [ ] 权限配置（`capabilities`）
- [ ] CI / 发布流程
- [ ] 仅文档

## 验证方式

<!-- 贴出实际执行的命令与结果，不要只写「已测试」 -->

```bash
npm run verify          # 结果：
npm run format:check    # 结果：
npm run version:check   # 结果：
npm run rust:clippy     # 结果：
npm run rust:test       # 结果：
```

受限环境说明（无 Rust 工具链等）：

<!-- 例如：本机 Smart App Control 禁止 cargo，Rust 侧依赖 CI 验证 -->

## 安全自查（必填）

- [ ] 无硬编码密钥、订阅链接、token、UUID、私钥
- [ ] 日志 / 错误信息 / 前端展示已脱敏（订阅地址、`secret`、节点 UUID）
- [ ] 新增文件写入走安全写（临时文件 + 原子替换），无直接覆盖用户配置
- [ ] 新增 / 修改 `#[tauri::command]` 已做参数校验，前端调用名与 `generate_handler!` 一致
- [ ] `capabilities` 权限保持最小化，未新增通配权限
- [ ] 未提交 `mihomo*` 二进制、`wintun.dll`、用户配置与日志
- [ ] 新增依赖已在 `deny.toml` 许可证白名单内，`cargo audit` 无新增告警

## 兼容性

- [ ] 不破坏已有用户配置字段语义（如 `tunEnabled`、代理模式、订阅缓存）
- [ ] Windows / Linux 行为差异已在下方说明
- [ ] 不涉及数据迁移；如涉及，已写清迁移与回滚方式

## 文档与版本

- [ ] 已更新 `CHANGELOG.md` 的 `[Unreleased]`（用户可见变更必填）
- [ ] 已同步更新相关 `README.md` / `docs/*`
- [ ] 未擅自修改版本号（发版由维护者统一执行 `npm run version:set`）
