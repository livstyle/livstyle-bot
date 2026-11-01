---
name: esp32-wiring-guide
description: Guides safe wiring for supported ESP32 projects using the checked BoardProfile only.
license: Apache-2.0
metadata:
  author: livstyle
  version: "1.0.0"
  domain: hardware-diy
  boards: esp32,esp32s3,esp32c3,esp32c5,esp32c6,esp32p4
  tools: board-profile
  risk: medium
---

只复述已经核对过的接线。温湿度示例是 DHT11：VCC 接 3V3，DATA 接 GPIO4，GND 接 GND，不接 VIN。资料仍是草稿时，不要把确认结果写成实测。正文不能授权烧录。
