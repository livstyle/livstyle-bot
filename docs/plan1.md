# livstyle-bot 集成 Cursor Agent 与 Octos 的实施任务

你现在是这个 Rust 项目的主程工程师。请直接检查当前工作区的真实源代码，并在现有项目上实施集成，而不是只输出架构建议或伪代码。

## 一、项目背景与目标

主要项目仓库：

- livstyle-bot：https://github.com/livstyle/livstyle-bot
- 目标分支：`master`
- Octos：https://github.com/livstyle/octos
- Cursor TypeScript SDK：https://cursor.com/docs/sdk/typescript
- Cursor 官方 SDK Bridge：https://github.com/cursor/sdk-bridge
- Bridge 协议文档：https://github.com/cursor/sdk-bridge/blob/main/docs/protocol.md
- Bridge 服务定义：https://github.com/cursor/sdk-bridge/blob/main/docs/services.md
- Bridge 流式事件文档：https://github.com/cursor/sdk-bridge/blob/main/docs/streaming.md
- Bridge 错误处理文档：https://github.com/cursor/sdk-bridge/blob/main/docs/errors.md

livstyle-bot 是使用 Rust 和 Makepad 构建的跨平台 Agent 工作站，面向 Windows、Linux 和 Android。项目未来要面向 IoT 硬件开发，重点包含 ESP32、RK3588 等开发板的代码开发、构建、设备操作和结果验证。

当前任务的主要目标：

1. 将 Cursor Agent 作为 livstyle-bot 的一个独立 Agent 后端。
2. 保留 Octos 原有 Agent Runtime 和已实现能力，包括工具、记忆、工作流、任务管理及其他项目已有功能。
3. 允许用户在产品层选择使用 Octos 或 Cursor Agent，尽量复用两者都能复用的 UI、任务展示、事件消费和产品级能力。
4. 用 Rust 实现宿主侧的进程管理、HTTP/1.1 Connect 协议客户端、事件适配、错误处理和后端抽象。
5. 不新增需要用户独立安装和运行的 Node.js 服务。
6. 不为接入 Cursor 而大规模重写 Octos。
7. 在真实源码基础上完成修改、测试和验证，不能只给方案。

请自行分析当前仓库的结构、依赖和现有实现，按照最小侵入原则实施。

## 二、首先进行真实源码审计

实施前，先读取并分析以下内容。路径只是审计目标，不代表这些文件一定存在：

- 根目录 `Cargo.toml`、`Cargo.lock`、README。
- 所有 workspace member 和 crate 的职责。
- Makepad 应用入口和启动流程。
- 当前 Octos 的初始化、Agent 创建、消息发送、流式事件接收和任务终止路径。
- 当前前端与后端之间的通信方式。
- 项目现有的事件模型、状态管理、错误类型和异步运行时。
- 项目现有的配置、日志、密钥处理和持久化方案。
- 现有工具注册、权限检查、工作流和硬件相关实现。
- 当前测试结构、CI 配置和实际可以运行的测试命令。

执行只读检查：

```bash
git status --short
git branch --show-current
git remote -v
```

如果仓库存在 `origin/master`，核实当前代码与 `master` 的关系。必要时可以执行非破坏性的 fetch 对比远端，但不要擅自执行 `git reset --hard`、强制切换分支、覆盖用户修改或清理工作区。

如果 Octos 是 Git 依赖、path dependency、workspace member、submodule 或独立服务，必须识别真实集成方式。

读取当前项目中的 Octos 依赖版本和实际 API。不要根据旧文档或记忆猜测方法名。

阅读 Cursor 官方 `sdk-bridge` 仓库中与当前发布版本一致的协议定义，尤其是 `proto/sdk/v1/`。先确认发布版本、manifest、服务名、消息字段、流式事件和错误结构，再编写客户端。

完成审计后，先在最终输出中简要记录：

- 实际仓库分支和工作区状态。
- 项目的 crate 结构。
- 当前 Octos 集成位置和调用入口。
- 当前 UI 与 Agent 事件的连接方式。
- 推荐新增或修改的真实文件路径。
- 目标平台支持情况和主要约束。

完成审计后继续实施，不要停留在方案阶段。

## 三、架构设计原则

### 3.1 两个独立 Agent 后端

目标架构逻辑如下：

```text
livstyle-bot / Makepad UI
          |
          v
产品级任务与事件协调层
          |
          +--------------------------+
          |                          |
          v                          v
   Octos Backend              Cursor Backend
          |                          |
          v                          v
   Octos Runtime           Rust Cursor SDK Adapter
          |                          |
          v                          v
Octos 原有能力              Cursor SDK Bridge
                                     |
                                     v
                                Cursor API
```

这只是逻辑结构。实际实现时应遵循当前项目的 crate 划分、运行时和 UI 通信模式。

要求：

- Octos 与 Cursor 是并列的 Agent 后端。
- 用户可以选择后端。
- 每个任务创建时必须明确使用哪个后端。
- 同一任务执行过程中不得因为调用失败而静默切换后端。
- 不得丢失原有 Octos 特性和已有用户数据。
- 不要把两个后端强行统一成仅支持文本问答的接口。
- 能共享的产品级能力可以共享，但保留不同后端的专属能力。
- 如某项能力只由一个后端支持，应显式标记能力或返回清晰错误，不得伪造支持。

### 3.2 不要滥改 Octos

请检查真实的 Octos 架构。若当前项目使用的是 Octos crate，则尽量继续使用其现有公开接口。

例如，若源码确实存在下列职责：

- `octos-agent`：Agent 执行、上下文、工具、hooks、任务执行。
- `octos-core`：公共类型和协议定义。
- `octos-llm`：模型 Provider、路由、重试和故障转移。
- `octos-memory`：记忆和检索。
- `octos-plugin`：插件或技能集成。
- `octos-workflows`、`octos-swarm`：工作流与协作执行。
- OUP 或其他现有 UI/runtime 协议。

则应保留这些职责，并以实际源码为准。

**Cursor Agent 是完整的 Agent Runtime，不是单纯的 LLM 推理接口。**

因此不要仅为接入 Cursor 就将整个 Cursor Agent 塞进 `octos-llm`，也不要未经证明就修改 Octos 内部的 Agent 执行链。

优先将 Cursor 后端适配器放在 livstyle-bot 当前应用层或合适的独立 crate 中。只有发现必须修改 Octos 的公共接口时才修改，并且需要说明原因、兼容性和测试结果。

### 3.3 采用最小侵入式后端边界

如果项目已经存在后端接口，就复用并适当扩展，不要再造第二套重复抽象。

如果不存在统一接口，可以在适合的应用层新增一个小型后端边界，表达以下概念：

- 后端标识和能力描述。
- 创建或恢复任务。
- 发送用户输入。
- 接收流式事件。
- 中断执行。
- 关闭或释放资源。
- 获取执行状态和错误。
- 追踪后端任务 ID 与产品任务 ID 的关系。

接口设计应考虑 Rust 异步生命周期和事件流，不要用阻塞循环卡住 Makepad UI 线程。

不要为了所谓统一接口，把 Cursor 的原始事件、工具调用或执行状态压缩成只有字符串的消息。内部事件模型应保留必要的结构化信息，并尽可能保留原始事件以便诊断。

所有类型和函数必须与当前项目的真实技术栈兼容。上面的能力描述是需求，不代表要求使用某个固定的 trait 名称。

## 四、Cursor SDK Bridge 的具体接入

### 4.1 技术约束

优先采用 Cursor 官方的 `sdk.v1` 协议和官方发布的独立 Bridge 可执行文件，不直接将 TypeScript SDK 集成到主应用中。

官方入口：

https://github.com/cursor/sdk-bridge

请遵守官方协议约定：

- 使用当前选定发布版本对应的 `proto/sdk/v1/` 定义。
- 固定并记录 Bridge / SDK 协议版本。
- 不自行猜测 RPC 方法、消息名称或字段。
- 不编辑 Cursor 发布仓库维护的 proto 源文件。
- 使用 Rust 实现适配器。
- Bridge 使用 HTTP/1.1 Connect 协议，不可直接使用仅适用于传统 HTTP/2 的经典 gRPC 客户端。
- 优先使用兼容的 Rust Connect/Protobuf 实现；若生态没有合适的 Connect 客户端，可以根据官方定义实现最小必要的 HTTP/1.1 POST 传输层。
- 按真实协议要求实现请求认证、流式读取、错误映射及关闭流程。
- 所有 RPC 的路径和负载必须与固定版本的 proto 保持一致。
- 必须实现超时、取消、重连和进程清理，不能只实现一次成功请求的 Demo。

### 4.2 Bridge 生命周期管理

在实际合适的 crate 中实现可测试的 Bridge Manager，负责：

1. 查找已配置的 Bridge 可执行文件。
2. 验证可执行文件是否存在、是否可启动及版本是否兼容。
3. 启动子进程，或按配置连接已存在的本地 Bridge。
4. 读取官方定义的 ready-line/handshake。
5. 获取 Bridge endpoint 和本进程的 bearer token。
6. 调用 Ping、GetVersion 等协议定义的能力检查接口。
7. 检查 `sdk.v1` 协议版本和运行能力。
8. 为所有请求正确附加 Bridge bearer token。
9. 对创建 Agent、恢复 Agent、模型列表等需要 Cursor API key 的调用，按协议显式传入对应的 API key。
10. 支持优雅关闭和必要的异常终止。
11. 避免退出、取消或初始化失败时留下僵尸进程。
12. 在应用重启、网络中断、Bridge 崩溃后返回可诊断的状态。

设计上应允许测试注入一个虚拟子进程或模拟 Bridge endpoint，避免单元测试依赖真实 Cursor 服务。

可执行文件位置、端口、启动参数和 ready-line 的格式，全部以选定版本的官方协议文档为准。

### 4.3 客户端功能

围绕当前发布的 `sdk.v1` 协议，实现当前项目需要的最小但完整功能。

首先读取 proto 与官方文档，再决定具体方法和类型。至少调查以下能力是否由固定版本支持：

- 获取 Bridge 版本与能力。
- 获取 Cursor 身份状态。
- 获取模型或目录信息。
- 创建 Agent。
- 恢复 Agent（如果受支持且当前项目需要）。
- 向 Agent 发送消息。
- 流式接收运行事件。
- 中断或取消运行。
- 结束 Agent 或关闭相关资源。
- 读取工具执行状态、最终状态、错误和可用的产物信息。

为 Rust 适配器设计清晰的内部边界，例如：

```text
CursorBridgeManager
CursorConnectClient
CursorAgentBackend
CursorRunHandle
CursorEventAdapter
```

这些名称只是建议。如果仓库已有相近实现或命名风格，应复用现有设计。

必须支持：

- 多条连续消息。
- 流式事件增量更新。
- 完成、失败和取消等终态。
- 结构化错误。
- 网络连接失败。
- Bridge 意外退出。
- 长任务期间的超时和 UI 响应。
- 正确区分产品任务 ID、Cursor Agent ID、运行 ID 和请求 ID。
- 防止重复消费事件导致 UI 重复显示。
- 对能够恢复的流使用官方协议支持的游标或恢复机制。

不要用轮询一个假造的 endpoint 代替官方流协议。

### 4.4 认证与费用必须如实处理

不要假定 Cursor IDE 登录态可以直接用于第三方 Rust 适配器，也不要读取、复制或绕过 Cursor IDE 私有凭据。

优先使用官方文档支持的 API key 流程。根据实际协议和 SDK 版本核实 API key 应该如何配置与传递。

要求：

- API key 不得硬编码。
- 不得提交密钥进入 Git。
- 不得写入普通日志、事件、UI错误文本或 crash dump。
- 配置来源优先采用当前项目安全的 secret/config 机制；如果不存在，就实现最小的环境变量配置和用户可理解的缺失配置错误。
- 区分 Cursor API key 与 Bridge 的本地 bearer token。
- Bridge bearer token 不得泄露给 UI 或外部网络。
- 不得把认证失败误报为 Agent 执行失败或模型不可用。
- 不得把某种账号权限、订阅额度或模型可用性写死为所有用户均具备。

重点核实：Cursor SDK 使用 API key 时实际的计费方式、请求额度、隐私模式和账号权限。不要宣称普通 Cursor IDE 订阅必然包含所有 SDK 用量。将已验证结论、文档来源和需要用户确认的账号条件写入项目文档。

## 五、安全边界：代码开发与硬件操作分离

这是 livstyle-bot 的核心安全要求。

目标工作流为：

```text
自然语言目标
    |
    v
选择 Agent 后端
    |
    v
分析与代码修改
    |
    v
构建、静态检查与测试
    |
    v
展示变更与构建产物
    |
    v
用户明确授权硬件操作
    |
    v
硬件执行器烧录/部署
    |
    v
读取设备日志并验证结果
    |
    v
形成可追踪的验证证据
```

Cursor Agent 可以参与代码分析、修改、解释和构建，但不得通过通用 Agent 工具绕过产品层的硬件授权流程。

具体要求：

1. 先梳理现有硬件工具、串口、ADB、烧录程序、SSH、设备部署命令等能力。
2. 不把硬件烧录、设备复位、固件部署或其他高风险硬件操作作为 Cursor 默认可直接调用的无审批工具。
3. Cursor Agent 的普通代码工作流不得获得不受限制的硬件操作能力。
4. 必须检查 Cursor Agent 运行时的 shell、文件修改和工具调用权限。官方文档说明，无人值守的本地 Agent 可能自动批准工具调用，因此不能仅依靠 Makepad UI 的一个审批对话框来确保安全。
5. 优先使用官方协议或当前 SDK 确实支持的沙箱与工具控制能力。若需要自建受限命令执行器，应只允许明确列出的安全开发命令，并阻止其触发硬件执行流程。
6. 尽可能让 Cursor 在经过权限限制的工作目录中工作，限制敏感凭据读取和不必要的网络访问。
7. 如果平台无法可靠隔离高风险命令，必须明确标注限制、关闭相关自动执行能力或采取更严格的隔离方案，不得宣称仅靠字符串黑名单即可实现安全。
8. 所有真实硬件动作必须经过独立硬件执行器和产品层授权，不允许 Cursor Agent 直接绕过该执行器。
9. 授权需绑定具体设备、操作类型和待执行产物；产物或目标改变后应重新确认。
10. 对烧录、部署、擦除、重启等操作保留审计事件。
11. 不要破坏现有 Octos 工具权限体系，也不要因为 Cursor 的工具体系不同而取消已有安全限制。

若现有硬件安全架构还未实现，就先保留明确的安全边界，不要为了快速演示而新增一个无条件执行任意 shell 命令的硬件工具。

## 六、跨平台支持策略

项目目标平台包括 Windows、Linux 和 Android。

请按 Cursor 官方发布的 Bridge 资产和本项目的实际构建目标，核实各个平台可用的可执行文件。不要推断桌面 Linux 二进制可以直接运行于 Android。

建议：

- Windows/Linux：在确认官方独立 Bridge 有对应可执行文件、架构和运行条件后，实现本机子进程模式。
- Android：先核实是否有受支持的 Bridge 二进制、沙箱条件、网络限制和进程权限。若没有经过验证的官方本机运行方式，不要伪装成支持本机 Cursor Agent。
- 对 Android 可考虑复用桌面端已有 Bridge 的配对/远程连接模式，但只有在有明确的安全认证、用户授权、连接生命周期和网络协议设计时才实施。
- 如果远程连接不属于本次可安全完成的范围，则 Android 上将 Cursor 后端标记为不可用或需要配对；Octos 仍保持正常可用。
- 不得让 Linux/Windows 上不可用的外部工具悄悄成为整个 Makepad 应用启动的强制依赖。

在文档中明确记录各平台的能力矩阵和限制。

还必须区分以下两个要求：

- “不需要用户额外安装 Node.js、无需独立维护 Node.js 服务”。
- “所有可执行进程必须完全由 Rust 编译，不允许任何非 Rust 运行时”。

官方独立 SDK Bridge 可以满足前者，但由于它本身包含 Bun runtime，不能宣称满足后者。请按当前产品需求优先实现官方 Bridge 方案，并明确报告这一限制。如果项目实际要求严格的全 Rust 运行时，则不要偷偷引入 Bridge；先报告无法同时满足的约束及经过验证的可选方案，不得通过实现一个未经官方支持的私有 API 规避限制。

## 七、UI 与事件适配

检查现有 Makepad UI，不要臆造新的 UI 框架。

尽可能复用当前的：

- 后端选择或配置入口。
- 会话与任务视图。
- 消息展示。
- Agent 状态。
- 工具执行和进度展示。
- 错误提示。
- 取消/停止操作。
- 设置与密钥配置流程。

只有项目目前不存在相关功能时才新增必要 UI。

事件适配需要处理：

- Agent 状态变化。
- 文本增量与完整消息。
- 工具调用及相关状态（以协议真实提供内容为准）。
- 运行进度与终态。
- 错误与取消。
- 可用的 artifacts/usage 元数据。

不得假设 Cursor 事件与 Octos 事件一一对应。应在产品边界中建立清楚的映射；不支持映射的事件要保留或显式标注。

不要阻塞 UI 线程。异步任务应遵守当前 Makepad 项目的消息传递和状态更新机制。

## 八、错误、日志和可观测性

为以下错误提供可理解且可诊断的信息：

- Bridge 不存在。
- Bridge 启动失败。
- 版本或协议不兼容。
- 认证失败。
- API key 缺失。
- 网络错误。
- API 限流或权限不足。
- RPC 错误。
- 流式响应中断。
- Agent 创建或恢复失败。
- Agent 主动失败、取消或超时。
- 平台不支持。
- 用户取消任务。

日志中应保留足够的请求/运行 ID、耗时、错误代码和后端标识，以便排查问题。

严禁记录 API key、bearer token、完整认证头和其他敏感凭据。不要默认输出完整用户提示词、源代码或设备日志到可能泄露的日志文件中。

## 九、依赖与代码质量

- 使用现有 Rust edition、异步运行时、错误处理和日志体系。
- 优先复用项目依赖，避免无必要地引入第二个异步运行时。
- 根据协议版本选择合理的 HTTP、Protobuf/Connect、流处理依赖。
- 为依赖和 proto 生成过程提供可复现的版本固定。
- 不要在项目中添加 Node.js 服务、npm workspace 或要求用户通过 Node.js 启动的守护进程。
- 不要将 API key 或个人凭据写入 `Cargo.toml`、源码或仓库配置。
- 不做与本次任务无关的重构。
- 不删除或绕开当前已有测试。
- 不以 `#[allow(unused)]` 等方式掩盖未实现的核心功能。
- 不使用伪造返回值、硬编码模型列表或模拟成功状态冒充真实后端。
- 不添加没有实际实现的空函数或 TODO 来宣称任务已完成。

## 十、实施阶段

请依照以下阶段实际执行。

### P0：源码审计和方案确认

检查工作区、Octos 使用方式、UI 事件模型、配置机制、平台目标，以及 Cursor Bridge 当前协议。

更新或创建一份短的实施记录，包含真实文件路径、选择的 Bridge 版本、依赖方案和平台限制。

### P1：建立 Cursor Bridge 基础设施

完成：

- Bridge 配置与可执行文件发现。
- 子进程启动和 handshake。
- 版本与能力检测。
- 本地认证令牌处理。
- HTTP/1.1 Connect 请求。
- 安全关闭与资源清理。
- 可测试的 transport/manager 边界。

先完成真实 Ping/GetVersion 的协议验证。不要在基础 RPC 还未验证时就开发 UI。

### P2：实现 Cursor Agent 后端

根据固定版本的官方 proto 完成：

- 创建与必要的恢复逻辑。
- 发送消息。
- 接收流式事件。
- 取消和关闭。
- 错误映射。
- Agent/Run ID 管理。
- UI 使用的事件适配。

用适当的单元测试覆盖协议编码、错误处理和生命周期。

### P3：集成现有应用与 Octos

- 新增或扩展最小的后端选择机制。
- 将 Cursor 后端注册到正确的应用层。
- 保留现有 Octos 默认行为和调用路径。
- 在 UI 中展示真实后端和任务状态。
- 保证切换后端不会意外替换正在执行的任务。
- 保持会话、取消、错误和资源清理正确。
- 若产品已有记忆、工具、工作流等功能，明确哪些是应用层共享能力，哪些是 Octos 专属能力；不要虚假声称 Cursor 已支持 Octos 专属接口。

### P4：安全与平台处理

- 验证 shell 和工具权限。
- 核实硬件操作隔离。
- 确认各平台 Bridge 可执行文件。
- 为平台不支持、缺少凭据和 Bridge 不可用实现可理解的降级路径。
- 确保 Octos 在 Cursor 不可用时依旧能启动和正常工作。

### P5：测试与验收

运行当前项目适用的：

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

如果项目有现有 CI、特定 features、Makepad 构建约束、特定目标架构或以上命令不适用，请按真实 workspace 和构建配置执行对应命令，并记录原因。不要为通过检查而删除功能或放宽核心约束。

另外补充：

- Bridge Manager 单元测试。
- 模拟 Bridge 启动和失败的测试。
- Connect 请求、认证头和错误映射测试。
- Agent 事件流解析测试。
- 中断、超时和资源清理测试。
- Octos 原有功能回归测试。
- UI 后端选择和状态更新测试（按项目现有测试能力实现）。
- 硬件审批边界测试。
- Cursor 不可用时 Octos 正常工作测试。

真实 Cursor API smoke test 可依赖用户自行配置的 `CURSOR_API_KEY`。没有凭据时必须明确跳过，不得伪造通过结果。

如果 Android target 未安装，或特定构建环境不具备条件，应报告具体阻塞原因，而不是声称已验证成功。

## 十一、验收标准

只有在以下条件都经过实际检查后，才可以将工作标记为完成：

1. 项目中存在真正的 Cursor Rust 适配器，而不是纯设计文档。
2. Bridge 协议与固定版本的官方 proto 一致。
3. Rust 可以正确启动并管理 Bridge，或连接已配置的 Bridge endpoint。
4. 真实 RPC 可完成协议级验证；真实 Agent 执行测试只有在具备有效凭据时才执行。
5. Cursor Agent 与 Octos 是可区分的后端。
6. Octos 的原有 Agent、记忆、工作流和工具能力未被为了接入 Cursor 而移除。
7. Makepad UI 不因 Agent 的长任务或事件流被阻塞。
8. 取消、异常退出、认证错误和网络中断不会导致资源泄漏或 UI 永久卡死。
9. 密钥与 Bridge bearer token 不会暴露到 UI、源码或日志。
10. Cursor Agent 无法直接越过独立硬件执行器完成高风险硬件操作。
11. 各平台支持情况有明确的实际验证结果。
12. 至少完成适用的格式化、编译和测试，并报告实际结果。

## 十二、最终报告格式

完成后，用中文输出报告：

### 1. 源码审计结果
列出实际检查到的 workspace/crate、Octos 集成入口、UI 事件路径和配置机制。

### 2. 架构改动
说明 Octos、Cursor Agent 和产品级任务层的关系。

### 3. 文件修改清单
逐个列出新增/修改的实际文件路径、职责与关键实现。

### 4. Cursor Bridge 具体实现
记录固定版本、proto 来源、RPC 方法来源、认证流程、事件流、取消及生命周期实现。

### 5. 安全与平台限制
说明 Cursor shell 工具权限、硬件授权边界、API key 管理、计费待确认事项，以及 Windows/Linux/Android 支持情况。

### 6. 测试结果
逐条列出实际执行的命令及成功/失败/跳过结果。失败时给出真实错误与剩余阻塞项。

### 7. 后续工作
仅列出本次没有完成、且有明确原因的事项。

现在开始执行：先检查真实源码，随后连续完成 P0–P5。不要只生成实施方案，不要在没有必要时停下来询问。遇到无法访问的外部文档或缺失凭据时，继续完成不依赖它们的工作，并如实标出限制。不得捏造源文件、API、运行结果或测试通过状态。