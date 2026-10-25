# livstyle

livstyle 是一个可验证的硬件开发智能体工作台，基于 Makepad 与 Octos。
首个交付目标是在 Linux 桌面上完成 ESP32 系列（首批 S3、C3、C5、C6、P4 与经典 ESP32）的
开发、构建、烧录与板上验证闭环，执行中可随时打断和修改需求；验收板卡为 S3 与 C5。
RK3588、Windows、Android 以及通用超级智能体能力分阶段扩展。

- [产品需求文档 v0.3.1](docs/PRD.md)（归档：[v0.1](docs/archive/PRD-v0.1.md)、[v0.2](docs/archive/PRD-v0.2.md)）
- [技术设计 v0.3](docs/TECHNICAL_DESIGN.md)（含 UI 线框）
- [架构决策记录](docs/adr/)
- [实施路线图](docs/ROADMAP.md)

## 设计原则

1. Makepad 客户端负责交互，不复制 Octos 的会话与执行内核。
2. 产品级任务、计划和干预记录由 livstyle Product Service 唯一持有；Octos 负责原生 Agent 执行、记忆和工具。
3. 物理设备由 Hardware Supervisor 授权、由受限的本地或远程 Runner 执行。
4. Cursor 等外部系统是 Agent Runtime，不伪装成普通 LLM API。
5. 芯片和板卡差异放在声明式 Chip / Board Profile 中，不在代码里按型号分支。
6. 运行中干预由 Jev 方法做快速分类，Judge 只给建议，不授权硬件操作。
7. RSI 只允许在评测、审批、签名和回滚约束下演进。
8. Ontology RAG 先做有来源、有版本的硬件知识记录，图推理后置。

ESP32 闭环的工作流参考了 [EspSmith](https://github.com/fangkuaiLS/EspSmith)，对应关系见 PRD §2.3。

## 代码结构

| 路径 | 内容 |
|---|---|
| `crates/livstyle-domain` | 任务状态机、计划依赖图与失效计算、风险与验证等级 |
| `crates/livstyle-profiles` | Chip / Board Profile 模型、加载与校验 |
| `crates/livstyle-hardware` | Supervisor、租约与 fencing token、审批摘要、Runner 契约、Mock Runner |
| `crates/livstyle-steering` | 运行中干预：记录队列、Judge 接口、规则判断、处理决策 |
| `crates/livstyle-judge` | Jev 方法的 logprob Judge（Ollama / OpenAI 兼容后端）与评测 |
| `crates/livstyle-cursor` | Cursor SDK Bridge 生命周期与 `sdk.v1` 客户端 |
| `crates/livstyle-store` | SQLite 持久化、事件日志、审计哈希链、崩溃恢复 |
| `crates/livstyle-octos` | Octos OUP 客户端（loopback WebSocket） |
| `crates/livstyle-core` | 产品服务：任务、干预落库、崩溃恢复 |
| `crates/livstyle-workspace` | 每个 Run 一个 git worktree 与写锁 |
| `crates/livstyle-esp` | ESP-IDF 工具链探测、USB 串口发现、esptool Runner |
| `crates/livstyle-secrets`、`livstyle-http` | 凭据存储；仅限 loopback 的 HTTP 客户端 |
| `crates/livstyle-cli` | `livstyle` 命令行 |
| `crates/livstyle-app` | Makepad 工作台 |
| `profiles/` | 芯片与板卡 Profile（`draft`，实板核对后改为 `verified`） |
| `evals/` | 干预分类样本集与评测结果 |
| `scripts/fetch-cursor-bridge.sh` | 下载并校验锁定版本的 Cursor SDK Bridge |

## 快速开始（Linux）

```sh
cargo test --workspace

# Cursor SDK Bridge（哈希写死在脚本中）
scripts/fetch-cursor-bridge.sh
cargo run -p livstyle-cli -- cursor smoke --offline

# Cursor API Key：编辑仓库根目录 .env，把 key 写在 CURSOR_API_KEY= 后面
cargo run -p livstyle-cli -- auth status
cargo run -p livstyle-cli -- cursor smoke

# ESP32 内循环（Toit/Jaguar：底座已在板上时，应用更新不走 USB）
cargo run -p livstyle-cli -- iterate --change app --base ready

# Jev 干预分类评测（本地 Ollama）
cargo run -p livstyle-cli -- judge eval --backend rule
cargo run -p livstyle-cli -- judge eval --backend ollama --out evals/results/mine.json


cargo run -p octos-cli --features api --bin octos -- serve   --host 127.0.0.1 --port 50080   --cwd "$PWD"   --data-dir /tmp/octoscode-dev-data   --auth-token "123456"   --solo

# Makepad 工作台
cargo run -p livstyle-app

# Octos OUP（本机 octos serve 已在跑时）
cargo run -p livstyle-cli -- octos probe

# ESP-IDF 与设备
cargo run -p livstyle-cli -- toolchain
cargo run -p livstyle-cli -- devices
cargo run -p livstyle-cli -- chip-info /dev/ttyACM0   # 会复位开发板，需确认
```

## 平台分期

- MVP-0/1：Linux 桌面，完整开发与设备执行平面
- MVP-2：Windows 桌面客户端和原生 Runner
- MVP-3：Android 远程客户端（会话、干预、审批、证据浏览），硬件执行在桌面或服务器 Runner 上

macOS、iOS 和 WebAssembly 保留 Makepad UI 适配能力，但不属于首个硬件闭环的
验收平台。
