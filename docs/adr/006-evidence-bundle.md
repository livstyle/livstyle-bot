# ADR-006：EvidenceBundle schema 与产物存储

**状态：** Proposed（MVP-1 定稿）· 2026-10-18

## 提议

每次验证生成一个只追加的目录 `$XDG_DATA_HOME/livstyle/evidence/<task>/<run>/`：

```
manifest.json      # 源码 revision、worktree 分支、IDF 版本、Profile 版本、artifact_sha256、设备身份、芯片与 revision、操作 id、审批摘要、V 等级
artifacts/         # flasher_args.json 与镜像（按 sha256 去重存储）
logs/boot.log      # ReadLog 原文
assertions.json    # 每条断言的模式、匹配行号、结果
audit.jsonl        # 本次相关的审计条目（含哈希，可与 state.db 交叉校验）
```

- manifest 自身哈希写回审计表；
- V2 需要 boot log 中出现 Profile 的 `boot.log_patterns`；V3 需要需求断言全部通过；
- 报告只引用 bundle 中的文件，不引用临时目录。

## 待定

镜像保留策略（按任务保留最近 N 个）与远程 Runner 的 bundle 回传方式。
