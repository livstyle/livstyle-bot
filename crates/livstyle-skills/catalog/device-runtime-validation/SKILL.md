---
name: device-runtime-validation
description: Checks device logs and sensor readings after a run. A chat reply is not success evidence.
license: Apache-2.0
metadata:
  author: livstyle
  version: "1.0.0"
  domain: hardware-diy
  boards: esp32,esp32s3,esp32c3,esp32c5,esp32c6,esp32p4
  tools: serial-log
  risk: medium
---

用串口日志或传感器读数判断设备是否在跑。模型说“应该成功了”不能记成验证通过。
