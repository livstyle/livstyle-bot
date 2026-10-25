# ADR-008：Ontology 存储、来源与版本

**状态：** Proposed（MVP-3）· 2026-10-18

## 提议

- 类型化实体：Chip、Board、Peripheral、Pin、Errata、IdfApi、ErrorPattern；关系：`has_pin`、`requires_idf`、`affected_by`、`fixed_in`；
- 每条事实带 `source`（文档 URL / 版本 / 页码或 Profile 文件与提交）与 `applies_to`（芯片 / revision / IDF 范围）；
- 存储：与 state.db 分离的 SQLite（FTS5 + 向量列）；Profile TOML 是 Chip / Board 的唯一真相，Ontology 只索引不复制；
- 回答规则：无来源 → 返回“未知”（AC-08）。

## 待定

向量模型与 Octos Recall 的复用方式。
