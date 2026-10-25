# livstyle 实施路线图

状态：v0.3.1（对齐 [PRD v0.3.1](PRD.md) §17）  
原则：先做出 Linux + ESP32 系列的可验证闭环，再扩展平台、板卡和自治程度。每个阶段都能独立运行和验收。

## 阶段 0：技术可行性验证

| 项 | 状态 | 证据 / 下一步 |
|---|---|---|
| Spike 1：Cursor SDK Bridge + Rust | **进行中** | 无 Key 部分已通过（启动、Ping、GetVersion、token 错误区分、Shutdown；确认无运行中注入）。下一步：用户执行 `livstyle auth cursor` 后运行 `livstyle cursor smoke` |
| Spike 2：Octos OUP + 持久化 | 完成（能力面） | 对 2.0.3-rc.13 实测：`turn/steer`、`turn/interrupt`、`task/cancel`、`task/restart_from_node`、`event.turn_steer_dropped.v1` 均在握手里。`session/open` 在 profile 没有 LLM 时拒绝（`missing: llm`），所以没有 LLM 的会话不会开 turn。`livstyle-core` 负责任务、干预和崩溃恢复 |
| Spike 3：Mock Hardware Runner | **完成** | `livstyle-hardware/tests/safety.rs` 11 项 + `livstyle-esp` runner 7 项 |
| Spike 4：Jev 干预分类 | **完成（基线）** | 112 条样本；规则基线与 Ollama 35B MoE 结果见 PRD §8.3；下一步评估 3–4B 小模型 |
| ADR-001 ~ 013 | 001/002/003/005/007/009–013 Accepted；004/006/008 Proposed | [docs/adr](adr/) |

退出条件：未配置 Octos LLM Provider 时 Cursor 能跑通任务并在断线后恢复观察；Mock Runner 故障注入不产生重复副作用；Jev 基线数据就绪。

## 阶段 1：MVP-0 最小工作台

已具备的无头基础：任务状态机、计划依赖图、干预队列与解析、SQLite 存储与恢复、worktree 隔离、凭据、Cursor 客户端。

待做：

- `livstyle-octos`：OUP 客户端（Spike 2，已接通本机 `octos serve`）
- `livstyle-core`：任务、干预落库、崩溃恢复（已完成）
- `livstyle-core`：把各 crate 组合为 Product Service（任务推进、干预管线、恢复）
- `livstyle-app`：Linux Makepad 客户端（线框见技术设计 §13）
- Cursor Runtime 选择与状态展示；任务详情、Diff 与日志
- Live Steering 基础版：运行中输入、补充上下文、取消步骤/任务、暂停、`turn/steer_dropped` 重新排队

退出条件：客户端重启后任务可恢复；Runtime 错误不会静默切换；运行中输入不丢失。对应 AC-01、AC-02、AC-11。

## 阶段 2：MVP-1 ESP32 系列闭环

已具备：Chip / Board Profile（含 C5 验收板卡）、IDF 工具链探测与按芯片准入、sysfs 设备发现、`EspRunner`（fencing、产物哈希、ChipCheck、写入结果三态）、被动串口日志、`chip-info`。

待做：

- 接板实测：`livstyle devices` → `livstyle chip-info` → 核对并把 S3、C5（及 C3、C6）Profile 改为 `verified`
- 规划器按 Board Profile 生成 `sdkconfig.defaults`（flash 大小、PSRAM、控制台）
- Supervisor 状态机与 Store 合并；回读校验消歧 `OutcomeUnknown`
- 启动日志断言与 EvidenceBundle（ADR-006）
- 借鉴 EspSmith 的失败恢复与 CoreDump 解析
- 计划差异展示、硬件节点中断边界的 UI
- Jev 判断层接入干预管线（ADR-009）

退出条件：**S3 与 C5 实机达到 V3**，C3、C6 回归通过；新增 Chip Profile 无需改 Runner 代码；故障注入不造成重复烧录；改需求只重做受影响节点。对应 AC-03 至 AC-07、AC-10、AC-12 至 AC-14。

## 阶段 3：MVP-2 稳定性、Windows 与 RK3588

- Windows 客户端与原生 Runner（设备发现 SetupAPI、Win32 串口、Credential Manager）
- Experimental 芯片（P4、经典 ESP32）实机验证后升级为 Verified
- Bridge 升级流程
- Runner 故障恢复、并发任务与设备互斥
- 端到端安全测试
- 可视化 Pin Planner
- RK3588 Linux 用户态交叉构建与 SSH 部署闭环

## 阶段 4：MVP-3 知识增强与远程客户端

- Ontology 最小 schema 与来源可追溯的混合检索（AC-08）
- RSI 候选与离线评测、人工批准发布，含 Judge 校准迭代（AC-09）
- Android 远程客户端：会话、事件流、干预输入、审批、证据浏览

Android 按平台验证结果独立发布，不阻塞 Linux + ESP32 闭环。

## 后续

工作台近期明确留下的两项：

- **板子结构 3D 观察**（PRD §11.5）：用板子照片或摄像头识别结构，显示在「板子调试」折叠框里，先看出 pin 和外设的插拔，再在调试中做姿态检测并据此建议改代码。当前该框只有串口和芯片识别的文本。
- **Octos 托管外部 code agent：** 同一条对话可以在 Cursor 和 Octos 之间换人，Cursor 仍由 livstyle 直接调用。让 Octos 再去调用 Cursor 或其他 code agent，要等下面的 External Agent Driver（UPCR），不把 Cursor 填进 Octos 的模型位置。

更后面的范围：

- R3/R4 高风险操作的专用审批流程（擦除、eFuse、Secure Boot）
- 交互式 JTAG 调试器
- S2、C2、C61、H2 等更多芯片 Profile
- RK3588 BSP、Loader/MaskROM 镜像烧录
- 远程 Runner 集群与硬件在环机架
- 正式的 External Agent Driver 上游 Octos 协议（UPCR）
- 通用超级智能体能力（办公、研究、Computer Use）
