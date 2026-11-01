---
name: embedded-build-debug
description: Reads an existing ESP-IDF build log and points at the first compiler or toolchain error.
license: Apache-2.0
metadata:
  author: livstyle
  version: "1.0.0"
  domain: hardware-diy
  boards: esp32,esp32s3,esp32c3,esp32c5,esp32c6,esp32p4
  tools: esp-idf
  risk: low
---

根据已有编译日志指出第一条错误。没有日志时说明缺日志，不要假装编译已经通过。
