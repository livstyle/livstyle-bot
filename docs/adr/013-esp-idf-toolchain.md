# ADR-013：ESP-IDF 工具链发现与准入

**状态：** Accepted · 2026-10-18

## 决策

1. **发现：** 先读 EIM 的 `~/.espressif/tools/eim_idf.json`（采用 `idfSelectedId`）；若环境中导出了 `IDF_PATH`，以它为准；
2. **版本：** 从 `tools/cmake/version.cmake` 解析；
3. **准入：** 规划前检查所选 IDF ≥ Chip Profile 的 `idf.min_version`，不满足时直接报错并提示升级；
4. **执行：** `idf.py` 通过 EIM activation script 运行；esptool 用该 IDF venv 的 `python -m esptool`；按 IDF 主版本选择 esptool v5（kebab-case）或 v4（snake_case）命令；
5. **配置默认值：** 规划器根据 Board Profile 生成 `sdkconfig.defaults`（flash 大小、PSRAM、控制台），作为 Build 节点输入摘要的一部分；
6. **验收锁定：** ESP-IDF v6.0.2，记入 EvidenceBundle；
7. espflash、probe-rs 只作可选工具。

## 本机实测（2026-10-18）

- EIM：IDF v6.0.2，Python venv 可用，esptool v5.3.1，openocd-esp32 v0.12.0-esp32-20260424；
- `hello_world` 以 esp32c5 为目标编译成功（19 s）；`flasher_args.json` 解析出 3 个镜像；
- 发现：示例默认 `--flash-size 2MB`，与 C5 DevKitC-1 的 8 MB 不符 → 决策第 5 条。

## 验证

`livstyle toolchain`；`livstyle-esp` toolchain / flasher 测试；`LIVSTYLE_TEST_IDF_BUILD=… cargo test -p livstyle-esp real_idf`。
