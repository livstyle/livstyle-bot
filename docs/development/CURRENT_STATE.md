# livstyle 现状地图

审计日期：2026-10-25。下面的矩阵写于审计当时。其后的 Phase 0–4 已在代码里增量落地；以源代码为准。

2026-10-25 增量：取消不再调用 `send("不做了")`。空闲时的取消按钮、输入「不做了」、以及运行中的取消，都只把未结束的产品任务迁到 `Cancelled`，并停下当前 Runtime 调用。已结束的任务不会因此再开一轮。排队的下一句在取消后丢掉。

2026-10-25 对话闭环：`App::handle_actions` 把非空输入交给 `Cmd::Send`。`worker::State::send` 按 `CodeAgent` 进入 `ask_octos` 或 `ask_cursor`。Octos 在发送前用 `session_align` 核对 `octos_session_id()`，对不上就丢掉旧的 `session_id` 和 `active_turn`，再 `ensure_session`。Cursor 用 `cursor_agent_choice`：任务上已有 agent 且不是当前 cwd 下那个活 agent 时走 `resume_agent`，没有记录才 `create_agent`。事件仍经 `apply_beats` / `write_turn_line` 写回 `turn_task`。连接失败只写 `hint`，没有 mock 成功。



`design/reference/livstyle-dashboard.png` 在仓库中，作为工作台布局参考。桌面端界面是 `crates/livstyle-app/src/main.rs` 里的 Makepad 脚本：顶栏、左栏、对话、右侧任务详情。导航可以切换对应面板；知识库和插件仍标明未接入。图上的阶段打勾和百分比没有对应数据，界面不绘制。

## A. 仓库结构

Workspace：`livstyle/Cargo.toml`，edition 2024，Rust 1.85，版本 0.1.0。没有 Android target，没有独立的 Windows crate。

| Crate | 职责 | 谁调用它 |
| --- | --- | --- |
| `livstyle-domain` | 产品任务状态机 `TaskStatus`、计划节点 `NodeStatus`、干预类型 | `livstyle-core`、`livstyle-steering` |
| `livstyle-store` | SQLite schema v1：tasks、plans、interventions、operations、audit | `livstyle-core` |
| `livstyle-core` | `Service`：建任务、状态迁移、干预、审批记账、崩溃恢复 | CLI 间接、**Makepad worker** |
| `livstyle-steering` | 规则判断 `RuleJudge`，把用户句子收成干预决议 | worker `record_intervention` |
| `livstyle-judge` | 另一套判断/评测（Ollama 形状、ECE）。工作台主路径不用它 | `livstyle judge eval` |
| `livstyle-octos` | OUP WebSocket 客户端：`client_hello`、`session/open`、`turn/start`、`turn/steer`、`turn/interrupt`、`session/hydrate`、`drain` | worker、`livstyle octos probe` |
| `livstyle-cursor` | Cursor SDK Bridge `sdk.v1`：定位、拉起、Ping、GetVersion、建 Agent、Send、Observe、Cancel。固定版本 `v1.0.38` | worker、`livstyle cursor smoke`、`livstyle auth cursor` |
| `livstyle-secrets` | `.env` 里的 Cursor API Key，不从 argv 读 | cursor 启动 |
| `livstyle-http` | 只允许本机的 HTTP 客户端 | 选 Ollama 模型 |
| `livstyle-profiles` | 芯片/板卡 TOML（ESP32 族，含 C3/C5/C6/S3/P4） | CLI `profiles`，runner 准入 |
| `livstyle-hardware` | 租约、审批摘要、Supervisor、`Runner` trait、MockRunner | `livstyle-esp` 的 `EspRunner`；**UI 不调用 Supervisor** |
| `livstyle-esp` | sysfs 枚举、`identify_chip`、`EspRunner`（esptool fencing + 哈希）、IDF 探测、被动串口 `read_log` | UI 只调用 `enumerate` 和 `identify_chip`；CLI 调用 toolchain/devices/chip-info |
| `livstyle-workspace` | git worktree 与写锁 | **UI 未调用** |
| `livstyle-cli` | 无头命令 | 独立二进制 |
| `livstyle-app` | Makepad 窗口 + 后台 `worker::run` | 桌面入口 |

真实调用关系：

```
Makepad App::handle_event
  → mpsc Cmd
  → worker::State::handle          （独立线程，不在 UI 线程上跑 turn）
       → livstyle_core::Service    （SQLite 产品任务）
       → livstyle_octos::OctosClient 或 livstyle_cursor::{Bridge, CursorClient}
       → livstyle_esp::{enumerate, identify_chip}
  → Snapshot
  → App::apply                     （写 Label / TextInput）
```

`EspRunner`、`Supervisor`、`WorkspaceManager`、`read_log` 有实现和测试，但没有从 `livstyle-app` 调用。

## B. 功能矩阵

状态词：已实现且已验证 / 已实现但未验证 / 部分实现 / 仅有 UI 或 Mock / 尚未实现 / 存在缺陷。

### 1. Makepad 启动和主窗口

- 状态：已实现，本机可编译。窗口行为未做自动化测试。
- 代码：`crates/livstyle-app/src/main.rs` 的 `app_main!`、`App::handle_startup`。
- 调用：启动时 `std::thread::spawn(worker::run)`，发 `Cmd::Boot`。
- 缺陷：顶栏「任务 / 设备 / 知识库 / 插件」不能切换页面。没有移动端布局分支。

### 2. 工作目录选择

- 状态：已实现但未验证（无 UI 测试）。
- 代码：`worker::browse_workspace`、`set_workspace`、`load_workspace` / `save_workspace`。
- 调用：按钮 `browse`、`use_ws` → `Cmd::BrowseWorkspace` / `SetWorkspace`。目录必须已存在。换目录会清空 `session_id` 和 `active_turn`，再 `ensure_session`。
- 缺陷：路径存在性只在 worker 里查。对话正文不进 SQLite。

### 3. Cursor Bridge 发现与启动

- 状态：已实现。无 Key、错误 token、版本握手有单测（`livstyle-cursor`）。对真 Bridge 的 smoke 不在本次 `cargo test` 里自动算通过。
- 代码：`locate_bridge`、`Bridge::spawn`（`bridge.rs`），worker `open_cursor` / `spawn_cursor`。
- 调用：`LIVSTYLE_CURSOR_BRIDGE_BIN`，否则 `~/.local/share/livstyle/cursor-sdk-bridge/v1.0.38/bin/cursor-sdk-bridge`。manifest 的 `protocol` 必须是 `sdk.v1`。stderr 读 `cursor-sdk-bridge ready`。Drop `Bridge` 会关进程。
- 缺陷：IDE 登录不等于 Key。没有 Key 时状态是「Cursor 还没有 Key」，不会假装已认证。

### 4. Bridge 版本和认证

- 状态：部分实现。
- 代码：`CursorClient::ping`、`version`、`me`；`runtime_caps`。
- 调用：`spawn_cursor` 只检查 `version()` 的 protocol 是否为 `sdk.v1`。`me()` 没有接到启动路径。Key 来自 `CredentialStore`，经 `livstyle auth cursor` 写入。
- 缺陷：额度、账号错误要到 `send` 失败才出现在对话里。

### 5. Cursor Agent 创建和发消息

- 状态：已实现但未在本次测试中对真 Bridge 验证。
- 代码：`CursorClient::create_agent`、`send`；worker `ensure_cursor_agent`、`ask_cursor`。
- 调用：同一 `cwd` 复用 `agent_id`。`create_agent` 成功后 `Service::bind_runtime(..., "cursor", "agent", agent_id)`。
- 缺陷：`use_agent` 切到 Cursor 时 `bind_runtime` 的外部 id 写成字面量 `"desk"`，不是真实 agent id（`worker.rs` `apply_agent`）。

### 6. Cursor 流式事件

- 状态：已实现但未验证端到端。
- 代码：`RunStream`、`ask_cursor` 里把 stream 放到另一线程，150ms `recv_timeout`，`cursor_beats` / `apply_beats` 追加到内存 transcript。
- 缺陷：事件没有游标，没有落库。任务切换不改变这条流的归属，流始终写当前 `State.transcript`。

### 7. Cursor 取消、恢复、错误

- 状态：部分实现。
- 代码：`cancel_run` 在用户取消或 20 分钟超时里调用，且要求 `caps.interrupt`。`resume_agent`、`observe_run` 在 client 里存在。
- 缺陷：worker 不用 `resume_agent` / `observe_run`。进程重启后不会接着观察未完成的 run。取消错误被 `let _ =` 丢掉。

### 8. Octos OUP 连接

- 状态：已实现。单元测试 `caps_follow_advertised_methods` 通过。对本机 `octos serve` 的 `tests/live.rs` **失败**。
- 代码：`OctosClient::connect`，默认 `ws://127.0.0.1:50080/api/ui-protocol/ws`，token 默认 `123456`（`LIVSTYLE_OCTOS_URL` / `LIVSTYLE_OCTOS_TOKEN`）。
- 缺陷：见下方 P0。断线后只有下次 `ensure_session` 发现 `octos` 为 None 才重连；连接还在但会话失效时不会重建。

### 9. Octos 会话、恢复、发消息

- 状态：部分实现。
- 代码：`create_profile("livstyle")`、`open_session("livstyle:local:desk", ...)`、缺 LLM 时 `upsert_primary_llm` 指到本机 Ollama。`turn_start` / `steer` / `interrupt`。`drain` 轮询 150ms，上限 180 秒。
- 调用：`ask_octos`。`session/hydrate` 在 client 里有，worker **不调用**。
- 缺陷：会话 id 写死，不按产品任务分开。`bind_runtime` 在切换 agent 时仍写 `"desk"`。

### 10. 工作区与会话持久化

- 状态：部分实现。
- 代码：工作目录路径单独保存。产品任务在 `Service::open` 的 SQLite（`livstyle-store` schema v1）。对话 `transcript: Vec<String>` 只在进程内存，上限约 2000 行，重启即失。
- 缺陷：UI 任务列表展示的是 `ProductTask.workspace_id` 和状态，不是会话标题。种子任务在 `worker` 的 `seed` 里写成演示数据（`wifi-rssi`、`psram`）。

### 11. 多任务列表和切换

- 状态：部分实现。
- 代码：`PortalList` + `LINES`；`Cmd::Select` 只改 `selected`；`create_task` / `delete_task`。
- 缺陷：切换任务不切换 Octos session、Cursor agent，也不换 transcript。后台没有每任务一条运行环，`busy` 是整个 worker 一个布尔值。

### 12. 串口发现

- 状态：已实现。`enumerate_at` 有测试；本次未插板验收。
- 代码：`livstyle_esp::devices::enumerate` 读 sysfs 的 `ttyACM*` / `ttyUSB*`，**不开端口**。worker `refresh_boards` / `scan_reply`。
- 缺陷：只在 Linux sysfs。Windows SetupAPI 没有。UI 右栏把扫描句子当「实时日志」，不是固件构建日志。

### 13. 芯片识别

- 状态：已实现但未在本次用真板验证。
- 代码：`identify_chip` → `Toolchain::detect` → `EspRunner::read_chip_line`。UI 先弹确认卡 `CardKind::Identify`，确认后 `run_identify`。会复位开发板。CLI：`livstyle chip-info`。
- 缺陷：识别结果只写入 `board_status` 字符串，不写入设备身份记录，也不回写 Board Profile 的 verified 状态。

### 14. ESP32 构建和烧录

- 状态：Runner 已实现且 Mock/安全测试通过；**UI 和产品路径未调用**。
- 代码：`EspRunner` 实现 `livstyle_hardware::Runner`（`runner.rs` `impl Runner for EspRunner`）。`hardware/tests/safety.rs` 11 项通过，包括审批绑定、芯片不符拒绝、烧录中取消、OutcomeUnknown 不自动重试。
- UI：`Cmd::OpenFlash` 只打开文案卡。确认后 `Service::authorize(..., "7f3e-draft", "esp32c5-devkitc-1")`，hint 写明「还没有烧录」。摘要和板卡 id 是字面量，不是这次扫描到的设备。
- CLI 没有 `flash` 子命令。`livstyle toolchain` 只探测 IDF。

### 15. 日志采集与报告

- 状态：串口读取函数已实现，未接到 UI。证据包尚未实现。
- 代码：`livstyle_esp::serial::read_log`。ADR-006 EvidenceBundle 状态是 Proposed，仓库里没有 `evidence/` 写入器。
- UI「实时日志」是 `board_log` 文本，内容是扫描和识别句子，绿色只是样式。

### 16. Windows / Linux / Android

- Linux：当前开发与测试环境。串口枚举依赖 sysfs。
- Windows：仅 `bridge.rs` 在 `cfg(windows)` 下使用 `cursor-sdk-bridge.exe`。没有 SetupAPI、没有 Win32 串口。本次没有在 Windows 上编译或运行。
- Android：仓库无 target、无条件编译、无 UI 分支。不能声称支持。

## C. 运行时调用链

### 用户发一句话（已接通的部分）

```
TextInput composer 回车或「发送」
→ App::handle_actions Cmd::Send
→ worker::State::send
→ 若是问芯片/串口：refresh_boards 或 ask_identify（不进 Agent）
→ 否则 record_intervention（RuleJudge + Service::intervene）
→ CodeAgent::Cursor → ask_cursor
     Bridge 已在 Boot 时 spawn
     ensure_cursor_agent → create_agent
     CursorClient::send → RunStream
     另一线程迭代事件 → apply_beats → transcript
→ CodeAgent::Octos → ask_octos
     ensure_session → session/open（固定 id livstyle:local:desk）
     turn_start 或 steer
     OctosClient::drain 轮询 → apply_beats → transcript
→ publish(Snapshot) → App::apply 改 TextInput
```

尚未接通：计划节点 UI、工具调用卡片、diff、验证步骤、EvidenceBundle、把硬件意图交给 `EspRunner`。

### 硬件（库有，UI 没有）

```
UI「烧录确认」→ authorize 记账 → 结束
（缺失）Supervisor::prepare/authorize/begin_dispatch
（缺失）EspRunner::dispatch
（缺失）serial::read_log → 断言 → EvidenceBundle
```

CLI 上存在、UI 上不存在：`livstyle devices`、`livstyle chip-info`、`livstyle toolchain`、`livstyle cursor smoke`、`livstyle octos probe`。

## D. 技术债

### P0

1. **本机 OUP 集成测试失败。** `livstyle-octos/tests/live.rs` 的 `spike2_against_local_serve`：`session/open` 返回成功，随后 `session/hydrate` 报 `unknown_session`，session id 为 `livstyle:spike2-<pid>`。工作台恢复会话依赖的 hydrate 在这条实测路径上不成立。
2. **烧录确认不是烧录。** `worker.rs` `primary` 的 `CardKind::Flash` 用固定摘要 `7f3e-draft` 和固定板卡 `esp32c5-devkitc-1` 记账。用户若把按钮理解成已烧录，会和设备状态不一致。

### P1

1. **产品任务和 Agent 会话是两套状态。** 任务在 SQLite；对话在内存；Octos 会话 id 全局一个；Cursor agent 按 cwd 一个。`Select` 不改这些。切换任务后，后续事件仍写同一个 transcript。
2. **`apply_agent` 把 runtime 外部 id 写成 `"desk"`**，覆盖不了 `ensure_cursor_agent` 里写过的真实 agent id。恢复时无法从任务记录找回 Cursor agent。
3. **断线与取消的错误被丢掉。** `interrupt` / `cancel_run` 使用 `let _ =`。Octos 180 秒、Cursor 20 分钟超时后只在对话里说一句，不把产品任务打到 `Failed` 或 `OutcomeUnknown`。
4. **worker 单飞。** `busy` 期间新输入进 followup 队列，没有每个任务一条执行环。
5. **UI 线程与快照。** turn 在 worker 线程，这点是对的。但 `LINES` 用 `Mutex::expect`，中毒会直接崩 UI。快照整份替换 transcript，没有事件游标，重放会整段 set_text。

### P2

1. 顶栏四个导航没有动作。横幅和三栏是布局，不是任务计划/验证流水线。
2. `design/reference/livstyle-dashboard.png` 缺失，视觉没有可版本化的源稿。
3. 右栏标题是「实时日志」，数据是板级扫描，不是构建日志。
4. `identify_chip` 不把型号写进持久设备表。
5. Android / Windows 硬件路径不存在。RK3588 在代码中无实现（全仓库 `rk3588` 无匹配）。
6. ADR-006 证据包、ADR-004 运行时事件仍是 Proposed，和代码之间没有落地类型。
