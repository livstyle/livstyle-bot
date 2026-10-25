# ADR-014：ESP32 系列的内循环采用 Toit / Jaguar 分层

状态：Accepted

## 背景

ESP-IDF 的默认循环是整镜像编译再 USB 烧录，改一行应用代码也要重烧。Toit 把设备分成底座虚拟机和若干应用容器，Jaguar 在第一次 USB 刷入底座之后，用局域网在约数秒内替换单个容器（https://github.com/toitlang/jaguar ，设计说明见 Toit 公开材料）。

livstyle 要覆盖 ESP32-C3/C5/C6/S3/P4，但不能把「每次修改都 USB 烧录」写成唯一路径。

## 决策

1. 应用逻辑的内循环是：电脑上先跑通 → 局域网替换一个容器。底座已经在板上时，计划里不出现 USB。
2. USB 烧录只用于两件事：第一次刷底座，以及底座运行时自身发生变化。
3. 硬实时、尚无驱动、芯片 bring-up、以及会改变 flash 布局或 `sdkconfig` 的修改，继续走完整 ESP-IDF 镜像。容器运行时不承担微秒级时序。
4. 一个应用容器失败不要求重刷底座，也不自动整镜像重烧。
5. 热替换不依赖云。芯片差异仍由 Chip / Board Profile 决定，不在这条策略里按型号写死。

选择由 `livstyle_domain::plan_iteration` 给出。`livstyle iterate --change app --base ready` 可查看当前选择。

## 后果

- 规划器在「底座已就绪」时不得把 USB 烧录排进普通应用修改。
- 底座未就绪时，USB 步骤只有一次，并且排在电脑上跑通之后。
- IDF 整镜像路径保留，给硬实时和板级配置。

## 验证

`cargo test -p livstyle-domain plan_iteration` 覆盖：已就绪的板子改应用不走 USB；未就绪先刷底座再热替换；实时与 sdkconfig 仍是整镜像。
