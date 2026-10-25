# livstyle 产品需求文档

**版本：** v0.3.1  
**状态：** Accepted for MVP-0/MVP-1（ADR 见 [docs/adr](adr/)）  
**日期：** 2026-10-18  
**产品定位：** 可验证的硬件开发智能体工作台  
**首发场景：** Linux 桌面 + ESP32 系列（首批 S3、C3、C5、C6、P4 与经典 ESP32）的开发、构建、烧录与板上验证；**验收板卡 S3 + C5**  
**核心技术：** Rust + Octos + Makepad + Cursor SDK Bridge + Hardware Supervisor/Runner + Jev 快速判断  
**历史版本：** [v0.1](archive/PRD-v0.1.md)、[v0.2](archive/PRD-v0.2.md)

---

## 0. 版本说明

### 0.0 v0.3.1 修订（Spike 结果回填）

| 变化 | v0.3 | v0.3.1 |
|---|---|---|
| 验收板卡 | S3 + C3/C6 之一 | **S3 + C5**（已有 C3/C5/C6/S3 实物；C3、C6 作为回归板卡）；C5 升为 Verified 目标 |
| Cursor 能力 | 待确认是否支持运行中注入 | Bridge v1.0.38 实测：**不支持运行中注入**；`append_context` 排队到下一次 Send，`amend_requirement` = Cancel + 新 Run（§8.4、§9.1） |
| Jev 后端 | 本地 llama-server | 增加本地 Ollama（`/api/generate` raw 模式）；Spike 4 实测数据见 §8.3 |
| Cursor API Key | 安全存储 | 用户经 `livstyle auth cursor` 隐藏输入录入，权限 0600，只显示指纹（§9.2） |
| ADR | ADR-001 ~ 010 | 新增 ADR-011 持久化、ADR-012 worktree 隔离、ADR-013 ESP-IDF 工具链 |

### 0.1 v0.3 相对 v0.2 的变化

| 变化 | v0.2 | v0.3 |
|---|---|---|
| 运行中干预 | 只有取消 | 新增 §8：执行中随时打断、补充输入、修改需求，并基于计划依赖图快速重编排；进入 MVP-0/MVP-1 |
| Jev 快速判断 | 无 | 引入 jev-rs 作为 System One 判断层，用于干预分类、影响范围判断和工具输出分诊；可选加速器，不影响正确性 |
| 参考产品 | 未体现 EspSmith | 新增 §2.3：逐项列出对 EspSmith 的借鉴点和差异 |
| 芯片范围 | “单一 ESP32 板型” | 改为 ESP32 系列，Chip Profile + Board Profile 数据驱动适配；首批 S3/C3/C5/C6/P4/经典 ESP32；分级支持 |
| 安全不变量 | — | 新增芯片/revision 不匹配拒绝写入；Jev 判断不能授权任何硬件操作 |

### 0.2 v0.2 相对 v0.1 的变化（保留）

| 变化 | v0.1 | v0.2 |
|---|---|---|
| 产品定位 | 跨平台超级智能体，首个方向为 IoT | 先做“可验证的硬件开发智能体工作台”，超级智能体作为长期愿景 |
| MVP 范围 | 三平台 + ESP32 + RK3588 + Ontology + RSI 同时进入 MVP | 拆为 MVP-0 至 MVP-3，首个可交付闭环只含 Linux + ESP32 |
| 状态所有权 | Octos 与 livstyle 的任务职责有重叠 | 产品级任务只有一个真源（livstyle Product Service） |
| 硬件安全 | 设备租约与审批 | 新增独立 Hardware Supervisor；Runner 必须校验 fencing token；引入操作意图与执行收据 |
| Cursor | 作为 Agent Runtime 接入 | 增加认证、额度、IDE 登录与 SDK API Key 分离、禁止静默切换等要求 |
| 验证 | 区分命令成功与需求验证 | 正式定义验证等级 V0–V3 与操作风险等级 R0–R4 |
| Ontology / RSI | MVP 内实现 | 先做可追溯知识记录与候选离线评测，图推理和自动发布后置 |
| 指标 | 只列指标名 | 要求固定基线、测试集、样本数与统计周期 |

### 0.3 外部评审意见对照（v0.2 引入）

| 优先级 | 评审问题 | 处理位置 |
|---|---|---|
| P0 | Octos、Cursor、硬件 Runner 执行职责不清 | §5 |
| P0 | Cursor 套餐复用的认证与计费边界不清 | §9 |
| P0 | 烧录安全缺少完整的并发与故障恢复协议 | §7、§10 |
| P0 | MVP 横跨过多平台与技术领域 | §3、§15、§17 |
| P1 | OUP、Bridge 与产品任务的状态映射不完整 | §6、§7.3、ADR-002/004 |
| P1 | Ontology 与 RSI 同时做大 | §13、§14 |
| P1 | 验收指标缺少基线与统计口径 | §19 |
| P1 | 多 Agent 并行与硬件独占冲突 | §9.3、§10.1 |

---

## 1. 产品定义

livstyle 是一个面向物联网和嵌入式开发的智能体工作台。

用户用自然语言提出开发目标，livstyle 将其转换为可执行、可恢复、可审计的任务，协调 Agent、开发工具链和真实硬件，完成代码修改、构建、部署、调试和验证。**执行过程中用户可以随时打断、补充或修改需求，系统只重做受影响的部分。**

**livstyle 的核心价值不是生成代码，而是建立从需求到真实设备验证结果的闭环。** 传统 AI 编程工具证明代码已经生成；livstyle 要证明：

- 代码在指定工具链下构建成功；
- 指定产物被写入了正确的物理设备；
- 操作确实执行，且没有重复副作用；
- 设备产生了预期行为；
- 以上结论都有日志、哈希、设备身份和来源可追溯。

因此 livstyle 的核心对象不是聊天消息，而是**可恢复的开发任务、执行状态和验证证据**。

系统以 Octos 作为原生 Agent 内核，以 Makepad 构建跨平台客户端，以可插拔 Agent Runtime 支持 Cursor 等外部执行环境，并通过受限的 Hardware Runner 操作开发板。所有关键硬件操作由确定性安全策略控制，Agent 不能绕过授权直接访问特权设备。

### 1.1 长期愿景

长期上，livstyle 会扩展为类似 WorkBuddy、GrokBot 的通用超级智能体：多任务并行、本地文件与工具操作、外部 Agent 协作、办公与研究能力。本 PRD 只对硬件开发闭环做实现承诺，通用能力不进入 MVP。

### 1.2 产品原则

1. **Evidence over claims：** 以可追溯证据证明任务完成。
2. **One source of truth：** 每个产品级任务只有一个权威状态。
3. **Safe by construction：** 危险操作在执行边界强制校验。
4. **Runtime is replaceable：** Agent Runtime 可替换，任务语义保持稳定。
5. **Recoverable by default：** 断线、重启和超时不导致副作用重复。
6. **Human approval for irreversible actions：** 不可逆操作必须明确授权。
7. **User stays in control：** 用户输入随时可达，不因任务运行而被阻塞或丢失。
8. **Profiles over code：** 新芯片、新板卡优先通过声明式 Profile 接入，而不是改代码。
9. **Incremental delivery：** 每个阶段都能独立运行和验收。

---

## 2. 目标用户、场景与参考产品

### 2.1 主要用户

- ESP-IDF / ESP32 系列固件开发者
- RK3588 Linux / Android 边缘设备开发者
- IoT 创业团队与小型硬件团队
- 自动化硬件在环测试工程师

### 2.2 首发场景

用户提出：

> “检查当前 ESP32-S3 项目，修复启动问题，构建固件，在连接的开发板上运行并验证启动日志。”

系统应能：

1. 检查项目、目标芯片（`idf_target`）和工具链；
2. 制定任务计划并识别风险；
3. 修改代码并记录 Diff；
4. 构建固件，保存产物和哈希；
5. 识别目标设备的实际芯片和 revision，与项目目标比对，获取独占租约；
6. 展示精确的烧录授权请求；
7. 在授权后执行烧录；
8. 采集启动日志并执行预定义断言；
9. 生成带设备身份、产物、源码版本和日志证据的报告。

执行中用户追加“顺便把日志波特率改成 921600”或“先别烧录，改成 C6 板子”，系统应能识别这是补充还是需求变更，展示计划变化，只重做受影响的步骤（见 §8）。

无法确定设备状态时，系统必须报告“未知”，不得推断成功，也不得盲目重试。

### 2.3 参考产品：EspSmith

[EspSmith](https://github.com/fangkuaiLS/EspSmith)（Apache-2.0，Tauri + React + Rust）是面向 ESP32 的 AI 原生 IDE，把代码生成、编译、烧录、串口验证和 JTAG 调试放进同一工作流。它是 livstyle ESP32 闭环最直接的参考对象。以下依据其 `src-tauri/src` 源码整理。

#### 借鉴点

| EspSmith 能力 | 源码位置 | livstyle 中的对应 | 阶段 |
|---|---|---|---|
| 闭环流水线 `plan → preflight → build → flash → verify → report` | `self_healing/stages.rs`、`runner.rs` | §11.3 标准流程，结构基本一致，增加 ArtifactCheck、Approval、Lease | MVP-1 |
| Self-Healing：失败分类 → 恢复动作（串口复位、探针软/硬复位、断电重启）→ 回到锚点（Start/Build/Load/Check），带分类重试预算 | `self_healing/types.rs`、`recovery.rs` | Product Service 的 Recovery Policy；锚点映射到计划节点。**会触碰设备的恢复动作作为 R2 操作经 Supervisor 执行**，不得自动循环重试 | MVP-1 |
| Experience 引擎：记录板卡/链路组合稳定性、错误与修复对应关系 | `experience/engine.rs`、`storage.rs` | 记为 Ontology 的 `Failure`、`TestEvidence` 实体，并作为 RSI 候选信号；要求来源和适用芯片 | MVP-1 采集，MVP-3 利用 |
| JTAG/UART 自动识别：USB VID/PID、CP210x/CH340/FTDI 判别、esptool 读芯片 | `connection.rs` | Runner 设备发现；结果写入 `DeviceIdentity`，只作为身份候选，需芯片 ID 确认 | MVP-1 |
| ESP-IDF 发现顺序（EIM → VS Code 扩展 → 手动 → `IDF_PATH`）与 OpenOCD 查找优先级 | `idf.rs`、`commands/openocd.rs` | Toolchain Preflight，结果写入 `toolchain_manifest` | MVP-1 |
| 多芯片支持：ESP32/S2/S3/C3/C5/C6/C61/H2/P4 的 OpenOCD target、GDB 工具链（Xtensa/RISC-V）映射 | `commands/openocd.rs`、`gdb_session.rs` | 改为 Chip Profile 字段（§11.1），不在代码里硬编码 | MVP-1 |
| JTAG 调试：断点、单步、寄存器、调用栈、CoreDump 分析 | `commands/debug.rs`、`adapters/gdb*.rs` | CoreDump 解析进入 MVP-1 的失败证据；交互式调试后置 | MVP-1（CoreDump）/ MVP-2+ |
| 图形化引脚分配、冲突检测、生成头文件 | `commands/hardware.rs` | Board Profile 的保留引脚与冲突规则；可视化 Pin Planner 后置 | MVP-2 |
| `sdkconfig` 加载与 menuconfig 服务 | `sdkconfig*.rs`、`confserver.rs` | 作为 R1 配置修改工具；改动纳入 Diff 与产物失效判断 | MVP-1 |
| AUTO / CODE 双模式 | 前端 | Makepad 客户端：任务视图（AUTO）为主；CODE 视图首版只做 Diff、文件浏览和日志 | MVP-0 |
| 项目工具以 MCP 暴露给 AI | `mcp.rs` | 工具经 Octos `peer/tools/register` 注册，硬件工具一律经 Supervisor | MVP-0 |

#### 差异与改进

| 维度 | EspSmith | livstyle |
|---|---|---|
| 设备安全 | 无租约、审批、fencing token；Agent 可直接触发烧录 | Supervisor + Runner、精确审批、R0–R4、操作收据、`OutcomeUnknown` |
| AI 接入 | 单 Provider（CodeWhale / MiMo-Code CLI） | Runtime Adapter：Octos 原生 + Cursor SDK Bridge，可扩展 |
| 运行中干预 | 未提供结构化的打断与重编排 | §8 Live Steering + Jev |
| 芯片适配 | 代码内 `match` 分支 | 声明式 Chip/Board Profile，分级支持 |
| 验证结论 | 流水线通过即成功 | V0–V3 验证等级与 EvidenceBundle |
| 经验积累 | 本地经验记录 | 带来源的 Ontology 记录 + 离线评测的受控 RSI |
| UI 技术 | Tauri + React + Monaco | Makepad（Rust 原生，Linux/Windows/Android） |

许可：EspSmith 为 Apache-2.0。若直接复用其代码片段，需保留版权与许可声明并在 `NOTICE` 中注明；优先复用设计而非复制代码。

### 2.4 参考产品：Toit / Jaguar

[Toit](https://github.com/toitlang/toit) 与配套工具 [Jaguar](https://github.com/toitlang/jaguar) 把 ESP32 的内循环从「改一行、整镜像重编、USB 重烧」改成「USB 只刷一次底座，之后应用经局域网热替换」。livstyle 的 ESP32 系列交付遵循这个分层，不把 USB 烧录当作每次修改的默认动作。决策见 ADR-014，选择逻辑在 `livstyle_domain::plan_iteration`。

| 情形 | 交付 |
|---|---|
| 普通应用逻辑，底座已在板上 | 先在电脑上跑通，再经局域网替换一个应用容器。不复位，不重烧整镜像 |
| 板子还没有底座 | 电脑上跑通 → USB 刷一次底座 → 之后改为局域网替换 |
| 只加一个服务（对时、现成驱动） | 替换或安装对应容器，业务代码不必跟着重编 |
| 底座运行时本身变了 | 才再次使用 USB |
| 硬实时、新驱动、芯片 bring-up、`sdkconfig` / flash / PSRAM | 仍走完整 ESP-IDF 镜像。带 GC 的运行时不承担微秒级时序 |

一个应用失败不得要求重刷底座。热替换走局域网，不依赖云。C3 / C5 / C6 / S3 / P4 共用这条策略，芯片差异仍由 Chip / Board Profile 表达。

---

## 3. MVP 目标

### G1：完成 ESP32 系列真实硬件闭环

在 ESP32 系列上完成构建、烧录、启动验证和证据归档。至少一个 Xtensa 芯片（S3）和一个 RISC-V 芯片（C3 或 C6）在真实硬件上达到 V3，以证明适配不是针对单一芯片硬编码（§11.2）。

### G2：支持可靠任务恢复

支持任务持久化、事件重放、取消、失败恢复和 `OutcomeUnknown` 状态。

### G3：复用 Cursor Agent Runtime

在未配置 Octos 原生 LLM Provider 的情况下，通过官方 Cursor SDK Bridge 创建和运行 Agent，观察事件、取消任务并恢复观察。

Cursor 路径遵循官方 SDK 的认证、计费和额度规则，不依赖非官方私有推理接口。**只有在真实账户、真实套餐下跑通后，才能把该能力写成已实现的产品承诺。**

### G4：保证硬件操作安全

所有危险操作必须绑定设备身份、芯片与 revision、产物哈希、操作参数、审批有效期和执行记录。

### G5：建立可扩展的硬件工具架构

以 Profile、插件或 capability 方式增加芯片、板卡、工具链和 Runner，不把设备逻辑写进 UI 或 Agent 内核。

### G6：运行中随时干预与快速重编排

任务执行中，用户可以随时打断、补充输入或修改需求；输入不丢失；系统给出可见的计划变化，并只重做受影响的节点。硬件操作的中断遵守安全边界（§8.5）。

---

## 4. 非目标

首版不实现：

- Android 本地执行完整嵌入式工具链；
- 通用办公、邮件、PPT 和浏览器自动化套件；
- 对没有声明 Profile 的芯片或板卡自动写入（自动识别只用于建议 Profile）；
- 把特权 USB、JTAG 或 `/dev` 直接暴露给 Agent；
- 自动执行 eFuse、OTP、Secure Boot 密钥写入和整盘擦除；
- 将 Cursor 订阅转换为通用 OpenAI 兼容模型 API；
- 无审批地修改或发布生产内核代码；
- 无界自我修改、自主发布或不可回滚的 RSI；
- 在 MVP 中实现完整知识图谱平台；
- 让 Jev 或任何模型判断直接授权硬件操作；
- 交互式 JTAG 图形调试器（CoreDump 分析除外）。

---

## 5. 系统架构与职责

### 5.1 模块划分

**A. Makepad Client**

负责工作区、会话、任务列表、事件流、计划视图、审批界面、设备状态和证据报告。输入框在任务运行期间始终可用。只保存 UI 状态和未提交草稿，不拥有任务执行的权威状态。

**B. livstyle Product Service**

负责产品级任务、计划依赖图、硬件工作流编排、审批记录、跨 Runtime 映射、EvidenceBundle 发布和产品级任务恢复。

**C. Octos Runtime**

负责原生 Agent 执行、上下文、记忆、原生工具、技能、工作流和 OUP 会话。优先复用 Octos 现有能力；只有现有能力无法满足持久化或恢复要求时才扩展内核（见 §5.3）。

**D. Agent Runtime Adapter**

统一的外部接口：

- Discover / Capabilities
- Create / Resume
- Send / Steer / Interrupt
- Observe / Stream
- Cancel
- Close
- GetUsage（仅当 Runtime 支持）

首批适配器为 `OctosNative` 和 `CursorBridge`。Adapter 只做协议转换，不拥有产品级任务的最终状态。Steer 和 Interrupt 能力通过 capability 声明，不支持时由 Intervention Router 退化处理（§8.4）。

**E. Cursor Bridge Manager**

负责 Bridge 二进制校验、版本锁定、启动、握手、鉴权、健康检查、能力协商、退出和升级。Rust 侧通过官方 `sdk.v1` 协议接入，Bridge 的 RPC、事件和错误映射为 livstyle 内部规范化类型。

**F. Hardware Supervisor**

确定性安全边界：设备身份、芯片匹配、租约、操作授权、执行状态机和安全拒绝。Agent 和 Jev 都无权修改 Supervisor 的策略或覆盖其拒绝结果。

**G. Hardware Runner**

负责本机工具链和真实设备操作，包括 ESP-IDF、esptool、串口、OpenOCD、SSH 和 ADB。Runner 是实际操作设备的可信执行边界；远程 Runner 必须经过认证，并自行验证设备租约与 fencing token。

**H. Evidence Store**

保存任务事件、操作收据、产物元数据、日志引用、测试断言和最终报告。大日志和二进制产物外置存储，事件流只保存引用、摘要和校验值。

**I. Intervention Router**（Product Service 内的组件）

接收运行中的用户输入，持久化为 `InterventionRecord`，调用 Judge 分类，按规则映射为 Runtime 操作和计划变更（§8）。

**J. Judge Service（Jev）**

System One 快速判断层。对给定状态回答类型化问题（是/否、N 选 1、有序评分），返回概率和置信度，不生成文本。首选 [jev-rs](https://github.com/yijunyu/jev-rs)，可通过 `jev serve`（`/v1/systemone`）或 `jev mcp` 接入。Judge 只提供建议；低置信度时交给 System Two（当前 Runtime 的大模型）或用户确认（§8.3）。

### 5.2 所有权规则

| 状态 | 唯一所有者 |
|---|---|
| Octos 原生会话、turn、原生工具执行 | Octos Runtime |
| Cursor Agent / Run | Cursor Runtime（经 Cursor Adapter 映射） |
| 产品级开发任务、计划依赖图、干预记录 | livstyle Product Service |
| 设备租约与硬件操作状态 | Hardware Supervisor |
| EvidenceBundle 发布状态 | Evidence Store / Product Service |
| Chip / Board Profile 注册表 | Product Service（只读下发给 Supervisor 与 Runner） |

附加规则：

- “产品级任务”与“Agent Run”不是同一对象。一个 ESP32 任务可以包含多个 Octos 节点和多个 Cursor Run，但只有一份产品级任务状态和一份设备授权记录。
- UI 不得根据聊天文本推断任务成功。
- Runtime 切换不得静默发生；切换时记录原 Runtime、原因和新的会话映射。
- Judge 的输出只作为 Intervention Router 的输入，不直接写任务状态。

### 5.3 Octos 现有能力核对

以下依据本地 `octos` 仓库（`2.0.3-rc.13`）核对，用于防止把目标架构误认为已有能力。实施前仍需在 Spike 2 中逐项实测。

| 能力 | 现状 | 依据 | 对 livstyle 的含义 |
|---|---|---|---|
| OUP 会话、turn、approval、hydrate/replay | 已有 | `octos-core/src/ui_protocol.rs` | 客户端直接复用 |
| 运行中追加输入 `turn/steer` | 已有 | `ui_protocol_transport.rs`（带 `expected_turn_id`） | 干预“补充上下文”直接映射 |
| 未吸收输入回退 `turn/steer_dropped` | 已有 | `event.turn_steer_dropped.v1`；`octos-cli/src/steer_return.rs` | 轮次结束时未被吸收的输入按顺序返回，客户端重新排队，保证不丢 |
| 打断当前轮次 `turn/interrupt` | 已有 | `methods::TURN_INTERRUPT` | 干预“改需求 / 停当前步骤”使用 |
| 任务取消 `task/cancel` | 已有 | `methods::TASK_CANCEL` | 只取消软件侧；硬件操作仍按 §8.5 |
| 计划推送 `plan/updated` | 已有 | `ui_protocol.rs` feature flag | Agent 计划可见；产品级计划图仍由 Product Service 持有 |
| 无 LLM Provider 启动 OUP 会话 | **不支持** | `octos-cli/src/runtime/profile.rs:1033` 报错 “has no LLM provider configured” | Cursor-only 模式不能靠普通 `octos serve` 实现，需 Adapter 并列编排或扩展 Octos |
| 宿主代理推理（子进程不持有凭据） | 部分已有 | `octos acp --host-managed` + `octos-llm/src/host.rs`（v1 为单次有界补全，非流式） | 可作为后续“外部推理驱动”的参考，不能直接承载 Cursor Agent 语义 |
| 宿主注册工具 | 已有 | `peer/tools/register`（UPCR-035） | 硬件工具和 Judge 工具可由 livstyle 注册给 Octos Agent |
| 任务从节点重启 | 已有 | `task/restart_from_node` | 重编排时从最早受影响的软件节点重启；硬件步骤仍需 Supervisor 状态机 |
| 记忆检索 | 已有 | `memory/search`、`memory/load`、`memory/ingest` | Ontology 首版可建在其上 |
| 技能自我修正 | 仅建议 | `app-skills/skill-evolve`：生成 SKILL.md 补丁，人工应用 | 可作为 RSI E0 候选来源，不等于 RSI |
| 外部 Agent 驱动协议 | 未进入 Core | octoscode-web 有 `external_driver_v1` 客户端草案 | 需走 UPCR，不在 MVP 依赖 |
| Jev / System One 判断 | 无 | Octos 仓库中无相关实现 | 作为外部 Judge Service 接入，不改 Octos 内核 |

---

## 6. 关键数据模型

### 6.1 ProductTask

- `task_id`
- `workspace_id`
- `session_id`
- `runtime_id`
- `runtime_session_id`
- `status`
- `requirement_version`（每次需求变更递增）
- `plan_version`
- `current_node_id`
- `created_at` / `updated_at`
- `version`
- `parent_task_id`（可选）

所有状态变更使用持久化版本号或等效并发控制，避免旧执行者覆盖新状态。

### 6.2 PlanNode

- `node_id`
- `kind`（analysis / code_change / config_change / build / flash / verify / report …）
- `depends_on`
- `input_digests`：需求版本、源码 revision、`sdkconfig` 摘要、Chip/Board Profile ID、上游产物哈希
- `status`（pending / running / done / invalidated / skipped）
- `interruptibility`（§8.5）
- `outputs`（产物、日志引用）

输入摘要变化的节点及其下游被标记为 `invalidated`；输入未变的已完成节点保留结果。

### 6.3 InterventionRecord

- `intervention_id`
- `task_id`
- `received_at`
- `input`（原文与附件引用）
- `target_turn_id` / `target_node_id`
- `judgment`（Judge 原始输出、置信度、模型与校准版本）
- `resolved_by`（judge / system_two / user）
- `intent`、`scope`
- `action`（steer / interrupt_and_replan / cancel_node / cancel_task / pause / side_answer / new_task）
- `status`（received / classified / applied / queued / returned / rejected）
- `plan_diff_ref`

### 6.4 DeviceIdentity

- `device_id`
- `runner_id`
- `transport`
- `serial_number` 或平台适用的身份字段
- `chip_id`（如 MAC）
- `detected_chip`（esptool 读取的芯片型号，如 `esp32c6`）
- `detected_revision`
- `flash_size` / `psram`（可检测时）
- `board_profile_id`
- `identity_confidence`
- `last_seen_at`

USB 路径（如 `/dev/ttyUSB0`、`COM3`）只是连接信息，不作为设备身份。

### 6.5 DeviceLease

- `device_id`
- `lease_id`
- `task_id`
- `fencing_token`
- `expires_at`
- `runner_id`
- `state`

续期和释放必须做所有权检查。过期持有者不得发起新的设备写操作。

### 6.6 OperationIntent 与 OperationReceipt

`OperationIntent`：

- `operation_id`
- `task_id`
- `device_id`
- `runner_id`
- `operation_type`
- `risk_level`
- `target_chip` / `target_revision_range`
- `artifact_sha256`
- `parameters_digest`
- `approval_id`
- `fencing_token`
- `cancel_requested`
- `state`
- `started_at` / `completed_at`

`OperationReceipt` 由 Runner 生成，记录实际执行的命令摘要、退出状态、关键输出摘要、实测芯片与 revision，以及执行时使用的 fencing token。

### 6.7 EvidenceBundle

- `evidence_id`
- `task_id`
- `requirement_version`
- `source_revision`
- `toolchain_manifest`
- `chip_profile_id` / `board_profile_id`
- `artifact_manifest`
- `device_identity`
- `operation_receipts`
- `interventions`（本任务的干预记录引用）
- `log_references`
- `assertion_results`
- `verification_level`
- `created_at`

所有产物以 SHA-256 或同等强度哈希绑定。报告必须区分命令成功、设备启动和需求验证通过，并标明验证的是哪个需求版本。

### 6.8 ChipProfile 与 BoardProfile

见 §11.1。

---

## 7. 任务状态机与恢复

### 7.1 产品级任务状态

主路径：

`Queued → Planning → Running ⇄ AwaitingApproval → Verifying → Succeeded`

干预相关：`Running → Replanning → Running`；`Running → Paused → Running`。

异常状态：`Failed`、`Cancelled`、`Blocked`、`OutcomeUnknown`。

状态转换只由明确的事件和前置条件驱动，不允许仅凭模型的自然语言输出或 Judge 判断改变状态。

### 7.2 硬件操作状态

主路径：

`Prepared → Authorized → Dispatched → Running → Completed`

异常分支：`Rejected`、`Failed`、`Cancelled`（仅限未开始执行）、`OutcomeUnknown`、`Reconciled`。

执行前持久化操作意图，执行后持久化收据。结果无法确认的操作不得自动重新派发。

### 7.3 恢复要求

- 每个任务事件有稳定 ID 或任务内单调序号；
- 客户端维护最后确认的事件游标；
- 重连后通过 hydrate + replay 恢复视图；
- 重复事件幂等处理；
- 事件缺口触发补拉或完整快照；
- 未处理的 `InterventionRecord` 在重启后恢复，不得丢失；
- 取消任务不代表正在执行的外部操作已停止；
- 设备操作结果未知时，先查询设备和 Runner 状态，再由确定性策略或用户决策消歧。

---

## 8. 运行中干预与快速重编排（Live Steering）

### 8.1 用户能力

任务执行的任何阶段，用户都可以：

| 干预 | 示例 | 默认处理 |
|---|---|---|
| 补充上下文 | “板子是 8MB Flash 的版本” | 注入当前轮次，不打断 |
| 修改需求 | “波特率改成 921600”“换成 C6 板子” | 打断当前轮次，计算计划差异，从最早受影响节点重做 |
| 调整优先级或约束 | “先别烧录，只构建” | 修改计划节点，不重做已完成部分 |
| 暂停 / 继续 | “等一下” | 在下一个安全点暂停 |
| 取消当前步骤 | “这个分析不用做了” | 中断当前节点，计划继续 |
| 取消任务 | “不做了” | 取消软件侧；硬件操作按 §8.5 |
| 旁路提问 | “现在用的是哪个 IDF 版本？” | 单独回答，不影响主任务 |
| 新任务 | “另外帮我写个 OTA 示例” | 创建新的 ProductTask |

UI 要求：输入框在任务运行时始终可用；每条干预都显示处理结果（已注入 / 已重编排 / 已排队 / 已退回 / 需确认）；重编排时展示计划差异（保留、作废、新增的节点）。

### 8.2 处理流程

`Receive → Persist → Classify（Jev）→ Resolve → Apply → Replan → Report`

1. **Receive / Persist：** 输入先持久化为 `InterventionRecord`，再做任何处理。
2. **Classify：** Judge 对“当前任务状态 + 计划 + 新输入”回答一组类型化问题（§8.3）。
3. **Resolve：** 置信度达到阈值直接采用；否则交给 System Two（当前 Runtime 的大模型）判断；仍不确定时以快捷选项请用户确认。
4. **Apply：** 按 §8.4 映射为 Runtime 操作。
5. **Replan：** 需求变更时递增 `requirement_version`，按计划依赖图做确定性失效计算；只让 Agent 生成新增或作废节点的替代方案，不重写整份计划。
6. **Report：** 向用户展示计划差异和处理结果，写入事件流。

快速重编排的来源：失效计算由依赖图和输入摘要确定性完成；分类由 Judge 在百毫秒级完成；大模型只参与新节点生成。

### 8.3 Jev 判断层

**为什么用 Jev：** 干预分类、影响范围判断这类决策选项固定、要求低延迟，不需要大模型生成长文本。jev-rs 读取模型下一个 token 的概率分布做判断，不解码，状态只预填充一次。上游公开测试中，Qwen3-4B 本地运行单题延迟 p50 约 75–90ms（Apple M1 Ultra）。

**首批判断问题：**

| 问题 | 类型 | 选项 |
|---|---|---|
| intent | choice | append_context / amend_requirement / reprioritize / pause / cancel_step / cancel_task / side_question / new_task |
| scope | choice | current_step / remaining_plan / whole_task / unrelated |
| touches_hardware_params | noul | 是否改变芯片、板卡、烧录参数或分区 |
| invalidates_artifact | noul | 是否需要重新构建 |
| urgency | score | 可等当前步骤完成 / 尽快 / 立即 |

后续扩展：构建与烧录错误分诊（选择 Self-Healing 恢复动作的候选）、预测命令输出量、任务路由到技能、日志断言辅助判断。

**约束：**

- Judge 只给建议，不授权、不执行任何操作；`touches_hardware_params=是` 时必须走确定性重新审批路径；
- 置信度阈值由 livstyle 自有干预数据集上的 `jev eval` / `jev calibrate` 确定，不使用通用默认值。上游数据显示小模型零样本准确率有限（Qwen3-4B 在其 dev_tasks 上约 0.71），且需要温度校准；
- 每次判断记录模型、校准版本、概率和最终采用方，便于离线评估；
- Judge 不可用、超时或后端不支持 logprobs 时，退化为规则 + System Two，功能不受影响，只是变慢。

**后端：**

- jev-rs 需要能返回 logprobs 的后端：本地 `llama-server`（GGUF 小模型），或支持 `logprobs` 的 OpenAI 兼容接口；
- **Cursor SDK 不提供 logprobs，Cursor Runtime 不能作为 Jev 后端。** 只用 Cursor 的用户需要本地小模型才能启用 Jev，否则走退化路径；
- jev-rs 预编译版本覆盖 Linux x86_64/arm64 和 macOS；Windows 需源码构建，在 MVP-2 验证；Android 不在本地运行 Judge，由桌面或服务器节点提供；
- 本地 Ollama 也可作为后端：使用 `/api/generate` raw 模式自行套聊天模板，取首 token 的 `top_logprobs`（livstyle 内置实现，无需 jev-rs 二进制）；
- 模型选择、阈值和部署方式由 ADR-009 决定。

**Spike 4 实测（2026-10-18，Linux，Ollama 0.24，112 条中英文标注样本 `evals/interventions.jsonl`）：**

| 配置 | intent 准确率 | intent ECE | 置信 ≥0.8 覆盖 / 准确 | scope 准确率 | 硬件参数标记准确率 / 漏报 | 单条输入延迟 p50 / p95 |
|---|---|---|---|---|---|---|
| 规则 Judge（退化路径） | 49.1% | 0.211 | 14.3% / 93.8% | 46.4% | 83.9% / 0 | < 1 ms |
| Jev，4 个问题 | 92.0% | 0.031 | 87.5% / 98.0% | 51.8% | 95.5% / 0 | 8.9 s / 9.2 s |
| **Jev，默认**（scope 由 intent 推出，仅 append/amend 追问；不问是否重建） | **92.0%** | **0.031** | **87.5% / 98.0%** | **80.4%** | **95.5% / 0** | **4.5 s / 6.9 s** |

模型：`Qwen3.6-35B-A3B`（Q6_K，MoE 3B 激活），未做温度校准。错误集中在“补充 vs 修改需求”和“跳过当前步骤 vs 调整顺序”两组边界，其中部分是样本标注本身的歧义；样本集由项目方编写，需用真实使用中的干预持续扩充后再定阈值。

结论：Jev 路径在准确率和校准上满足“高置信自动执行、低置信确认”的设计；**延迟是主要短板**，ADR-009 要求在 MVP-1 前评估 3–4B 小模型，并保持“先显示‘已接收’、判断结果异步到达”的交互，不阻塞输入。

### 8.4 映射到 Runtime 操作

| intent | Octos Native | Cursor Runtime |
|---|---|---|
| append_context | `turn/steer`；若收到 `turn/steer_dropped` 则在下一轮重新提交 | 以 capability 为准；不支持运行中注入时排队到下一次 Send |
| amend_requirement | `turn/interrupt` → 重编排 → `task/restart_from_node` 或新 turn | Cancel 当前 Run → 重编排 → 新 Run，携带需求差异与已保留结果 |
| cancel_step | `turn/interrupt` | Cancel Run |
| cancel_task | `task/cancel` | Cancel Run |
| pause | 在节点边界暂停，不发起新 turn | 当前 Run 结束后不再派发 |
| side_question | 独立的旁路会话或只读回答，不进入主任务上下文 | 同左 |
| new_task | 新 ProductTask | 同左 |

不支持的组合通过 capability 显示为不可用或退化处理，不得静默丢弃输入。

### 8.5 硬件操作的中断边界

每类节点声明 `interruptibility`：

| 类型 | 示例 | 打断策略 |
|---|---|---|
| free | 代码分析、构建、串口日志采集 | 立即中断 |
| safe_point | 多步烧录序列、测试套件 | 在当前原子步骤结束后停止 |
| non_interruptible | 单次 Flash 写入、分区写入 | 默认不强行中断：标记 `cancel_requested`，完成后停止并作废后续节点 |

- 尚未派发（`Prepared` / `Authorized`）的硬件操作被干预时直接取消，释放租约，作废审批；
- 用户强制中断正在写入的操作时，必须二次确认，操作进入 `OutcomeUnknown`，按 §7.3 消歧，不自动重烧；
- 需求变更导致产物、芯片、板卡或烧录参数变化时，旧审批立即失效；
- 干预不得延长或转移设备租约给其他任务。

---

## 9. Cursor Runtime

### 9.1 接入方式

采用官方 [Cursor SDK Bridge](https://github.com/cursor/sdk-bridge) 的 `sdk.v1` 协议实现 Rust Adapter。Cursor 被建模为外部 Agent Runtime，而不是普通模型 Provider，否则会混淆两套执行循环。

最小能力：

1. Bridge 版本与 capability 检查（`GetVersion`）；
2. Agent 创建与恢复；
3. Run 发送、事件流和取消；
4. Runtime 错误与额度信息映射；
5. Run ID、Agent ID 和事件 offset 持久化；
6. Bridge 进程退出后通过 offset 恢复观察。

运行中注入、模型目录、自定义工具回调和云端 artifacts 作为后续增强项，按 capability 启用。

**实测（Bridge v1.0.38）：** capabilities 为 `agent.create, agent.resume, agent.send, run.observe, run.wait, run.cancel, agent.management, cursor.catalog, artifacts.chunked, agent.usage`，**没有运行中注入**；只有 `ObserveRun` 流的 offset 可用于断线续看。版本锁定、安装校验与生命周期见 ADR-003。

### 9.2 认证与费用

- 用户 API Key 由用户本人写在 gitignore 的环境变量文件里（仓库根目录 `.env` 的 `CURSOR_API_KEY=`，或 `$XDG_CONFIG_HOME/livstyle/env`），不经过命令行参数，不进入聊天；进程环境变量 `CURSOR_API_KEY` / `LIVSTYLE_CURSOR_API_KEY` 仍可临时覆盖；文件权限 0600；界面和 `livstyle auth status` 只显示指纹；
- Bridge 本地 bearer token 每次启动独立生成，不与 Cursor API Key 混用；
- 凭据不得写入项目文件、普通日志或 Agent 上下文；
- **Cursor IDE 已登录不等于 SDK 可调用**，登录状态和 SDK API Key 必须分开建模；
- 启动前检查认证；运行中明确报告鉴权失败、额度不足和服务不可用；
- 不承诺套餐额度无限使用，计费与额度遵循 Cursor 规则；干预导致的 Cancel + 新 Run 会产生额外消耗，需在 UI 中可见；
- 认证失败或额度耗尽时明确失败，由用户主动选择其他 Runtime，不得静默切换或重复执行；
- 不支持的操作通过 capability 显示为不可用。

### 9.3 执行边界

- Cursor Agent 负责自身的 Agent 执行循环；Octos 负责原生 Agent 执行循环；跨 Runtime 的开发流程由 livstyle 任务编排层管理。
- 软件分析和代码生成可以并行；同一步骤不得由 Octos 和 Cursor 同时对同一工作目录做冲突写入，工作区写入通过任务锁或隔离工作树控制。
- 硬件操作始终由 Hardware Supervisor 和 Runner 执行，Cursor 内置工具不得绕过审批。

### 9.4 平台约束

官方 Bridge 当前只发布 Linux（x64/arm64）和 Windows（x64）版本。Android 不在本地运行 Bridge，只能通过受信任的桌面或服务器节点远程使用 Cursor Runtime，且 APK 中不得包含 Cursor 凭据。

---

## 10. 硬件安全与设备管理

### 10.1 设备租约

同一设备的串口、JTAG、烧录等互斥接口由统一设备租约协调：

- 同一设备的互斥操作必须串行化；
- 每次操作必须验证当前 fencing token，**且由实际执行操作的 Runner 校验**——只在服务端生成递增数字而 Runner 不验证，无法阻止旧任务在租约过期后继续写设备；
- Runner 拒绝已过期或被更高 token 取代的操作；
- 断线或超时后，租约不得自动触发危险操作重试。

### 10.2 精确审批

烧录审批至少显示：

- 设备稳定 ID、实测芯片与 revision、板型；
- 项目目标芯片（`idf_target`）及是否匹配；
- Runner 身份；
- 产物 SHA-256；
- 操作类型；
- 分区、地址范围和关键参数；
- 风险等级；
- 审批过期时间。

审批绑定不可变的操作参数摘要，任何关键参数改变都要求重新审批。

### 10.3 操作风险等级

| 等级 | 操作 | MVP 策略 |
|---|---|---|
| R0 | 只读检查、日志读取、芯片识别 | 允许 |
| R1 | 可恢复的软件修改、`sdkconfig` 修改、普通构建 | 允许 |
| R2 | 设备部署、普通 Flash 写入、设备复位类恢复动作 | 需精确审批 |
| R3 | 擦除、引导配置变更 | 默认拒绝 |
| R4 | eFuse、OTP、密钥烧录、不可逆安全配置 | 默认拒绝 |

R3/R4 需要在后续版本经过独立安全评审并引入专用审批流程后才能开放。

### 10.4 安全不变量

- 未授权的操作不得派发；
- 未确认设备身份不得写入设备；
- **实测芯片或 revision 与产物目标不匹配时，Runner 拒绝写入**；
- 产物哈希变化后不得沿用旧审批；
- 普通 Flash 授权不能升级为 Erase 授权；
- 超时不代表操作失败，断线不代表外部动作已停止；
- 断线后不得盲目重试非幂等操作；
- 旧租约持有者不得继续写设备；
- Agent 不得通过重新生成命令绕过状态机，也不得修改或覆盖 Supervisor 的拒绝；
- Judge（Jev）的输出不得作为任何硬件操作的授权依据；
- 所有拒绝和恢复决策留下审计记录。

---

## 11. ESP32 系列开发闭环

### 11.1 Chip Profile 与 Board Profile

芯片和板卡差异全部放在声明式 Profile 中，Runner、Supervisor 和 Agent 工具只读取 Profile，不在代码里按型号分支。新增芯片或板卡原则上只需新增 Profile 和对应的验证用例。

**Chip Profile** 字段：

- `chip`（`esp32` / `esp32s3` / `esp32c3` / `esp32c5` / `esp32c6` / `esp32p4` …）与 `idf_target`、esptool 芯片名；
- 架构（Xtensa / RISC-V）、核心配置；
- 支持的 ESP-IDF 版本范围；
- 支持的 revision 范围，以及 revision 对构建配置的影响；
- 调试接口：内置 USB-Serial-JTAG 或需外置桥接 / 探针；OpenOCD target 与 interface 配置；GDB 工具链；
- 默认控制台（UART / USB-Serial-JTAG / USB-CDC）；
- 识别规则：USB VID/PID、esptool 芯片识别结果；
- 无线能力：Wi-Fi 标准与频段、BLE、802.15.4，或“无射频”；
- 典型启动日志特征与默认断言模板；
- 可选：Rust（esp-hal / esp-idf-svc）支持级别。

**Board Profile** 字段：

- 引用的 Chip Profile、模组型号；
- Flash 大小、PSRAM 类型和大小；
- 保留引脚：strapping、USB、Flash/PSRAM 占用引脚；
- 板载外设：LED、按键、屏幕、摄像头接口等；
- 连接方式与自动下载电路行为（是否需要手动按 BOOT）；
- 伴随芯片（如 ESP32-P4 板上用于无线的 ESP32-C6）；
- 默认分区表与烧录参数。

Profile 有版本号；产物和 EvidenceBundle 记录所用 Profile 版本。

### 11.2 首批芯片与分级支持

| 芯片 | 架构 | 适配要点 | 首批目标级别 |
|---|---|---|---|
| ESP32-S3 | Xtensa 双核 | 内置 USB-Serial-JTAG；PSRAM 配置多样（含 Octal） | Verified（MVP-1 验收芯片，Xtensa） |
| ESP32-C5 | RISC-V | 2.4/5 GHz 双频 Wi-Fi 6；要求 ESP-IDF ≥ 5.5 | Verified（MVP-1 验收芯片，RISC-V） |
| ESP32-C3 | RISC-V 单核 | 内置 USB-Serial-JTAG；常见于低成本板 | Verified（回归板卡，已有实物） |
| ESP32-C6 | RISC-V | Wi-Fi 6 + BLE + 802.15.4；内置 USB-Serial-JTAG | Verified（回归板卡，已有实物） |
| ESP32-P4 | RISC-V 高性能 | **无射频**，无线需伴随芯片；revision 差异影响构建配置，烧录前必须核对 revision | Experimental → 实机 V3 后升级 |
| ESP32（经典） | Xtensa 双核 | 无内置 USB，需 UART 桥接（CP210x/CH340）；JTAG 需外置探针 | Experimental → 实机 V3 后升级 |

后续可按同样方式接入 S2、C2、C61、H2 等。

上表中的芯片特性用于确定适配字段，具体数值（IDF 最低版本、revision 范围等）以乐鑫官方数据手册和 ESP-IDF 支持矩阵为准，进入 Ontology 时附来源，并在 ADR-007 中固化。

**支持级别：**

- **Verified：** 有 Profile，有至少一块真实板卡在固定工具链下达到 V3，纳入回归测试；
- **Experimental：** 有 Profile，尚未完成实机 V3；UI 明确标注，烧录审批中额外提示；
- **Unsupported：** 无 Profile；只允许 R0 只读识别，并可生成 Profile 草稿供人工确认。

**芯片相关的规划约束：** Agent 规划时读取 Chip Profile 能力。例如在 ESP32-P4 上要求 Wi-Fi 功能时，必须提示需要伴随芯片方案，不能直接生成调用本机 Wi-Fi 的代码；在 C3 上不得规划 802.15.4 功能。

**切换目标芯片：** 修改 `idf_target` 属于需求变更（§8），作废全部构建产物和审批，从配置节点重编排。

### 11.3 标准流程

`Plan → Preflight（工具链 + Profile 解析）→ Build → ArtifactCheck → Approval → Lease → ChipCheck → Flash → BootWait → LogCapture → Assertions → Report`

`ChipCheck` 在持有租约后、写入前由 Runner 用 esptool 读取实际芯片与 revision，与产物目标比对，不匹配则拒绝。

失败恢复借鉴 EspSmith Self-Healing：按失败类型选择恢复动作和回退锚点，重试预算按步骤类别分别计数；恢复动作中涉及设备复位的属于 R2，需在审批范围内或重新审批。

### 11.4 验证等级

| 等级 | 含义 |
|---|---|
| V0 | 命令成功 |
| V1 | 产物存在且哈希有效 |
| V2 | 设备身份与芯片匹配，且有启动证据 |
| V3 | 预定义需求断言通过 |

产品不得把 V0 或 V1 标记为硬件验证成功。

### 11.5 板子结构观察（未实现）

工作台里的「板子调试」折叠框目前只记录串口扫描和芯片识别。下面这项不在 MVP-0 / MVP-1 里，做到之前保持为文本日志。

**目的：** 让工作台知道这块板子现在长什么样、外面插了什么，而不是只靠 USB 描述符。

**输入：**

- 用户提供的板子照片；
- 摄像头对板子的实时画面或录制。

**要显示的内容：** 在「板子调试」折叠框里给出这块板的 3D 结构，包括板型、排针和已经接上的外设。插上或拔掉 pin、杜邦线或其他外设时，视图要能看出来。

**后续用法：** 调试过程中对板子做姿态检测，把观察到的插拔和姿态写成任务上下文，用来建议或修改代码。代码修改仍走现有任务和审批，视觉结果不能单独批准烧录、擦除或改硬件参数。

**现在不做：** 不接摄像头，不建 3D 场景，不根据画面改代码。这和 MVP-2 的可视化 Pin Planner 是两件事：Pin Planner 排脚位，这里看实板上的插拔。

---

## 12. RK3588（MVP-2）

支持 Linux 用户态程序的交叉构建、SSH/SCP 部署、进程运行、退出码与日志采集。

沿用 §11.1 的 Profile 机制，每个 Board Profile 声明：

- SoC 与板型；
- 系统架构和 target triple；
- sysroot；
- 连接方式；
- 设备身份校验策略（如 SSH host key）；
- 支持的部署与验证能力。

厂商 BSP、Buildroot/Yocto 全量构建、Android 系统镜像和 MaskROM 烧录后置。

---

## 13. Ontology RAG（渐进式）

### 13.1 MVP 目标

建立可追溯的硬件知识记录，而不是先构建通用知识图谱平台。

首批实体：Chip、Board、Pin、Peripheral、Driver、Toolchain、FirmwareArtifact、Failure、TestEvidence、SourceDocument。

每个实体和关系必须包含来源、版本、适用范围（芯片、revision、板型）和采集时间。Chip/Board Profile 中的事实性字段应能追溯到 Ontology 中的来源记录。

### 13.2 检索方式

首版使用文档与代码索引、关键词检索、向量召回、元数据过滤以及明确的芯片、板型和 revision 约束。

关系存储优先使用现有持久化基础设施（关系表或结构化记录，可建在 Octos `memory/*` 之上）。是否引入专用图数据库由 ADR-008 决定。

### 13.3 回答规则

- 显示来源和适用芯片 / 板型；
- 区分已验证事实、文档推断和模型假设；
- 不把同系列其他芯片（如 S3 与 C3）或相似板型的信息当作当前目标的事实；
- 证据不足时返回未知或请用户确认。

---

## 14. 受控 RSI（渐进式）

MVP 只实现候选生成、离线评测、人工批准和版本化发布。

每个候选包含：

- 失败 fingerprint；
- 基线版本；
- 拟议修改；
- 评测集；
- 预期收益；
- 风险等级；
- 回滚方案。

首版只允许评估和发布经过审批的技能、提示词、工作流规则和 Judge 校准参数。涉及工具权限、硬件安全策略、生产执行器或 Octos 核心代码的改动一律不得自动发布。

候选信号来源：现有 `skill-evolve`、OctoLoop 进化采集、EspSmith 式 Experience 记录、Judge 判断与最终采用结果的偏差。

---

## 15. 跨平台策略

| 阶段 | 平台范围 |
|---|---|
| MVP-0 | Linux 桌面完整支持 |
| MVP-1 | Linux 桌面 + ESP32 系列本地闭环 |
| MVP-2 | Windows 桌面；通过能力协商隐藏未支持的本地工具链 |
| MVP-3 | Android 远程客户端：会话查看、事件流、干预输入、审批和证据浏览；硬件执行全部在受信任的桌面或服务器 Runner 上 |

Makepad 的平台能力、文件访问和打包方式通过早期技术 Spike 验证，不以“能编译”作为平台支持的唯一标准。

---

## 16. 安全与非功能需求

- 所有执行任务都有持久化身份和状态；
- 远程 Runner 使用节点认证和加密传输；
- 本地 Bridge、Runner 和 Judge 默认绑定 loopback；
- 外部事件、工具参数和用户干预文本一律视为不可信输入；
- 路径操作限制在授权工作区内；
- 日志和事件缓存有界，慢客户端不得阻塞硬件执行；
- 干预处理不得阻塞事件流或硬件执行；Judge 超时有上限，超时后走退化路径；
- 关键状态机用 Mock Runner 做确定性测试；
- 任务恢复不得重复非幂等副作用；
- Linux/Windows 使用同一 EvidenceBundle schema；
- 未知 Runtime 事件可见并可安全降级；
- 关键版本和配置（含 Profile 版本、Judge 模型与校准版本）可追溯。

---

## 17. MVP 交付阶段

### 阶段 0：技术可行性验证（Spike）

交付：

- **Spike 1 — Cursor SDK Bridge + Rust：** Bridge 下载校验、进程启动、认证、Agent 创建、事件流、取消和断线恢复；确认实际账户套餐、API Key 认证和用量规则；确认是否支持运行中注入。
- **Spike 2 — Octos OUP + 持久化：** 验证会话和任务能否在服务重启后恢复，外部 Runtime 的运行状态能否安全映射到产品任务；逐项实测 §5.3 的能力表，包括 `turn/steer`、`turn/steer_dropped`、`turn/interrupt`、`task/restart_from_node`。
- **Spike 3 — Mock Hardware Runner：** 不接真实板子，模拟设备租约、烧录、断线、超时、旧租约执行、芯片不匹配、运行中干预和 `OutcomeUnknown`，验证状态机和审批逻辑。
- **Spike 4 — Jev 干预分类：** 用本地 `llama-server` + 小模型运行 jev-rs，构建首批干预样本集（建议不少于 100 条，覆盖 §8.3 全部 intent），测量 Linux 上的准确率、校准误差和延迟，作为 ADR-009 的依据。
- ADR-001、ADR-002 定稿；Runtime Adapter 接口草案。

退出条件：Cursor 在未配置 Octos LLM Provider 的情况下通过官方 SDK 接口运行任务，断线后能恢复观察；Mock Runner 故障注入不产生重复副作用；干预样本集与 Jev 基线数据就绪。

### 阶段 1：最小工作台（MVP-0）

交付：

- Linux Makepad 客户端；
- 工作区、任务列表与计划视图；
- OUP 连接、事件流和恢复；
- Cursor Runtime 选择与状态展示；
- 基础任务详情、Diff 和日志查看；
- 任务与 Runtime 映射的持久化；
- **Live Steering 基础版：** 运行中输入、补充上下文、取消步骤 / 任务、暂停，`turn/steer_dropped` 重新排队；分类先用规则 + System Two。

退出条件：同一任务在客户端重启后可恢复；Runtime 错误不会导致静默切换；运行中的任何输入都有明确处理结果，不丢失。

### 阶段 2：ESP32 系列闭环（MVP-1）

交付：

- Chip / Board Profile 机制与注册表；
- S3、C3、C5、C6、P4、经典 ESP32 的 Chip Profile；ESP32-S3-DevKitC-1 与 ESP32-C5-DevKitC-1 验收板卡、C3/C6 回归板卡的 Board Profile；
- 工具链预检和构建；
- Artifact Manifest；
- 设备发现、芯片识别与租约；
- 精确烧录审批；
- ChipCheck、Flash 与启动日志验证；
- 借鉴 EspSmith 的失败恢复策略，CoreDump 解析；
- **需求变更重编排：** 计划依赖图、输入摘要失效计算、计划差异展示；
- **硬件中断边界：** §8.5 全部规则；
- **Jev 判断层：** 作为可选加速器接入，阈值来自 Spike 4；
- EvidenceBundle 报告。

退出条件：S3 与 C5 在真实硬件上端到端达到 V3，C3、C6 回归通过；新增一个 Chip Profile 不需要修改 Runner 代码；故障注入不会造成重复烧录；执行中修改需求只重做受影响节点。

### 阶段 3：稳定性、Windows 与 RK3588（MVP-2）

交付：

- Windows 客户端与原生 Runner（含 jev-rs Windows 构建验证）；
- Experimental 芯片升级为 Verified（视硬件到位情况）；
- Bridge 生命周期和升级策略；
- Runner 故障恢复；
- 并发任务与设备互斥；
- 端到端安全测试；
- 可视化 Pin Planner；
- RK3588 Linux 用户态部署闭环。

### 阶段 4：知识增强与远程客户端（MVP-3）

交付：

- Ontology 最小 schema 与来源可追溯的混合检索；
- RSI 候选与离线评测工作流（含 Judge 校准迭代）；
- Android 远程客户端（含干预输入和审批）。

Android 远程客户端按平台验证结果独立发布，不阻塞 Linux + ESP32 首个闭环。

---

## 18. 验收标准

### AC-01：Cursor Runtime

无 Octos 原生 LLM Provider 时，可创建 Cursor Agent、提交任务、观察流式事件并取消任务。认证失败和额度不足必须明确报告。

### AC-02：任务恢复

客户端重启后，任务状态、事件游标、当前节点和未处理的干预可以恢复；重复事件不会造成重复操作。

### AC-03：精确审批

修改设备 ID、artifact hash、操作类型、目标芯片或关键参数后，旧审批立即失效。

### AC-04：硬件不确定结果

烧录过程中断线后，任务进入 `OutcomeUnknown`，不会自动重复烧录；重新连接后可通过设备检查和操作收据消歧。

### AC-05：ESP32 系列端到端验证

在 ESP32-S3 和 ESP32-C5 上完成构建、审批、烧录、启动日志采集和断言，报告包含源码 revision、产物哈希、设备身份、芯片与 revision、Profile 版本和日志引用。

### AC-06：硬件互斥

两个任务同时申请同一设备的互斥操作时，只有持有有效租约的任务能执行；持有过期 token 的请求被 Runner 拒绝。

### AC-07：验证等级

仅有命令成功或固件文件存在时，系统不得报告需求验证通过。

### AC-08：知识来源

硬件问题的回答必须给出来源和适用芯片 / 板型；来源不足时明确返回未知。

### AC-09：RSI 安全

未经评测和人工批准的候选不得改变生产技能或工作流。

### AC-10：芯片匹配

把为 S3 构建的产物发往 C3 设备时，Runner 在 ChipCheck 拒绝写入并留下审计记录。

### AC-11：运行中干预不丢失

任务运行的任意阶段提交的输入，都会得到“已注入 / 已重编排 / 已排队 / 已退回 / 需确认”之一的结果；轮次结束时未被吸收的输入会被重新提交或明确退回给用户。

### AC-12：需求变更快速重编排

构建完成后修改一个只影响日志断言的需求，系统不重新构建；修改 `sdkconfig` 或目标芯片时，系统作废构建产物和审批并展示计划差异。

### AC-13：硬件中断安全

Flash 写入过程中提交“取消”，系统不强行中断写入，写入结束后停止后续节点；用户二次确认强制中断时，任务进入 `OutcomeUnknown` 且不自动重烧。

### AC-14：Judge 退化

关闭 Judge 服务或使用不支持 logprobs 的后端时，干预功能仍然可用（走规则 + System Two）；Judge 输出从不出现在任何硬件审批依据中。

---

## 19. 成功指标

所有指标必须先建立基线，并固定测试集、样本数和统计周期；未定义口径的指标不作为验收依据。

| 指标 | 目标 |
|---|---|
| ESP32 系列端到端闭环成功率（按芯片分别统计） | 基线建立后设定 |
| 从需求提交到 V3 验证通过的中位时间 | 基线建立后设定 |
| 错误设备、错误芯片或错误产物写入次数 | 0 |
| 因恢复导致的重复非幂等操作次数 | 0 |
| `OutcomeUnknown` 正确消歧率 | 基线建立后设定 |
| 任务恢复成功率 | 基线建立后设定 |
| 干预输入丢失次数 | 0 |
| 干预从提交到给出处理结果的 p50 / p95 延迟 | 基线建立后设定（分 Jev 与退化路径） |
| Jev 干预分类准确率与高置信覆盖率 | 基于 Spike 4 数据集设定 |
| 需求变更后不必要的重做节点比例 | 基线建立后设定 |
| 新增 Chip Profile 所需的代码改动行数 | 目标为 0 |
| 有有效来源的硬件知识回答比例 | 基线建立后设定 |
| Cursor Runtime 任务成功率与额度消耗 | 记录并对比 Octos 原生路径 |
| 人工绕过系统执行设备命令的次数 | 持续下降 |

---

## 20. 首批 ADR

1. **ADR-001：** Octos 上游版本、Git pin 和集成 fork 策略。
2. **ADR-002：** 产品级任务状态与 Octos OUP 会话的所有权边界：哪个服务写任务状态、谁持有事件游标、重启后由谁恢复执行。
3. **ADR-003：** Cursor SDK Bridge 的版本锁定、认证和生命周期管理。
4. **ADR-004：** Agent Runtime Adapter 的规范化事件模型（含 Steer / Interrupt capability）。
5. **ADR-005：** Hardware Supervisor、Runner 和设备租约协议。
6. **ADR-006：** EvidenceBundle schema 与产物存储。
7. **ADR-007：** ESP32 系列 Chip / Board Profile schema、首批验收板卡与 ESP-IDF 版本范围。
8. **ADR-008：** Ontology 存储、来源和版本策略。
9. **ADR-009：** Judge Service：jev-rs 部署方式、后端模型、置信度阈值、校准与退化策略。
10. **ADR-010：** 干预语义：intent 集合、计划依赖图与输入摘要规则、硬件节点中断策略。
11. **ADR-011：** 本地持久化（SQLite、乐观并发、事件日志、审计哈希链、崩溃恢复）。
12. **ADR-012：** 每个可写 Run 一个 git worktree，单写者锁，审阅后合并。
13. **ADR-013：** ESP-IDF 工具链发现（EIM 优先）、按芯片版本准入、Profile 驱动的配置默认值。

全文见 [docs/adr](adr/)。

---

## 21. 最终交付定义

livstyle MVP 不以“聊天界面完成”“模型能生成代码”或“命令返回零退出码”作为交付成功。

MVP 的完成标准是：

**用户能够从 Linux 桌面发起一个 ESP32 系列开发任务，系统在 Cursor Runtime 或 Octos 原生 Runtime 下完成代码工作，经过明确的硬件授权和芯片核对执行烧录，并用可追溯的设备证据验证结果；执行过程中用户可以随时打断和修改需求，系统只重做受影响的部分；在断线、崩溃和结果未知的情况下，系统仍能安全恢复，且不会盲目重复硬件副作用。**
