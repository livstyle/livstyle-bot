# livstyle 实施计划

## 2026-10-25 对话闭环增量

范围：用户输入必须进当前任务自己的 Octos 会话或 Cursor agent，不能沿用上一个任务的连接。

- `crates/livstyle-app/src/turns.rs`：`session_align`、`cursor_agent_choice`。
- `crates/livstyle-app/src/worker.rs`：`ask_octos` 在 session 不一致时重开；`ensure_cursor_agent` 按任务 mapping 复用或 `resume_agent`。
- `crates/livstyle-app/src/main.rs`：空输入不发送。

验证：`cargo test -p livstyle-app --offline --bin livstyle-app`。没有 Key 或 Bridge 时 Cursor 端到端标为未验证。没有 `octos serve` 时 Octos 端到端标为未验证。

# livstyle 实施计划（原阶段）

依据：`docs/development/CURRENT_STATE.md`（2026-10-25）。本文件只排期，不授权改代码。确认后再做 Phase 0。

原则：不新写第二套 Agent 循环；不把 `EspRunner` 塞进按钮回调；不用假百分比或假在线点代替扫描结果；每个阶段可单独回滚。

## 最小改动架构

保持现有 crate 边界。

- **产品任务**继续由 `livstyle-core::Service` + `TaskStatus` 持有。不新增第二套状态枚举。UI 上的 Queued / Planning / Running / AwaitingApproval / Verifying / Succeeded / Failed / Cancelled / OutcomeUnknown 映射到已有 `TaskStatus`（已含 Replanning、Paused、Blocked）。
- **Octos 会话**继续由 `livstyle-octos::OctosClient` 持有。一个产品任务对应一个 `runtime_session_id`，用已有 `Service::bind_runtime` 写入，禁止再写死 `livstyle:local:desk` 或 `"desk"`。
- **Cursor Agent**继续由 `livstyle-cursor::{Bridge, CursorClient}` 持有。Bridge 进程生命周期仍是 `Bridge` 的 Drop。agent id 写入 `bind_runtime`，不要另建 agent 表，除非 SQLite 现有字段不够（先证明不够）。
- **事件游标**放在 worker，按任务 id 分桶。UI 只消费该任务的 Snapshot。不在 Makepad 回调里解析 OUP 或 sdk.v1。
- **硬件**只经 `livstyle-hardware::Supervisor` 调用 `EspRunner`。UI 按钮只提交意图和审批，不拼 esptool 参数。

尚未实现、本计划才补上的模块：任务级 transcript 持久化、hydrate 修复后的会话恢复、UI→Supervisor→EspRunner、EvidenceBundle、Android/Windows 设备层。

## Phase 0：基础缺陷

前置：无。

任务：

1. 修 `livstyle-octos/tests/live.rs` 的 `session/hydrate` `unknown_session`。先读失败时的 session id 与 open 返回值，确认是客户端把 id 传错，还是服务端 open 与 hydrate 的作用域不一致。不改协议，只改 livstyle 客户端或测试里错误的 id。
2. 烧录卡文案和 `primary` 行为保持「只记账、不烧录」，直到 Phase 4。按钮文字改成能看出尚未烧录，避免和原型图里的「烧录设备」步骤混淆。不接 `EspRunner`。
3. `apply_agent` 不再把外部 id 写成 `"desk"`。没有真实 id 时不要覆盖已绑定的 id。

文件：`crates/livstyle-octos/tests/live.rs`、`crates/livstyle-octos/src/client.rs`（仅当 id 传递有 bug）、`crates/livstyle-app/src/worker.rs`、`crates/livstyle-app/src/main.rs` 上烧录按钮文案。

测试：`cargo test -p livstyle-octos --test live` 在本机 `octos serve` 可用时通过；`cargo test -p livstyle-core` 仍通过。没有 serve 时该测试应明确 skip，而不是红。

验收：hydrate 使用 open 返回的 session id 成功，或测试记录服务端缺陷并 skip。产品任务上的 runtime id 在切换 Cursor 后仍是 agent id。

风险：若是 Octos 服务端 bug，livstyle 侧无法单独修绿。回滚：还原上述文件。

## Phase 1：Runtime 接入补齐

前置：Phase 0 的 hydrate 结论。

任务：

1. Octos：`ensure_session` 按 `selected` 任务取或创建 session id，写入 `bind_runtime`。启动时对未终态任务调用已有 `hydrate`。失败则标 `octos_status`，不静默新建第二个会话。
2. 断线：`drain`/发送失败时区分网络错误和业务错误；网络错误重连一次。`interrupt` 的 `Err` 写入 hint。
3. Cursor：启动路径增加 `me()`，把未认证和 Bridge token 错误分成两句状态。取消失败要可见。评估 `observe_run`：只在进程重启且 SQLite 里有未完成 run id 时调用。run id 若无处存放，先加在现有 task doc 或 operations 表，不新建平行任务系统。
4. 超时：把产品任务迁到 `Failed` 或 `OutcomeUnknown`（用 `Service::transition`），与对话里的那句话一致。

文件：`worker.rs`、`livstyle-octos/src/client.rs`、`livstyle-cursor/src/client.rs`、必要时 `livstyle-store` 的 task doc 字段（须带 schema 迁移说明）。

测试：core 状态迁移单测；octos client 用假 socket 或现有单测覆盖错误 id；cursor 单测已有 token 错误，补「me 失败不当成已登录」。不对真烧录设备做测试。

验收：重启应用后，未完成的 Octos 任务能 hydrate 或明确失败。Cursor 无 Key 与 Bridge 拒绝 token 的文案不同。

回滚：恢复 worker 的单会话路径；schema 迁移需向下兼容 v1。

## Phase 2：任务工作台

前置：Phase 1 的 session/agent 绑定。

任务：

1. `Cmd::Select` 切换 `selected` 时，transcript 和 hint 换成该任务的记录。运行中的 turn 继续在原任务上更新，不写进新选中的任务。
2. transcript 有界持久化。优先放在已有 task doc 或单独表，由 `Service` 读写。上限保持与现在类似的裁剪。
3. 列表展示任务标题和 `TaskStatus` 的中文，不再只显示 `workspace_id`。
4. 中央区增加只读的计划摘要：读 `Service` 已保存的 plan。没有 plan 时显示空态，不画六步假流程。

文件：`worker.rs`、`main.rs`（绑定已有 id，不删按钮）、`livstyle-core`、`livstyle-store`。

测试：内存 Service 上「任务 A 的事件不会出现在任务 B 的 snapshot」。UI 无自动化，用 core 测试锁住不变量。

验收：切换任务后输入框发送落到当前任务；旧任务的 Cursor/Octos 事件不再追加到新任务文本。

回滚：snapshot 仍可退回单一 transcript 字段。

## Phase 3：视觉

前置：Phase 2 的区域已经承载真实数据。没有前置时只允许改样式令牌，不许改调用链。

参考图：仓库内没有 `design/reference/livstyle-dashboard.png`。实施前把确认过的稿放进该路径。在此之前不以「像素对齐某张外置图」为验收。

方向（与现有 Makepad 脚本兼容）：

- 令牌集中在 `main.rs` 脚本顶部：底 `#07122b` 一带海军蓝，面板比底亮一档，边只用在可点击或可滚动区域。电光蓝用于主按钮和当前导航，青色用于设备已连接，绿色只用于真实日志流。
- 顶栏导航改成真正切换可见区域的按钮，而不是四个 `Label`。知识库、插件在没有后端时按钮禁用并写「未接入」，不导航到空白假页。
- 去掉重复圆角套圆角。横幅可保留一句产品句，右侧继续用现有 `BoardStage`，不贴静态宣传图，不把业务句子画进背景。
- 右栏标题与数据一致：扫描结果叫「设备」，构建日志出现之后才叫「实时日志」。
- 输入区保持无文字的拖动手柄；向上拖增加高度（当前 `composer_h` 用减号，已与「向下拖变矮」一致）。

文件：主要是 `crates/livstyle-app/src/main.rs`，3D 清屏色在 `board_view.rs`。不改 worker 协议，除非 Phase 2 的空态需要一个新的 Snapshot 字段。

测试：`cargo check -p livstyle-app`。目视清单：导航可点、禁用态可见、无假 68%、无假在线点。

回滚：还原 `main.rs` 脚本块。

## Phase 4：硬件闭环

前置：Phase 1 的任务状态可靠。设备操作不依赖视觉阶段。

任务：

1. UI 扫描结果变成结构化设备列表（端口、vid/pid、bridge），来自已有 `enumerate`。识别结果写入这条记录，而不是只改一句 `board_status`。
2. 烧录按钮提交 `OperationIntent` 给进程内的 `Supervisor`（已有 `prepare` / `authorize` / `begin_dispatch`）。参数来自本次 `FlasherArgs` 和芯片识别，禁止再传 `7f3e-draft`。
3. `EspRunner::dispatch` 在 worker 线程执行。成功、失败、OutcomeUnknown 映射到 `TaskStatus`。取消走已有 `request_cancel`。
4. 烧录后调用已有 `read_log`。断言先做最小集：日志非空、无 panic 字样。通过才 `Succeeded`。
5. EvidenceBundle 按 ADR-006 的目录落盘。ADR 仍是 Proposed，落地前先把 ADR 改为 Accepted 或在本阶段记一条例外。

文件：`worker.rs`、`livstyle-esp`、`livstyle-hardware`、新的 evidence 写入模块（放在 `livstyle-esp` 或 `livstyle-core`，二选一，不两处写）。

测试：沿用 `hardware/tests/safety.rs`。新增「UI 意图的摘要必须等于 FlasherArgs 摘要」的纯函数测试。真板烧录不作为 CI 验收。

验收：未确认不能 dispatch。摘要不符被 Runner 拒绝。未接板时 UI 显示「没有串口」，按钮禁用。

回滚：烧录按钮退回 Phase 0 的只记账行为。

## Phase 5：平台

前置：Linux 闭环在 Phase 4 有一次人工记录（命令、板型、结果）。没有这条记录就不开始跨平台验收。

任务：

1. Windows：编译 `livstyle-app` 与 `livstyle-cli`；串口枚举单独模块，不改 Linux sysfs 路径。Bridge exe 路径已有 `cfg(windows)`。
2. Android：先定 Makepad 目标能否链接。不能链接就在文档标明阻塞，不添加灰色桌面按钮。能链接则隐藏烧录、IDF、Bridge 启动。
3. RK3588：当前零代码。单独立项，不塞进 ESP32 Runner。

验收：每个 OS 分开写「已编译 / 已运行 / 未测」。禁止用 Linux 结果代替。

回滚：平台模块用 cfg 隔离，默认可不编译。

## 建议的 ADR（待确认）

1. **会话键。** 产品任务 id 与 Octos `session_id`、Cursor `agent_id` 的对应是否永远 1:1。备选：一个任务多次 run，session 不变。推荐 1 任务 1 Octos session、1 Cursor agent，run 可多个。
2. **hydrate 失败时的策略。** 重开 session 会丢 Octos 侧上下文。推荐失败保持 `Blocked`，由用户选择重开。
3. **EvidenceBundle。** 是否在本轮把 ADR-006 从 Proposed 改为 Accepted。不接受就不做 Phase 4 的目录格式。
4. **视觉稿。** 是否把确认过的仪表盘图提交为 `design/reference/livstyle-dashboard.png`。没有文件则 Phase 3 只执行令牌和信息架构，不对图验收。
5. **默认 Octos token。** 代码默认 `123456`。是否改为缺失 token 时拒绝连接。推荐拒绝，避免连上错误的本机服务。
