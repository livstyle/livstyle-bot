# Cursor Bridge 实施记录

日期：2026-10-25。依据 `docs/plan1.md`。Cursor 与 Octos 并列，不改 Octos 内部循环。

## 真实位置

- 协议适配：`crates/livstyle-cursor`（`bridge.rs`、`transport.rs`、`client.rs`、`stream.rs`、`errors.rs`、`ready.rs`）。
- 产品层选择：`crates/livstyle-app/src/worker.rs` 的 `CodeAgent`。`ask_octos` 与 `ask_cursor` 分开。失败不换后端。
- UI 事件：worker 线程把 `RunEvent` 收成 `TurnBeat`，经 `Snapshot` 回到 Makepad。UI 线程不读 Bridge。
- 密钥：`livstyle-secrets` 的 `CURSOR_API_KEY`。Bridge bearer token 只留在 `Endpoint` 内存里。

## 固定版本

`PINNED_BRIDGE_VERSION = v1.0.38`，协议 `sdk.v1`。RPC 前缀 `sdk.v1.{Service}/{Method}`，HTTP/1.1 Connect。方法名来自该版本的官方服务：`SdkBridgeControlService`、`SdkAgentService`、`SdkCursorService`。安装脚本 `scripts/fetch-cursor-bridge.sh`。

## 平台

- Linux：本机子进程。启动前校验 `manifest.json` 的 `protocol == sdk.v1`。`LIVSTYLE_CURSOR_BRIDGE_BIN` 必须是已存在的文件。
- Windows：同一套代码按 `.exe` 路径查找。本次没有在 Windows 上编译或启动 Bridge。
- Android：`locate_bridge` 直接返回不可用。不把桌面 Linux 二进制当成 Android 支持。Octos 路径不依赖 Bridge。

## 安全

烧录、识别芯片走 `livstyle-esp` 的产品命令和确认卡，不注册成 Cursor 工具。Cursor 的 shell 由官方 Bridge 执行；livstyle 没有把它包成可烧录的工具。Bridge 自带 Bun，不是纯 Rust 进程。计费以 Cursor 账户对 API key 的实际额度为准，不把 IDE 订阅写成必然包含 SDK 用量。

## 未在本次验证

没有 `CURSOR_API_KEY` 时不跑真实 CreateAgent / Send。`cargo test -p livstyle-cursor` 覆盖协议解析、错误映射和 manifest 校验。
