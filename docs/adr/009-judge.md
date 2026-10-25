# ADR-009：Judge（Jev 方法）的后端、模型、阈值与退化

**状态：** Accepted（默认模型待小模型评测后定稿）· 2026-10-18

## 背景

PRD §8.3 要求运行中干预在低延迟下分类，并能可靠地区分“可自动执行”和“需确认”。jev-rs 的方法（只读下一个 token 的 logprobs，在选项字母上做受限 softmax）不需要解码长文本，概率可直接作为置信度。

## 决策

1. **原生实现**（`livstyle-judge`），不依赖 jev-rs 二进制：共享状态前缀 + 类型化问题后缀，`Answer:` 后取 top-k logprobs，合并 `" B"` / `"B"` 等变体，缺席选项给底值；
2. **后端：** 默认本地 Ollama `/api/generate` raw 模式（自行套 ChatML 并预填空 `<think>` 块）；备选 OpenAI 兼容 `chat/completions` + `logprobs`（llama-server / vLLM）。Cursor 无 logprobs，不能作为后端；
3. **问题：** intent 必问；`touches_hardware_params` 必问；scope 只在 append_context / amend_requirement 时问，其余由 intent 推出；`invalidates_artifact` 默认不问；
4. **阈值：** 自动执行需 `min(intent, scope) ≥ 0.8` 且 `touches_hardware_params < 0.2`；
5. **校准：** 按（问题类型, 选项数）分桶温度；当前模型 ECE 0.031，暂用 T = 1；扩充样本后用 `judge eval` 重新拟合；
6. **退化：** Judge 不可达 / 超时 / 无 logprobs → `RuleJudge` + System Two，UI 标注慢路径；
7. **交互：** 输入先落库并显示“已接收”与规则分类，Jev 结果异步到达；Judge 从不阻塞输入，也从不进入审批依据；
8. 每次判断记录模型、校准名、各选项概率与 `label_mass`。

## 实测（Spike 4，112 条样本）

| 配置 | intent | ECE | ≥0.8 覆盖 / 准确 | scope | 硬件标记 / 漏报 | p50 / p95 |
|---|---|---|---|---|---|---|
| RuleJudge | 49.1% | 0.211 | 14.3% / 93.8% | 46.4% | 83.9% / 0 | < 1 ms |
| Qwen3.6-35B-A3B Q6_K，4 问 | 92.0% | 0.031 | 87.5% / 98.0% | 51.8% | 95.5% / 0 | 8.9 s / 9.2 s |
| Qwen3.6-35B-A3B Q6_K，默认 | 92.0% | 0.031 | 87.5% / 98.0% | 80.4% | 95.5% / 0 | 4.5 s / 6.9 s |

## 后果与待办

- 准确率与校准满足设计；**延迟高于 PRD 期望**（上游 4B 模型约 90 ms / 题）。MVP-1 前用同一样本评测 3–4B 模型（例如 `ollama pull qwen3:4b`），在“准确率下降 ≤ 5 个百分点”的前提下选更快的模型作为默认；
- 本机另一个模型 `Qwopus3.6-35B-A3B-Coder-MTP` 在 Ollama 中加载失败，不作为候选；
- 样本集由项目方编写，存在标注偏差；MVP-0 起把真实干预（脱敏后）加入评测集。

## 复现

```sh
cargo run -p livstyle-cli -- judge eval --backend rule
cargo run -p livstyle-cli -- judge eval --backend ollama --out evals/results/<name>.json
cargo run -p livstyle-cli -- judge eval --backend ollama --model qwen3:4b
```
