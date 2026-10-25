# ADR-001：Octos 上游版本与 pin 策略

**状态：** Accepted · 2026-10-18

## 背景

PRD §5.3 的能力核对基于本地 `octos` 仓库 `2.0.3-rc.13`。OUP 仍在演进（`turn/steer_dropped` 是较新的事件），无 pin 会导致客户端与服务端语义漂移。

## 决策

1. livstyle 不嵌入 Octos，作为 OUP 客户端连接独立运行的 `octos serve`；
2. 支持的 Octos 版本锁定为 `2.0.3-rc.13`；客户端连接时读取服务端版本，不在支持范围内时只读连接并提示；
3. 不维护长期 fork。需要的能力（例如无 LLM Provider 的会话）先以上游 issue / PR 推进；在合入前，Cursor-only 模式由 livstyle 直接编排 Cursor，不经过 Octos；
4. 用到的 OUP 方法与事件逐项列入 Spike 2 实测表，升级 Octos 时重跑。

## 后果

- 升级 Octos 是显式动作，需要重跑 Spike 2 检查表；
- Cursor-only 用户不需要配置任何 Octos LLM Provider。

## 验证

Spike 2：在 pin 的版本上实测 `turn/steer`（含 `expected_turn_id` 冲突）、`turn/steer_dropped`、`turn/interrupt`、`task/cancel`、`task/restart_from_node`、服务重启后的会话恢复。
