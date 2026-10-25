# ADR-005：Hardware Supervisor、Runner 与设备租约

**状态：** Accepted · 2026-10-18

## 决策

1. **租约：** 每台设备同一时间最多一个有效租约；每次发放递增 `FencingToken`；租约有 TTL，需续约；
2. **精确审批：** 审批绑定 `OperationParams::digest()`（规范化 JSON 的 SHA-256），覆盖操作类型、设备、runner、目标芯片与 revision 范围、Board Profile、artifact 哈希、分区区域与额外参数；任何字段变化都使旧审批失效；审批一次性使用；
3. **风险等级：** R0 只读直接允许；R1/R2 需要审批；R3（整片 / 区域擦除）、R4（eFuse）在 MVP 中拒绝；
4. **Runner 独立校验**（不信任 Supervisor）：runner 身份、fencing token 不低于已见最大值、端口存在、artifact 文件哈希与审批一致、ChipCheck（芯片与 revision）；
5. **结果三态：** `Completed` / `Failed`（设备未改变）/ `OutcomeUnknown`（可能改变）。`OutcomeUnknown` 从不自动重试，只能由回读校验或用户确认消歧；
6. **中断：** 写入类操作不可中断；取消请求延后到写入结束；强制中断需二次确认并记为 `OutcomeUnknown`；
7. **审计：** 所有准备、审批、派发、结果写入哈希链审计表（ADR-011）。

## 验证

- `livstyle-hardware/tests/safety.rs`（11 项，Mock Runner 故障注入）；
- `livstyle-esp` runner 测试：芯片不符在写入前拒绝、产物被替换拒绝、旧 token 拒绝、写入中途断开 → `OutcomeUnknown`、连接失败 → `Failed`、擦除类 → `Unsupported`。
