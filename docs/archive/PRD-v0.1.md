# livstyle 产品需求文档

状态：Draft v0.1  
日期：2026-10-18  
产品名：livstyle  
首要方向：物联网硬件开发智能体

## 1. 产品定义

livstyle 是一个跨平台超级智能体工作台。它以 Octos 为智能体内核，以 Makepad
构建 Windows、Linux、Android 等平台的原生 UI，首个垂直方向是 ESP32、
RK3588 等开发板的端到端开发。

用户用自然语言描述目标后，livstyle 可以规划任务、修改代码、构建、申请硬件
操作授权、烧录、采集日志、调试并验证真实设备结果。它不是只生成代码的聊天
窗口，而是把“需求到硬件证据”做成可恢复、可审计的闭环。

长期上，livstyle 也可承载办公、研究和个人自动化等通用智能体能力，但 MVP
不以覆盖所有通用场景为目标。

## 2. 用户问题

当前硬件项目开发存在以下断点：

1. AI 生成代码后，用户仍需手工拼接 SDK、构建、烧录、串口和调试工具。
2. 串口、JTAG、ADB、SSH 和不同厂商 SDK 缺少统一的设备身份与任务状态。
3. AI 容易把“命令成功”误判为“设备工作正常”，缺少板上证据。
4. 烧录、擦除、eFuse、整盘镜像等操作有不可逆风险，普通 shell 审批粒度不足。
5. 硬件资料散落在数据手册、原理图、BSP、历史故障和项目代码中，传统向量 RAG
   难以表达芯片、引脚、外设、总线和约束关系。
6. 智能体从失败中学习的过程缺少评测门、审批、发布与回滚，不能安全地自动演进。
7. Octos 当前通常要求 LLM provider；用户希望复用 Cursor 等已登录的 Agent
   服务，而不是重复配置模型凭据。

## 3. 目标用户

### 3.1 主要用户

- ESP-IDF / ESP32 固件开发者
- RK3588 Linux、Android 和边缘 AI 应用开发者
- IoT 创业团队与小型硬件团队
- 需要自动化硬件在环测试的固件工程师

### 3.2 次要用户

- 教育、创客和实验室用户
- 远程管理开发板或设备农场的团队
- 希望通过 Cursor、Octos 或其他 Agent Runtime 协同开发的高级用户

## 4. 产品目标

### G1：一句话启动可验证的硬件开发任务

用户输入目标，系统形成计划并完成代码、构建、烧录、启动和验证，最终给出带
文件、固件哈希、设备身份和日志断言的证据。

### G2：同一界面管理多个并行任务

像 WorkBuddy 一样，用户可启动、切换和观察多个任务；任务在切换视图后继续由
服务端维护，审批和问题仍绑定原任务。

### G3：像 GrokBot Bridge 一样调用 Cursor

用户可选择 Cursor Agent Runtime 执行编程任务。livstyle 和 Octos 不要求为
此路径另配传统 LLM provider，并保留结构化运行事件、取消、恢复和产物语义。

### G4：真实硬件操作默认安全

任何破坏性操作都必须绑定精确设备、产物哈希和参数。超时或断连不得盲目重试。

### G5：积累可推理的硬件知识

系统将数据手册、原理图、设备树、sdkconfig、寄存器、故障与验证结果组织为
有来源、可版本化的硬件本体，并结合文本检索回答问题。

### G6：建立受控 RSI 演进闭环

系统能发现重复失败、提出改进、在隔离环境评测，并由用户批准后发布；任何改进
都可回滚，不允许运行时无界自改。

## 5. 非目标

MVP 明确不做：

- 在 Android 设备上本地运行完整 ESP-IDF、OpenOCD 或 RK3588 BSP 构建。
- 让 LLM 进入毫秒级硬件控制环或覆盖安全监督器。
- 直接把 USB、JTAG 或特权 `/dev` 暴露给通用 Agent shell。
- 自动执行 eFuse/OTP、Secure Boot、整盘擦除或 RK3588 MaskROM 镜像烧录。
- 以非官方 Cursor 私有推理接口伪装 OpenAI API。
- 无审批地自动修改、合并和发布 livstyle/Octos 内核代码。
- 首版实现通用办公套件、邮件、PPT、浏览器 Computer Use 全集。

## 6. 核心体验

### 6.1 工作区与会话

- 用户连接本地或远程 Octos 实例。
- 一个工作区对应服务端可访问的项目路径。
- 一个工作区可拥有多个独立会话和后台任务。
- 会话支持重连、hydrate、事件 replay、取消和从节点恢复。
- 客户端只保存 UI 状态和草稿，Octos 是执行与会话真源。

### 6.2 智能任务

用户可：

- 输入自然语言任务并附加文件、图片和设备。
- 查看计划、运行步骤、工具调用、diff、审批和验证证据。
- 中途追加指令、回答问题、停止任务或从失败节点重试。
- 将子任务委派给 Octos peer、Cursor Runtime 或其他 Agent Runtime。

### 6.3 Cursor 模式

- 设置页可发现并连接本机 Cursor SDK Bridge。
- 用户选择 Cursor 可用模型和工作目录。
- livstyle 显示 Cursor Agent、Run、流式事件、取消和恢复状态。
- Cursor 凭据由桌面安全存储或 Bridge 环境管理，不进入项目文件。
- Android 通过受信任的桌面/服务器节点远程使用 Cursor，不存放 Cursor 密钥。
- Cursor 不可用时，用户可切换 Octos 原生模型或其他 Agent Runtime。

### 6.4 硬件工作台

提供：

- 设备列表、热插拔、稳定身份、连接方式和占用状态。
- ESP32 串口、USB-JTAG、ESP-IDF、esptool、OpenOCD/GDB 状态。
- RK3588 SSH、ADB、系统信息、部署目标和日志入口。
- 构建产物、固件分区、哈希、工具链与源码 revision。
- 串口终端、结构化日志、断言、崩溃和 CoreDump 入口。
- 设备租约、任务队列、审批和证据包。

### 6.5 ESP32 闭环

标准流程：

```text
规划 → 环境预检 → 构建 → 产物检查 → 烧录审批 → 设备租约
→ 烧录 → 等待启动 → 串口/探针验证 → 报告
```

失败时区分：

- 产品代码失败
- 工具链失败
- 设备连接失败
- 测试夹具失败
- 操作结果未知

### 6.6 RK3588 闭环

MVP 先支持：

- Linux `aarch64` 用户态项目交叉构建
- SSH/SCP 部署与 `gdbserver`
- Android NDK 构建、ADB 部署与 logcat 验证

厂商 BSP、Buildroot/Yocto、Loader/MaskROM 和整机镜像烧录后置。

### 6.7 Ontology RAG

系统应理解而不只是匹配以下关系：

- Board `uses` SoC
- SoC `has` Peripheral
- Pin `supports` AlternateFunction
- Driver `controls` Peripheral
- FirmwareArtifact `targets` BoardRevision
- Failure `observed_on` Toolchain/Board/Firmware
- TestEvidence `verifies` Requirement

回答必须显示来源、版本、适用板型和置信度。没有证据时明确说明未知，不把跨板型
信息合并成事实。

### 6.8 受控 RSI

演进对象分级：

1. 提示词、规则和技能说明
2. 工作流和工具参数策略
3. Ontology 映射与检索策略
4. livstyle 插件/sidecar 代码
5. Octos 核心代码

级别越高，需要越严格的离线回放、仿真、硬件 canary、人工审批和签名发布。
首版只自动产生候选，不自动发布 4–5 级改动。

## 7. 功能需求

### FR-001 跨平台客户端

- Makepad 单代码库支持 Linux、Windows、Android。
- UI 在窄屏和桌面宽屏均可完成连接、聊天、审批和任务观察。
- 平台不支持的能力必须通过 capability 隐藏或解释，不显示假按钮。

### FR-002 OUP 会话客户端

- 支持能力协商、认证、session open/hydrate、turn start/steer/interrupt。
- 支持 durable cursor、事件去重和 replay loss 恢复。
- 支持审批、用户问题、任务、diff、peer 和 artifacts。

### FR-003 多运行时路由

- 统一发现 `OctosNative`、`CursorBridge`、`RemoteAgent` 等运行时。
- 路由选择绑定会话，运行中不静默切换。
- UI 明确显示当前运行时、模型、位置、权限和费用来源。

### FR-004 Cursor SDK Bridge

- 桌面端启动、探活和关闭 Bridge。
- 支持 GetVersion/capability 协商。
- 支持 Agent create/resume/close、Run send/observe/cancel。
- 持久保存 run ID 与最后 offset，断线后继续观察。
- 支持 custom tool callback 的鉴权、schema 校验、超时和审批。

### FR-005 设备发现与租约

- 使用 USB serial、芯片 MAC、ADB serial、SSH host key 等形成稳定设备身份。
- 设备操作前必须获取带 TTL 和 fencing token 的租约。
- 一个物理设备的串口、JTAG、烧录接口共享排他策略。

### FR-006 硬件执行状态机

- 所有状态转换先持久化再执行外部动作。
- 非幂等步骤不得自动重试。
- 断线或超时可进入 `OutcomeUnknown`，必须先消歧。
- 从节点重启产生新 task，并引用前序证据。

### FR-007 精确审批

烧录审批至少显示并绑定：

- runner 与设备稳定 ID
- 芯片、板型和硬件 revision
- artifact SHA-256
- 分区/地址范围
- 操作类型和风险等级
- 审批 TTL

### FR-008 ESP32 工具链

- ESP-IDF 环境探测与版本报告
- build/flash/monitor
- esptool 芯片和 flash 信息
- USB-JTAG、OpenOCD、GDB/GDBStub
- ELF、map、bin、partition table、sdkconfig 证据归档

### FR-009 RK3588 工具链

- SSH/ADB 连接与身份确认
- Rust/C/C++ `aarch64` 构建与 sysroot 检查
- 文件部署、服务启动、journal/logcat
- gdbserver 调试
- board profile 插件化

### FR-010 证据与报告

- 每个闭环生成结构化 EvidenceBundle。
- 报告区分“命令成功”“设备启动”“需求验证通过”。
- 原始大日志外置保存，Octos 事件中只保留摘要和引用。

### FR-011 Ontology RAG

- 支持类型、关系、约束、来源、版本与适用范围。
- 结合图查询、关键词和向量召回。
- 支持增量 ingest、冲突记录和人工修正。
- 生成回答时返回 provenance。

### FR-012 RSI

- 自动收集失败、用户修正、回滚、评测和硬件验证事件。
- 聚合稳定 fingerprint，形成演进候选。
- 每个候选包含基线、修改、评测集、风险、收益和回滚方案。
- 发布前通过对应级别的 gate。
- 发布后支持 canary、指标观察和一键回滚。

### FR-013 可扩展插件

- 新板卡、新工具链和新运行时不要求修改 UI 核心。
- 插件声明 capability、工具 schema、风险、平台和生命周期。
- 未签名或不兼容插件默认不可执行。

## 8. 安全需求

- 凭据不得写入项目仓库、普通日志或 Agent 上下文。
- sidecar 默认只监听 loopback；远程 runner 使用 mTLS 和节点身份。
- Agent 工具参数视为不可信输入，必须 schema 校验和路径约束。
- 安全监督器为确定性代码，LLM 无权覆盖 veto。
- eFuse、OTP、Secure Boot、整盘擦除独立为最高风险操作。
- Android 包中不得内置 Cursor、Octos 或 runner 长期密钥。
- 所有产物、插件和 runner capability 声明可校验签名或摘要。

## 9. 非功能需求

- UI 首次可交互：桌面目标小于 2 秒（不含首次下载 runtime）。
- 普通 UI 操作 P95 小于 100 ms。
- 日志流有界，慢客户端不会导致 sidecar 无界缓存。
- 任务重连后不重复非幂等操作。
- 关键状态机与审批逻辑可通过 mock device 做确定性测试。
- Linux/Windows 产生同一 EvidenceBundle schema。
- 客户端对未知 OUP、Bridge 事件向前兼容并可见降级。

## 10. MVP

MVP 由以下能力构成：

1. Makepad Linux/Windows 客户端；Android 提供远程监控与审批。
2. OUP 连接、会话、聊天、审批、任务和恢复。
3. Cursor SDK Bridge 本地 Runtime。
4. ESP32/Linux 本地完整闭环。
5. RK3588 Linux 用户态 SSH 部署最小闭环。
6. 硬件设备租约、精确烧录审批和 EvidenceBundle。
7. 硬件 Ontology 最小 schema 与混合检索。
8. RSI 只做到候选生成、回放评测与人工批准技能/工作流更新。

## 11. MVP 验收标准

### AC-01 ESP32

给定一个可构建 ESP-IDF 示例与连接的 ESP32，用户发出“构建、烧录并验证启动”
后，系统能生成产物、请求精确审批、烧录、捕获串口并以日志断言结束；报告包含
设备 ID、固件哈希、源码 revision 和关键日志。

### AC-02 不确定结果

烧录过程中断开客户端或 sidecar，系统不得自动再次烧录；任务进入
`OutcomeUnknown`，重新连接后通过芯片/固件读取或用户决策消歧。

### AC-03 Cursor

未配置 Octos LLM provider 时，用户仍可选择 Cursor Runtime 创建 Agent、发送
任务、观察流式事件、取消并在断线后从 offset 恢复。

### AC-04 权限

普通 flash 审批不能授权 erase-flash、eFuse 或另一块设备；篡改 artifact hash
会使审批失效。

### AC-05 RK3588

系统能将一个用户态程序交叉构建并通过 SSH 部署到 RK3588，采集退出码和远端日志，
且报告记录 sysroot、target triple 与设备 host key。

### AC-06 Ontology

对“此板的哪个引脚可作为 I2C SDA”提问时，系统返回板型/revision 约束和来源；
若只有相近板型资料，明确拒绝把其作为当前板的事实。

### AC-07 RSI

重复的同类工具失败可聚合为一个候选；候选未经评测和人工批准不会改变生产技能；
批准后能发布新版本并回滚。

### AC-08 跨端

同一 Octos 会话可从 Linux/Windows 客户端打开；Android 可远程查看运行、处理
审批和读取证据，但不会声明本地拥有桌面工具链。

## 12. 成功指标

- ESP32 示例项目首次闭环成功率
- 从需求到板上可验证结果的中位时间
- 错误设备/错误产物烧录次数，目标为 0
- `OutcomeUnknown` 被正确消歧的比例
- 人工离开 UI 执行命令的次数
- Ontology 回答的有效来源覆盖率
- RSI 候选通过评测后的实际失败率下降
- 会话恢复后重复副作用次数，目标为 0

## 13. 开放决策

以下事项在实现前需通过 ADR 固化：

1. livstyle 使用上游 Octos release、git pin，还是维护短期集成 fork。
2. External Agent Runtime 是通过新增 OUP driver contract，还是先由 livstyle
   编排 Octos session 与 Cursor run 两条记录。
3. Ontology 存储选用嵌入式图存储、关系表，还是在 Octos RecallStore 上扩展。
4. 桌面端 Bridge 二进制的下载、校验和升级策略。
5. 首批支持的 ESP32 板型与 RK3588 开发板清单。
