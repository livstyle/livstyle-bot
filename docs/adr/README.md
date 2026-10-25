# 架构决策记录（ADR）

格式：背景 → 决策 → 后果 → 验证方式。状态：Accepted（已决定并有代码 / 测试）、Proposed（待 Spike 结果）、Superseded。

| ADR | 标题 | 状态 |
|---|---|---|
| [001](001-octos-pin.md) | Octos 上游版本与 pin 策略 | Accepted |
| [002](002-task-ownership.md) | 产品任务与 Octos 会话的所有权边界 | Accepted |
| [003](003-cursor-bridge.md) | Cursor SDK Bridge 版本锁定、认证与生命周期 | Accepted |
| [004](004-runtime-events.md) | Agent Runtime Adapter 规范化事件模型 | Proposed |
| [005](005-hardware-supervisor.md) | Hardware Supervisor、Runner 与设备租约 | Accepted |
| [006](006-evidence-bundle.md) | EvidenceBundle schema 与产物存储 | Proposed |
| [007](007-chip-board-profiles.md) | Chip / Board Profile、验收板卡与 IDF 版本 | Accepted |
| [008](008-ontology.md) | Ontology 存储、来源与版本 | Proposed |
| [009](009-judge.md) | Judge：后端、模型、阈值与退化 | Accepted（模型待定） |
| [010](010-intervention-semantics.md) | 干预语义、计划依赖图与硬件中断策略 | Accepted |
| [011](011-storage.md) | 本地持久化：SQLite | Accepted |
| [012](012-worktree-isolation.md) | 每个 Run 一个 git worktree | Accepted |
| [013](013-esp-idf-toolchain.md) | ESP-IDF 工具链发现与准入 | Accepted |
| [014](014-esp-iteration.md) | ESP32 内循环：USB 只刷底座，应用走局域网热替换 | Accepted |
