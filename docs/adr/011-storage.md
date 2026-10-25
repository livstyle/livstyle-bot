# ADR-011：本地持久化采用 SQLite

**状态：** Accepted · 2026-10-18

## 背景

需要跨重启保存任务、计划修订、干预、硬件操作、Runtime 映射、事件与审计；要求事务性（状态变化与事件同时写入）、单机、三平台一致、无需服务进程。

## 决策

- rusqlite 0.32 + `bundled`（静态链接 SQLite），WAL、`synchronous=FULL`、外键开启、`busy_timeout` 5 s；
- 迁移用 `PRAGMA user_version`，遇到更新的 schema 拒绝打开；
- 领域对象存 JSON 文档 + 查询列；任务用 `version` 列做 CAS；计划修订只追加；
- 每个业务写入与 `events` 追加在同一事务中；
- `audit` 为哈希链（`prev_hash`、`hash` = SHA-256(长度前缀字段)），触发器禁止 UPDATE / DELETE；
- 启动时 `recover_inflight_operations()` 把 `Dispatched` 转 `OutcomeUnknown`；`OutcomeUnknown` 不能回到 `Dispatched`。

## 备选

- 纯文件 JSONL：无事务，恢复复杂；
- 嵌入式 KV（sled/redb）：缺 SQL 查询与成熟的外部检查工具。

## 验证

`livstyle-store` 8 项测试（CAS、计划修订、干预恢复、事件续读、操作迁移白名单、崩溃恢复、审计不可改、拒绝未来 schema）。
