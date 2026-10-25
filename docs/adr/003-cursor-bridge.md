# ADR-003：Cursor SDK Bridge 版本锁定、认证与生命周期

**状态：** Accepted · 2026-10-18

## 背景

Cursor 通过官方 [cursor/sdk-bridge](https://github.com/cursor/sdk-bridge) 暴露 `sdk.v1`（Connect 协议）。Bridge 是独立二进制（Bun 打包），协议与能力随版本变化。

## 决策

1. **版本：** 锁定 `v1.0.38`（bridgeVersion 1.0.0，protocol `sdk.v1`）。各平台归档的 SHA-256 写死在 `scripts/fetch-cursor-bridge.sh`；安装到 `$XDG_DATA_HOME/livstyle/cursor-sdk-bridge/v1.0.38/`；启动前校验 `manifest.json` 的 protocol。
2. **生命周期：** livstyle 按需拉起 Bridge 子进程，等待 stderr 的 ready 行（30 s 超时），读取 token 文件；退出时发 `Shutdown`，5 s 后强杀。Bridge 只监听 loopback；livstyle 的 HTTP 客户端拒绝非 loopback 目标。
3. **认证：** Cursor API Key 由用户通过 `livstyle auth cursor`（隐藏输入）或 UI 密码框录入，保存于 `livstyle-secrets`；Key 在每个接受它的 RPC 中显式传入，同时注入 Bridge 环境变量 `CURSOR_API_KEY`。Bridge bearer token 与 Key 分开，只在内存中。
4. **错误区分：** 无 details 的 `unauthenticated` = Bridge token 问题；带 `SdkErrorDetails` 的认证 / 额度 / 限流错误 = Cursor 账户问题，分别提示。认证失败或额度耗尽时明确失败，不静默切换 Runtime。
5. **能力：** 以 `GetVersion.capabilities` 为准。v1.0.38 无运行中注入，`RuntimeCaps { steer: false, interrupt: true }`。
6. **恢复：** 只持久化 `ObserveRun` 流的 offset；`Send` 流的 offset 不用于恢复。

## 后果

- 升级 Bridge 需要更新脚本中的哈希、`PINNED_BRIDGE_VERSION` 并重跑 Spike 1；
- `append_context` 在 Cursor 上只能排队到下一次 Send，UI 需明示“将在本轮结束后送达”。

## 验证

- 已通过（无 Key）：`crates/livstyle-cursor/tests/bridge_live.rs`，`livstyle cursor smoke --offline`；
- 待用户 Key：`livstyle cursor smoke`（Me、ListModels、CreateAgent、Send 流、CancelRun、ObserveRun 续看、CloseAgent、Shutdown）。
