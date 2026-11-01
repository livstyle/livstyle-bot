---
name: serial-flash-troubleshooting
description: Diagnoses a failed flash from the device runner output. Does not invent a flash command.
license: Apache-2.0
metadata:
  author: livstyle
  version: "1.0.0"
  domain: hardware-diy
  boards: esp32,esp32s3,esp32c3,esp32c5,esp32c6,esp32p4
  tools: device-runner
  risk: high
---

烧录只能走已经授权的设备执行器。本说明不包含可直接执行的 shell。失败时引用执行器返回的错误，不用自然语言代替烧录结果。
