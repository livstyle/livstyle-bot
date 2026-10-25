# ADR-010：干预语义、计划依赖图与硬件中断策略

**状态：** Accepted · 2026-10-18

## 决策

1. **intent 集合（8 个）：** append_context、amend_requirement、reprioritize、pause、cancel_step、cancel_task、side_question、new_task；**scope（4 个）：** current_step、remaining_plan、whole_task、unrelated；
2. **先持久化后判断：** 输入落库（`received`）后才进入 Judge；所有状态（判断中 / 已注入 / 已重编排 / 已排队 / 需确认 / 已退回）都写回记录；
3. **自动执行门槛：** `min(intent 置信, scope 置信) ≥ 0.8` 且 `touches_hardware_params < 0.2`；否则显示确认卡片；
4. **计划依赖图：** 节点声明输入键与摘要；需求变更计算新摘要，变化节点及传递下游作废；未变化的已完成节点保留；
5. **中断性：** `free`（立即中断）、`safe_point`（原子步骤后停止）、`non_interruptible`（写入完成后停止）；
6. **Runtime 映射**按 PRD §8.4；不支持的组合退化为排队，绝不丢弃。

## 验证

`livstyle-domain` 计划图测试、`livstyle-steering` resolve / queue 测试、`livstyle-hardware` 中断相关安全测试；`evals/interventions.jsonl`（112 条）。
