# ADR-002：产品任务与 Octos 会话的所有权边界

**状态：** Accepted · 2026-10-18

## 背景

一个产品任务可能跨越多个 Octos turn、多个 Cursor Run 和多个硬件操作。如果状态散落在各个 Runtime 中，重启后无法确定“现在该做什么”。

## 决策

1. **ProductTask、Plan、Intervention、OperationRecord、Approval、Audit 只由 livstyle Product Service 写入**（SQLite，ADR-011）；
2. Octos 拥有会话 / turn / 工具调用；Cursor 拥有 Agent / Run。livstyle 只保存映射与游标（`runtime_mappings`：`external_id`、`resume_offset`）；
3. 恢复由 livstyle 发起：读 Store → 重新连接 Runtime → 用游标续看事件 → 依据计划图决定继续、重试还是询问用户；
4. Runtime 的事件只是输入；任务状态迁移必须经过 `ProductTask::transition(expected_version, next)` 和 Store 的 CAS。

## 后果

- 同一任务的两个写者（例如 UI 与 CLI 同时操作）由 CAS 冲突拒绝其中之一；
- Runtime 侧的状态丢失（例如 Bridge 状态目录被清理）只影响续看，不影响任务真相。

## 验证

`livstyle-store` 测试：`optimistic_concurrency`、`restart_marks_dispatched_operations_outcome_unknown`、`interventions_survive_until_settled`。
