# ADR-004：Agent Runtime Adapter 规范化事件模型

**状态：** Proposed（待 Spike 2 完成后定稿）· 2026-10-18

## 背景

Octos OUP 与 Cursor `sdk.v1` 的事件粒度、偏移量语义、注入能力都不同，UI 和编排层不应关心差异。

## 提议

```rust
enum RuntimeEvent {
    TextDelta { text: String },
    ToolCall { id: String, name: String, args: serde_json::Value },
    ToolResult { id: String, ok: bool, summary: String },
    StatusChanged { status: RunStatus, message: Option<String> },
    SteerAccepted { input_ids: Vec<InterventionId> },
    SteerDropped { inputs: Vec<String> },      // Octos turn/steer_dropped
    Finished { summary: RunSummary },
    Failed { error: RuntimeError },
}

struct RuntimeCaps { steer: bool, interrupt: bool, resume: bool }
```

- 每个事件带 Runtime 原生 offset（不透明字符串）与 livstyle 自增 seq；
- 未知事件保留原始 JSON，不丢弃；
- Cursor：`sdkMessage` 按 `type` 映射，`interactionUpdate` → `TextDelta`，`result` → `Finished`；
- Octos：`turn/*` 事件映射，`turn_steer_dropped` → `SteerDropped`。

## 待定

Octos 工具调用中硬件工具的拦截点（`peer/tools/register` 注册的 livstyle 工具必须走 Supervisor）。
