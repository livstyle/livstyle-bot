# ADR-007：Chip / Board Profile、验收板卡与 ESP-IDF 版本

**状态：** Accepted · 2026-10-18

## 决策

1. **数据驱动：** `profiles/chips/*.toml`、`profiles/boards/*.toml`，`schema_version = 1`，`deny_unknown_fields`；新增芯片只加 TOML（有测试保证）；
2. **有效级别** = min(板卡级别, 芯片级别)；
3. **验收板卡（MVP-1 必须达到 V3）：**
   - ESP32-S3-DevKitC-1 N8R8（Xtensa）
   - **ESP32-C5-DevKitC-1 N8R4（RISC-V，双频 Wi-Fi 6）**
4. **回归板卡（已在手）：** ESP32-C3-DevKitM-1、ESP32-C6-DevKitC-1；
5. **Experimental：** ESP32-P4（无射频，Wi-Fi 由伴随 C6 提供）、经典 ESP32；
6. **ESP-IDF：** 验收锁定 v6.0.2；各芯片最低版本写在 Chip Profile（C3 4.3、S3 4.4、C6 5.1、P4 5.3、C5 5.5）；
7. **来源：** 所有 Profile 当前为 `provenance.status = "draft"`（首批数值参考 EspSmith 与乐鑫文档）；在实板上通过 `livstyle chip-info` 与 V3 后改为 `verified` 并填写来源。

## 理由

C5 与 S3 覆盖两种架构；C5 要求最新 IDF 且支持 5 GHz，能尽早暴露版本准入与射频能力约束的问题。C3/C6 已有实物，作为低成本回归。

## 后果

- C5 板卡的引脚、USB 桥接芯片需在实板上核对（Profile 中已注明）；
- IDF 示例默认 2 MB flash，规划器必须按 Board Profile 生成 flash 大小等配置（见 ADR-013）。

## 验证

`livstyle-profiles/tests/bundled.rs::acceptance_boards_are_s3_and_c5` 等 9 项。
