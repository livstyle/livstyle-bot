# livstyle 产品需求文档

**版本：** v0.2  
**状态：** Proposed / 待 ADR 与技术 Spike 确认  
**日期：** 2026-10-18  
**产品定位：** 可验证的硬件开发智能体工作台  
**首发场景：** Linux 桌面 + 单一 ESP32 板型的开发、构建、烧录与板上验证  
**核心技术：** Rust + Octos + Makepad + Cursor SDK Bridge + Hardware Supervisor/Runner  
**上一版本：** [v0.1](archive/PRD-v0.1.md)

---

## 0. 版本说明

### 0.1 v0.2 相对 v0.1 的主要变化

| 变化 | v0.1 | v0.2 |
|---|---|---|
| 产品定位 | 跨平台超级智能体，首个方向为 IoT | 先做“可验证的硬件开发智能体工作台”，超级智能体作为长期愿景 |
| MVP 范围 | 三平台 + ESP32 + RK3588 + Ontology + RSI 同时进入 MVP | 拆为 MVP-0 至 MVP-3，首个可交付闭环只含 Linux + ESP32 |
| 状态所有权 | Octos 与 livstyle 的任务职责有重叠 | 产品级任务只有一个真源（livstyle Product Service） |
| 硬件安全 | 设备租约与审批 | 新增独立 Hardware Supervisor；Runner 必须校验 fencing token；引入操作意图与执行收据 |
| Cursor | 作为 Agent Runtime 接入 | 增加认证、额度、IDE 登录与 SDK API Key 分离、禁止静默切换等要求；Cursor 路径成为独立验收项 |
| 验证 | 区分命令成功与需求验证 | 正式定义验证等级 V0–V3 与操作风险等级 R0–R4 |
| Ontology / RSI | MVP 内实现 | 先做可追溯知识记录与候选离线评测，图推理和自动发布后置 |
| 指标 | 只列指标名 | 要求固定基线、测试集、样本数与统计周期 |
| 现状核对 | 无 | 新增 Octos 现有能力核对表，区分“已有”和“需要扩展” |

### 0.2 评审意见对照

本版依据外部评审意见整理。评审认为 v0.1 方向正确、技术意识成熟，但更接近产品愿景与架构草案，尚不能直接按阶段实现和验收。对应处理如下：

| 优先级 | 评审问题 | 本版处理位置 |
|---|---|---|
| P0 | Octos、Cursor、硬件 Runner 执行职责不清 | §5 系统架构与所有权 |
| P0 | Cursor 套餐复用的认证与计费边界不清 | §8 Cursor Runtime |
| P0 | 烧录安全缺少完整的并发与故障恢复协议 | §7、§9 |
| P0 | MVP 横跨过多平台与技术领域 | §3、§14、§16 |
| P1 | OUP、Bridge 与产品任务的状态映射不完整 | §6、§7.3、ADR-002/004 |
| P1 | Ontology 与 RSI 同时做大 | §12、§13 |
| P1 | 验收指标缺少基线与统计口径 | §18 |
| P1 | 多 Agent 并行与硬件独占冲突 | §8.3、§9.1 |

---

## 1. 产品定义

livstyle 是一个面向物联网和嵌入式开发的智能体工作台。

用户用自然语言提出开发目标，livstyle 将其转换为可执行、可恢复、可审计的任务，协调 Agent、开发工具链和真实硬件，完成代码修改、构建、部署、调试和验证。

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
7. **Incremental delivery：** 每个阶段都能独立运行和验收。

---

## 2. 目标用户与核心场景

### 2.1 主要用户

- ESP-IDF / ESP32 固件开发者
- RK3588 Linux / Android 边缘设备开发者
- IoT 创业团队与小型硬件团队
- 自动化硬件在环测试工程师

### 2.2 首发场景

用户提出：

> “检查当前 ESP32 项目，修复启动问题，构建固件，在连接的开发板上运行并验证启动日志。”

系统应能：

1. 检查项目和工具链；
2. 制定任务计划并识别风险；
3. 修改代码并记录 Diff；
4. 构建固件，保存产物和哈希；
5. 识别目标设备并获取独占租约；
6. 展示精确的烧录授权请求；
7. 在授权后执行烧录；
8. 采集启动日志并执行预定义断言；
9. 生成带设备身份、产物、源码版本和日志证据的报告。

无法确定设备状态时，系统必须报告“未知”，不得推断成功，也不得盲目重试。

---

## 3. MVP 目标

### G1：完成真实硬件闭环

在一个受支持的 ESP32 板型上完成构建、烧录、启动验证和证据归档。

### G2：支持可靠任务恢复

支持任务持久化、事件重放、取消、失败恢复和 `OutcomeUnknown` 状态。

### G3：复用 Cursor Agent Runtime

在未配置 Octos 原生 LLM Provider 的情况下，通过官方 Cursor SDK Bridge 创建和运行 Agent，观察事件、取消任务并恢复观察。

Cursor 路径遵循官方 SDK 的认证、计费和额度规则，不依赖非官方私有推理接口。**只有在真实账户、真实套餐下跑通后，才能把该能力写成已实现的产品承诺。**

### G4：保证硬件操作安全

所有危险操作必须绑定设备身份、产物哈希、操作参数、审批有效期和执行记录。

### G5：建立可扩展的硬件工具架构

以插件或 capability 方式增加工具链、板卡和 Runner，不把设备逻辑写进 UI 或 Agent 内核。

---

## 4. 非目标

首版不实现：

- Android 本地执行完整嵌入式工具链；
- 通用办公、邮件、PPT 和浏览器自动化套件；
- 任意开发板的自动识别与自动适配；
- 把特权 USB、JTAG 或 `/dev` 直接暴露给 Agent；
- 自动执行 eFuse、OTP、Secure Boot 密钥写入和整盘擦除；
- 将 Cursor 订阅转换为通用 OpenAI 兼容模型 API；
- 无审批地修改或发布生产内核代码；
- 无界自我修改、自主发布或不可回滚的 RSI；
- 在 MVP 中实现完整知识图谱平台。

---

## 5. 系统架构与职责

### 5.1 模块划分

**A. Makepad Client**

负责工作区、会话、任务列表、事件流、审批界面、设备状态和证据报告。只保存 UI 状态和未提交草稿，不拥有任务执行的权威状态。

**B. livstyle Product Service**

负责产品级任务、硬件工作流编排、审批记录、跨 Runtime 映射、EvidenceBundle 发布和产品级任务恢复。

**C. Octos Runtime**

负责原生 Agent 执行、上下文、记忆、原生工具、技能、工作流和 OUP 会话。优先复用 Octos 现有能力；只有现有能力无法满足持久化或恢复要求时才扩展内核（见 §5.3）。

**D. Agent Runtime Adapter**

统一的外部接口：

- Discover / Capabilities
- Create / Resume
- Send / Steer
- Observe / Stream
- Cancel
- Close
- GetUsage（仅当 Runtime 支持）

首批适配器为 `OctosNative` 和 `CursorBridge`。Adapter 只做协议转换，不拥有产品级任务的最终状态。

**E. Cursor Bridge Manager**

负责 Bridge 二进制校验、版本锁定、启动、握手、鉴权、健康检查、能力协商、退出和升级。Rust 侧通过官方 `sdk.v1` 协议接入，Bridge 的 RPC、事件和错误映射为 livstyle 内部规范化类型。

**F. Hardware Supervisor**

确定性安全边界：设备身份、租约、操作授权、执行状态机和安全拒绝。Agent 无权修改 Supervisor 的策略或覆盖其拒绝结果。

**G. Hardware Runner**

负责本机工具链和真实设备操作，包括 ESP-IDF、esptool、串口、OpenOCD、SSH 和 ADB。Runner 是实际操作设备的可信执行边界；远程 Runner 必须经过认证，并自行验证设备租约与 fencing token。

**H. Evidence Store**

保存任务事件、操作收据、产物元数据、日志引用、测试断言和最终报告。大日志和二进制产物外置存储，事件流只保存引用、摘要和校验值。

### 5.2 所有权规则

| 状态 | 唯一所有者 |
|---|---|
| Octos 原生会话、turn、原生工具执行 | Octos Runtime |
| Cursor Agent / Run | Cursor Runtime（经 Cursor Adapter 映射） |
| 产品级开发任务 | livstyle Product Service |
| 设备租约与硬件操作状态 | Hardware Supervisor |
| EvidenceBundle 发布状态 | Evidence Store / Product Service |

附加规则：

- “产品级任务”与“Agent Run”不是同一对象。一个 ESP32 任务可以包含多个 Octos 节点和多个 Cursor Run，但只有一份产品级任务状态和一份设备授权记录。
- UI 不得根据聊天文本推断任务成功。
- Runtime 切换不得静默发生；切换时记录原 Runtime、原因和新的会话映射。

### 5.3 Octos 现有能力核对

以下依据本地 `octos` 仓库（`2.0.3-rc.13`）核对，用于防止把目标架构误认为已有能力。实施前仍需在 Spike 2 中逐项实测。

| 能力 | 现状 | 依据 | 对 livstyle 的含义 |
|---|---|---|---|
| OUP 会话、turn、approval、hydrate/replay | 已有 | `octos-core/src/ui_protocol.rs` | 客户端直接复用 |
| 无 LLM Provider 启动 OUP 会话 | **不支持** | `octos-cli/src/runtime/profile.rs:1033` 报错 “has no LLM provider configured” | Cursor-only 模式不能靠普通 `octos serve` 实现，需 Adapter 并列编排或扩展 Octos |
| 宿主代理推理（子进程不持有凭据） | 部分已有 | `octos acp --host-managed` + `octos-llm/src/host.rs`（v1 为单次有界补全，非流式） | 可作为后续“外部推理驱动”的参考，不能直接承载 Cursor Agent 语义 |
| 宿主注册工具 | 已有 | `peer/tools/register`（UPCR-035，`ui_protocol_transport.rs`） | 硬件工具可由 livstyle 注册给 Octos Agent 调用 |
| 任务从节点重启 | 已有 | `task/restart_from_node` | 可用于软件步骤；硬件步骤仍需 Supervisor 状态机 |
| 记忆检索 | 已有 | `memory/search`、`memory/load`、`memory/ingest` | Ontology 首版可建在其上 |
| 技能自我修正 | 仅建议 | `app-skills/skill-evolve`：生成 SKILL.md 补丁，人工应用 | 可作为 RSI E0 候选来源，不等于 RSI |
| 外部 Agent 驱动协议 | 未进入 Core | octoscode-web 有 `external_driver_v1` 客户端草案，Core 未收录 | 需走 UPCR，不在 MVP 依赖 |

---

## 6. 关键数据模型

### 6.1 ProductTask

- `task_id`
- `workspace_id`
- `session_id`
- `runtime_id`
- `runtime_session_id`
- `status`
- `current_node_id`
- `created_at` / `updated_at`
- `version`
- `parent_task_id`（可选）

所有状态变更使用持久化版本号或等效并发控制，避免旧执行者覆盖新状态。

### 6.2 DeviceIdentity

- `device_id`
- `runner_id`
- `transport`
- `serial_number` 或平台适用的身份字段
- `chip_id`（如 ESP32 MAC）
- `board_profile_id`
- `board_revision`
- `identity_confidence`
- `last_seen_at`

USB 路径（如 `/dev/ttyUSB0`、`COM3`）只是连接信息，不作为设备身份。

### 6.3 DeviceLease

- `device_id`
- `lease_id`
- `task_id`
- `fencing_token`
- `expires_at`
- `runner_id`
- `state`

续期和释放必须做所有权检查。过期持有者不得发起新的设备写操作。

### 6.4 OperationIntent 与 OperationReceipt

`OperationIntent`：

- `operation_id`
- `task_id`
- `device_id`
- `runner_id`
- `operation_type`
- `risk_level`
- `artifact_sha256`
- `parameters_digest`
- `approval_id`
- `fencing_token`
- `state`
- `started_at` / `completed_at`

`OperationReceipt` 由 Runner 生成，记录实际执行的命令摘要、退出状态、关键输出摘要和执行时使用的 fencing token。

### 6.5 EvidenceBundle

- `evidence_id`
- `task_id`
- `source_revision`
- `toolchain_manifest`
- `artifact_manifest`
- `device_identity`
- `operation_receipts`
- `log_references`
- `assertion_results`
- `verification_level`
- `created_at`

所有产物以 SHA-256 或同等强度哈希绑定。报告必须区分命令成功、设备启动和需求验证通过。

---

## 7. 任务状态机与恢复

### 7.1 产品级任务状态

主路径：

`Queued → Planning → Running ⇄ AwaitingApproval → Verifying → Succeeded`

异常状态：`Failed`、`Cancelled`、`Blocked`、`OutcomeUnknown`。

状态转换只由明确的事件和前置条件驱动，不允许仅凭模型的自然语言输出改变状态。

### 7.2 硬件操作状态

主路径：

`Prepared → Authorized → Dispatched → Running → Completed`

异常分支：`Rejected`、`Failed`、`OutcomeUnknown`、`Reconciled`。

执行前持久化操作意图，执行后持久化收据。结果无法确认的操作不得自动重新派发。

### 7.3 恢复要求

- 每个任务事件有稳定 ID 或任务内单调序号；
- 客户端维护最后确认的事件游标；
- 重连后通过 hydrate + replay 恢复视图；
- 重复事件幂等处理；
- 事件缺口触发补拉或完整快照；
- 取消任务不代表正在执行的外部操作已停止；
- 设备操作结果未知时，先查询设备和 Runner 状态，再由确定性策略或用户决策消歧。

---

## 8. Cursor Runtime

### 8.1 接入方式

采用官方 [Cursor SDK Bridge](https://github.com/cursor/sdk-bridge) 的 `sdk.v1` 协议实现 Rust Adapter。Cursor 被建模为外部 Agent Runtime，而不是普通模型 Provider，否则会混淆两套执行循环。

最小能力：

1. Bridge 版本与 capability 检查（`GetVersion`）；
2. Agent 创建与恢复；
3. Run 发送、事件流和取消；
4. Runtime 错误与额度信息映射；
5. Run ID、Agent ID 和事件 offset 持久化；
6. Bridge 进程退出后通过 offset 恢复观察。

模型目录、自定义工具回调和云端 artifacts 作为后续增强项。

### 8.2 认证与费用

- 用户 API Key 由系统安全存储或受控进程环境管理；
- Bridge 本地 bearer token 每次启动独立生成，不与 Cursor API Key 混用；
- 凭据不得写入项目文件、普通日志或 Agent 上下文；
- **Cursor IDE 已登录不等于 SDK 可调用**，登录状态和 SDK API Key 必须分开建模；
- 启动前检查认证；运行中明确报告鉴权失败、额度不足和服务不可用；
- 不承诺套餐额度无限使用，计费与额度遵循 Cursor 规则；
- 认证失败或额度耗尽时明确失败，由用户主动选择其他 Runtime，不得静默切换或重复执行；
- 不支持的操作通过 capability 显示为不可用。

### 8.3 执行边界

- Cursor Agent 负责自身的 Agent 执行循环；Octos 负责原生 Agent 执行循环；跨 Runtime 的开发流程由 livstyle 任务编排层管理。
- 软件分析和代码生成可以并行；同一步骤不得由 Octos 和 Cursor 同时对同一工作目录做冲突写入，工作区写入通过任务锁或隔离工作树控制。
- 硬件操作始终由 Hardware Supervisor 和 Runner 执行，Cursor 内置工具不得绕过审批。

### 8.4 平台约束

官方 Bridge 当前只发布 Linux（x64/arm64）和 Windows（x64）版本。Android 不在本地运行 Bridge，只能通过受信任的桌面或服务器节点远程使用 Cursor Runtime，且 APK 中不得包含 Cursor 凭据。

---

## 9. 硬件安全与设备管理

### 9.1 设备租约

同一设备的串口、JTAG、烧录等互斥接口由统一设备租约协调：

- 同一设备的互斥操作必须串行化；
- 每次操作必须验证当前 fencing token，**且由实际执行操作的 Runner 校验**——只在服务端生成递增数字而 Runner 不验证，无法阻止旧任务在租约过期后继续写设备；
- Runner 拒绝已过期或被更高 token 取代的操作；
- 断线或超时后，租约不得自动触发危险操作重试。

### 9.2 精确审批

烧录审批至少显示：

- 设备稳定 ID、芯片和板型；
- Runner 身份；
- 产物 SHA-256；
- 操作类型；
- 分区、地址范围和关键参数；
- 风险等级；
- 审批过期时间。

审批绑定不可变的操作参数摘要，任何关键参数改变都要求重新审批。

### 9.3 操作风险等级

| 等级 | 操作 | MVP 策略 |
|---|---|---|
| R0 | 只读检查、日志读取 | 允许 |
| R1 | 可恢复的软件修改、普通构建 | 允许 |
| R2 | 设备部署、普通 Flash 写入 | 需精确审批 |
| R3 | 擦除、引导配置变更 | 默认拒绝 |
| R4 | eFuse、OTP、密钥烧录、不可逆安全配置 | 默认拒绝 |

R3/R4 需要在后续版本经过独立安全评审并引入专用审批流程后才能开放。

### 9.4 安全不变量

- 未授权的操作不得派发；
- 未确认设备身份不得写入设备；
- 产物哈希变化后不得沿用旧审批；
- 普通 Flash 授权不能升级为 Erase 授权；
- 超时不代表操作失败，断线不代表外部动作已停止；
- 断线后不得盲目重试非幂等操作；
- 旧租约持有者不得继续写设备；
- Agent 不得通过重新生成命令绕过状态机，也不得修改或覆盖 Supervisor 的拒绝；
- 所有拒绝和恢复决策留下审计记录。

---

## 10. ESP32 开发闭环

### 10.1 首发支持范围

- 一个明确型号的 ESP32 开发板；
- 一个固定版本范围的 ESP-IDF 工具链；
- 本机 Linux Runner；
- 构建、普通 Flash 写入、串口日志采集；
- 预定义启动日志断言。

具体芯片型号、板型、revision 和工具链版本由 ADR-007 固化。

### 10.2 标准流程

`Plan → Preflight → Build → ArtifactCheck → Approval → Lease → Flash → BootWait → LogCapture → Assertions → Report`

### 10.3 验证等级

| 等级 | 含义 |
|---|---|
| V0 | 命令成功 |
| V1 | 产物存在且哈希有效 |
| V2 | 设备身份匹配且有启动证据 |
| V3 | 预定义需求断言通过 |

产品不得把 V0 或 V1 标记为硬件验证成功。

---

## 11. RK3588（MVP-2）

支持 Linux 用户态程序的交叉构建、SSH/SCP 部署、进程运行、退出码与日志采集。

每个 Board Profile 声明：

- SoC 与板型；
- 系统架构和 target triple；
- sysroot；
- 连接方式；
- 设备身份校验策略（如 SSH host key）；
- 支持的部署与验证能力。

厂商 BSP、Buildroot/Yocto 全量构建、Android 系统镜像和 MaskROM 烧录后置。

---

## 12. Ontology RAG（渐进式）

### 12.1 MVP 目标

建立可追溯的硬件知识记录，而不是先构建通用知识图谱平台。

首批实体：Board、SoC、Pin、Peripheral、Driver、Toolchain、FirmwareArtifact、Failure、TestEvidence、SourceDocument。

每个实体和关系必须包含来源、版本、适用范围和采集时间。

### 12.2 检索方式

首版使用文档与代码索引、关键词检索、向量召回、元数据过滤以及明确的板型和 revision 约束。

关系存储优先使用现有持久化基础设施（关系表或结构化记录，可建在 Octos `memory/*` 之上）。是否引入专用图数据库由 ADR-008 决定。

### 12.3 回答规则

- 显示来源和适用板型；
- 区分已验证事实、文档推断和模型假设；
- 不把相似板型的信息当作当前板型的事实；
- 证据不足时返回未知或请用户确认。

---

## 13. 受控 RSI（渐进式）

MVP 只实现候选生成、离线评测、人工批准和版本化发布。

每个候选包含：

- 失败 fingerprint；
- 基线版本；
- 拟议修改；
- 评测集；
- 预期收益；
- 风险等级；
- 回滚方案。

首版只允许评估和发布经过审批的技能、提示词和工作流规则。涉及工具权限、硬件安全策略、生产执行器或 Octos 核心代码的改动一律不得自动发布。

现有 `skill-evolve` 与 OctoLoop 进化采集可作为候选信号来源。

---

## 14. 跨平台策略

| 阶段 | 平台范围 |
|---|---|
| MVP-0 | Linux 桌面完整支持 |
| MVP-1 | Linux 桌面 + ESP32 本地闭环 |
| MVP-2 | Windows 桌面；通过能力协商隐藏未支持的本地工具链 |
| MVP-3 | Android 远程客户端：会话查看、事件流、审批和证据浏览；硬件执行全部在受信任的桌面或服务器 Runner 上 |

Makepad 的平台能力、文件访问和打包方式通过早期技术 Spike 验证，不以“能编译”作为平台支持的唯一标准。

---

## 15. 安全与非功能需求

- 所有执行任务都有持久化身份和状态；
- 远程 Runner 使用节点认证和加密传输；
- 本地 Bridge 和 Runner 默认绑定 loopback；
- 外部事件和工具参数一律视为不可信输入；
- 路径操作限制在授权工作区内；
- 日志和事件缓存有界，慢客户端不得阻塞硬件执行；
- 关键状态机用 Mock Runner 做确定性测试；
- 任务恢复不得重复非幂等副作用；
- Linux/Windows 使用同一 EvidenceBundle schema；
- 未知 Runtime 事件可见并可安全降级；
- 关键版本和配置可追溯。

---

## 16. MVP 交付阶段

### 阶段 0：技术可行性验证（Spike）

交付：

- **Spike 1 — Cursor SDK Bridge + Rust：** Bridge 下载校验、进程启动、认证、Agent 创建、事件流、取消和断线恢复；确认实际账户套餐、API Key 认证和用量规则。
- **Spike 2 — Octos OUP + 持久化：** 验证会话和任务能否在服务重启后恢复，外部 Runtime 的运行状态能否安全映射到产品任务；逐项实测 §5.3 的能力表。
- **Spike 3 — Mock Hardware Runner：** 不接真实板子，模拟设备租约、烧录、断线、超时、旧租约执行和 `OutcomeUnknown`，验证状态机和审批逻辑。
- ADR-001、ADR-002 定稿；Runtime Adapter 接口草案。

退出条件：Cursor 在未配置 Octos LLM Provider 的情况下通过官方 SDK 接口运行任务，断线后能恢复观察；Mock Runner 故障注入不产生重复副作用。

### 阶段 1：最小工作台（MVP-0）

交付：

- Linux Makepad 客户端；
- 工作区与任务列表；
- OUP 连接、事件流和恢复；
- Cursor Runtime 选择与状态展示；
- 基础任务详情和日志查看；
- 任务与 Runtime 映射的持久化。

退出条件：同一任务在客户端重启后可恢复；Runtime 错误不会导致静默切换。

### 阶段 2：ESP32 闭环（MVP-1）

交付：

- 一个固定板型的 Board Profile；
- 工具链预检和构建；
- Artifact Manifest；
- 设备发现与租约；
- 精确烧录审批；
- Flash 与启动日志验证；
- EvidenceBundle 报告。

退出条件：端到端测试通过，故障注入不会造成重复烧录。

### 阶段 3：稳定性、Windows 与 RK3588（MVP-2）

交付：

- Windows 客户端与原生 Runner；
- Bridge 生命周期和升级策略；
- Runner 故障恢复；
- 并发任务与设备互斥；
- 端到端安全测试；
- RK3588 Linux 用户态部署闭环。

### 阶段 4：知识增强与远程客户端（MVP-3）

交付：

- Ontology 最小 schema 与来源可追溯的混合检索；
- RSI 候选与离线评测工作流；
- Android 远程客户端。

Android 远程客户端按平台验证结果独立发布，不阻塞 Linux + ESP32 首个闭环。

---

## 17. 验收标准

### AC-01：Cursor Runtime

无 Octos 原生 LLM Provider 时，可创建 Cursor Agent、提交任务、观察流式事件并取消任务。认证失败和额度不足必须明确报告。

### AC-02：任务恢复

客户端重启后，任务状态、事件游标和当前节点可以恢复；重复事件不会造成重复操作。

### AC-03：精确审批

修改设备 ID、artifact hash、操作类型或关键参数后，旧审批立即失效。

### AC-04：硬件不确定结果

烧录过程中断线后，任务进入 `OutcomeUnknown`，不会自动重复烧录；重新连接后可通过设备检查和操作收据消歧。

### AC-05：ESP32 端到端验证

系统完成构建、审批、烧录、启动日志采集和断言，报告包含源码 revision、产物哈希、设备身份和日志引用。

### AC-06：硬件互斥

两个任务同时申请同一设备的互斥操作时，只有持有有效租约的任务能执行；持有过期 token 的请求被 Runner 拒绝。

### AC-07：验证等级

仅有命令成功或固件文件存在时，系统不得报告需求验证通过。

### AC-08：知识来源

硬件问题的回答必须给出来源和适用板型；来源不足时明确返回未知。

### AC-09：RSI 安全

未经评测和人工批准的候选不得改变生产技能或工作流。

---

## 18. 成功指标

所有指标必须先建立基线，并固定测试集、样本数和统计周期；未定义口径的指标不作为验收依据。

| 指标 | 目标 |
|---|---|
| ESP32 端到端闭环成功率 | 基线建立后设定 |
| 从需求提交到 V3 验证通过的中位时间 | 基线建立后设定 |
| 错误设备或错误产物写入次数 | 0 |
| 因恢复导致的重复非幂等操作次数 | 0 |
| `OutcomeUnknown` 正确消歧率 | 基线建立后设定 |
| 任务恢复成功率 | 基线建立后设定 |
| 有有效来源的硬件知识回答比例 | 基线建立后设定 |
| Cursor Runtime 任务成功率与额度消耗 | 记录并对比 Octos 原生路径 |
| 人工绕过系统执行设备命令的次数 | 持续下降 |

---

## 19. 首批 ADR

1. **ADR-001：** Octos 上游版本、Git pin 和集成 fork 策略。
2. **ADR-002：** 产品级任务状态与 Octos OUP 会话的所有权边界：哪个服务写任务状态、谁持有事件游标、重启后由谁恢复执行。
3. **ADR-003：** Cursor SDK Bridge 的版本锁定、认证和生命周期管理。
4. **ADR-004：** Agent Runtime Adapter 的规范化事件模型。
5. **ADR-005：** Hardware Supervisor、Runner 和设备租约协议。
6. **ADR-006：** EvidenceBundle schema 与产物存储。
7. **ADR-007：** 首发 ESP32 板型和工具链版本。
8. **ADR-008：** Ontology 存储、来源和版本策略。

---

## 20. 最终交付定义

livstyle MVP 不以“聊天界面完成”“模型能生成代码”或“命令返回零退出码”作为交付成功。

MVP 的完成标准是：

**用户能够从 Linux 桌面发起一个 ESP32 开发任务，系统在 Cursor Runtime 或 Octos 原生 Runtime 下完成代码工作，经过明确的硬件授权执行烧录，并用可追溯的设备证据验证结果；在断线、崩溃和结果未知的情况下，系统仍能安全恢复，且不会盲目重复硬件副作用。**
