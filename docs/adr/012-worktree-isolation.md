# ADR-012：每个可写 Run 一个 git worktree

**状态：** Accepted · 2026-10-18

## 背景

Cursor Agent 与 Octos 都可能写文件；被取消的 Run 可能留下半成品；PRD §9.3 要求同一工作目录不得被两个 Runtime 冲突写入。

## 决策

- 每个可写 Run：`git worktree add -b livstyle/<run_id> <root>/<run_id> <base>`，`root` 位于用户仓库之外；
- run_id 限制为 `[A-Za-z0-9_-]{1,64}`；
- 单写者锁 `.livstyle-lock`（`create_new` 原子创建，记录持有者与 pid），持锁时不可删除；
- Agent 的 cwd 一律指向 worktree；合并回用户分支是显式审阅动作；
- 删除 worktree 默认保留分支；
- 非 git 目录退化为 scratch 目录 + 锁，并提示无法隔离。

## 后果

- 构建目录随 worktree 独立，首次构建较慢（可后续加 ccache）；
- 硬件证据中的源码 revision 即 worktree 分支的提交。

## 验证

`livstyle-workspace` 4 项测试：两个 Run 互不影响且不影响用户 checkout、单写者锁、删除保留分支、非法 id 与非仓库拒绝。
