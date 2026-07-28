# Requirements / 需求文档: Assistant 运行模式

## Restated Understanding / 需求复述

新增 `ConversationMode::Assistant` 模式，用于自动化课题推进。Assistant 不是在 `packages/agent-app/src-tauri/src/core/engine.rs` 中继续扩写的新分支，而应收束到独立的助手模式文件中；后续 `engine` 只负责引入、实例化和调用 Assistant 对外接口。

Assistant 对外提供三个核心方法：

- 对话：接收用户输入，按 Assistant 流程推进一轮对话。
- 步进：由用户或上层系统手动启动一轮循环，行为应尽量与自动轮询触发的一轮保持一致。
- 注册系统轮询：把 Assistant 的自动推进逻辑注册到系统级 Poller 中，由 Poller 按间隔触发。

Assistant 内部每轮流程统一为：**beforehook → Assistant 对话/步进核心 → afterhook**。用户输入、手动步进和系统轮询触发都应复用同一套上下文构建、工具调用和 hook 执行规范，避免把 Assistant 业务散落到 engine、TUI 或 poller 中。

核心行为保持：

- **一次工具调用即结束**：与 Agent 模式的迭代循环不同，Assistant 模式每轮调一次 LLM + 一次工具执行后即结束，不继续轮询 LLM。
- **外层轮询驱动**：系统级 Poller 只负责调度到期任务；Assistant 注册自己的 handler，在触发时自行判断课题、会话和轮次推进。
- **神经元动态选为 system prompt**：beforehook 可在每轮前准备候选神经元，完成 7 选 1，选中 neuron 的 content 作为 system prompt 插入本轮上下文。
- **神经元持有工具权限**：系统工具注册表仍负责登记工具能力，但 Assistant 每轮可见、可调用的工具集由本轮选中的神经元决定；神经元持久化记录自己获准使用的工具 ID 集合。
- **课题与会话绑定**：Assistant 会话必须绑定课题，一个课题一个绑定会话。
- **AI 和用户均可全权管理课题**（已验证：Agent 工具 + TUI `/topic` 已覆盖）。
- **课题匹配**：用户输入时，Assistant 可通过 beforehook 检索未完成课题；匹配则切换到对应会话转发输入，无匹配则创建新课题关联当前会话。
- **轮后更新**：afterhook 可在每轮完成后检查 scope_in 完成情况，更新 scope_in 状态，重新计算 progress，全部完成则自动标记 Done。
- **课题关联会话**：Topic 增加 `session_id` 字段，轮询时判断是否已有活跃会话。

## Prerequisites / 前置依赖

- 神经元的数据模型、模型能力注入、自举创建、候选选择和工具暴露边界以 `docs/sdd-lab/2026-07-28_23-43_neuron-bootstrap/requirements.md` 为准。
- Assistant 的神经元选择、神经元工具权限和相关 Hook 接入必须等待该前置需求确认并实现。
- 本文只约束 Assistant 如何消费神经元能力，不重复定义神经元内部创建和管理实现。

## Scope / 范围

### In

- `ConversationMode` 新增 `Assistant` 变体。
- 新增独立 Assistant 模式文件，承载 Assistant 的状态、上下文构建、hook 调用、LLM 调用、工具调用、轮后处理和轮询注册逻辑。
- `engine.rs` 不承载 Assistant 业务实现；后续只允许作为上层入口实例化并调用 Assistant 对外方法。
- Assistant 对外提供三个方法：对话、步进、注册系统轮询。
- Assistant 每轮只执行一次 LLM 调用和一次工具调用，工具结果拼接回对话后结束本轮。
- Hook 设计改为助手级 `beforehook` / `afterhook`，不再以 PreHook0/PreHook1/PreHook2 作为通用协议命名。
- Hook 规范必须足够通用：不同 hook 功能可以不同，但都遵守同一输入、输出、执行时机和工具访问规则。
- beforehook 接收与 Assistant 对话相同级别的上下文，原则上可访问与对话一致的工具能力；它可以更新本轮对话入参，也可以只借用调用时机处理自身业务。
- afterhook 接收本轮对话上下文与对话出参，原则上可访问与对话一致的工具能力；它可以更新本轮对话出参，也可以只借用调用时机处理自身业务。
- 神经元模型新增允许使用的工具 ID 集合；该集合是 Assistant 工具权限的持久化来源。
- 系统工具注册表保留全部已注册工具及其稳定 ID，Assistant 根据选中神经元记录的工具 ID 从注册表解析本轮工具集。
- Assistant 只向模型暴露选中神经元获准使用的工具定义，并只允许执行该集合内的工具调用。
- beforehook/afterhook 在选中神经元确定后，与 Assistant 对话共享同一份经过神经元权限过滤的工具集，不得绕过权限直接使用系统完整工具集。
- 神经元记录的工具 ID 在系统注册表中不存在时，不得扩大权限或回退为完整工具集；具体报错或忽略策略由技术方案确定。
- Topic 表新增 `session_id` 字段（SQLite 迁移）。
- 神经元上下文 beforehook：获取候选神经元并选择本轮 system prompt；7 选 1（可配置），权重优先 + 同权重随机，不足时调用 LLM 创建。
- 神经元候选准备和自动补齐由前置神经元能力提供；Assistant 不自行复制神经元自举逻辑。
- 次生轮次选择：以已选神经元为起点 BFS 取邻居，不足时补充创建。
- 用户介入类 beforehook：检测输入与课题相关性、识别满意/纠偏/大怒/重来等反馈，并据此更新关联神经元权重或回滚对话。
- 轮后 afterhook：每轮完成后检查 scope_in 状态，更新 scope_in，重算 progress，全部完成则自动标记 Done。
- 系统级 `Poller`：作为通用轮询调度器，允许多个业务按不同间隔注册 handler；Poller 只调度并调用 handler，不理解 Assistant 业务，也不返回 handler 结果。
- TUI 命令：`/new_assistant`、`/poll`（status/pause/resume/trigger）。
- 会话列表显示 `[Assistant]` 标签。

### Out

- Assistant 模式与 Chat/Agent 模式共享会话（不支持 mode 切换）。
- 多课题并行轮询的并发上限控制。
- 持久化神经元评分历史。
- Hook 结果持久化（除用户反馈导致的 neuron weight 更新）。
- 在 `engine.rs` 内直接实现 Assistant 流程或 hook 细节。
- Poller 内硬编码 Assistant、Topic、Neuron 等业务逻辑。
- 为 Assistant 或 hook 另建一套脱离系统工具注册表的工具实现。
- Assistant、beforehook 或 afterhook 绕过神经元权限使用系统完整工具集。

## Tool Permission / 神经元工具权限

- 系统工具注册表是工具能力目录，负责工具的注册、定义查询和执行分发；它不是 Assistant 的最终授权来源。
- 每个神经元持久化一组允许使用的工具 ID，表示该神经元能够使用哪些系统工具。
- 每轮选出神经元后，Assistant 使用这些工具 ID 从系统工具注册表解析出本轮工具定义和执行权限。
- 模型只能看到解析后的授权工具定义；模型返回工具调用时，Assistant 必须再次校验工具 ID 是否属于本轮授权集合。
- 对话核心、beforehook 和 afterhook 在权限确定后共享同一份授权工具上下文。
- 空工具 ID 集合表示该神经元本轮无工具权限，不得解释为允许使用全部工具。
- 工具权限跟随神经元持久化，不跟随会话临时复制；神经元权限更新后，后续轮次使用更新后的权限集合。

## Hook Contract / Hook 规范

Assistant 级 hook 统一分为两类：

- `beforehook`：在 Assistant 核心对话/步进前执行。
- `afterhook`：在 Assistant 核心对话/步进后执行。

Hook 必须遵守以下通用规范：

- Hook 的输入上下文应与 Assistant 对话上下文保持同源，至少包含会话、课题、轮次、用户输入或轮询触发信息、可用工具、候选上下文和运行配置。
- Hook 使用与 Assistant 对话相同的系统工具注册体系和神经元授权结果；不为 hook 设计一套割裂的专用工具系统。
- `beforehook` 可以返回更新后的对话入参，例如 system prompt、消息上下文、课题绑定、神经元选择结果或工具可见上下文。
- `beforehook` 也可以不更新入参，只利用调用时机执行业务逻辑，例如记录状态、打分、触发外部同步。
- `afterhook` 可以返回更新后的对话出参，例如最终展示内容、工具结果补充、课题进度或状态变更。
- `afterhook` 也可以不更新出参，只利用调用时机处理轮后业务，例如统计、归档、状态同步。
- 单个 hook 的业务能力可以不同，但不应改变 hook 协议本身；新增 hook 应通过实现同一规范接入。
- Hook 执行失败时的错误处理策略由技术方案定义；需求层面要求失败不应破坏 Poller 的通用调度职责。

## Poller Requirement / 通用轮询需求

系统级 Poller 是通用轮询调度器，不属于 Assistant 专用模块。它以固定最小 tick 粒度运行，允许多个业务注册不同间隔的 handler；每次 tick 时只判断哪些任务到期，然后直接调用对应 handler 的 `on_tick`。

Poller 需要支持：

- 创建时指定基础间隔。
- 注册具名任务，每个任务有自己的间隔倍数和 handler。
- `tick` 时按到期规则调用 handler。
- `start`、`pause`、`resume`、`trigger` 和 `status`。
- `trigger` 表示下一次 tick 调用所有 handler。

Poller 的边界：

- Poller 不返回业务值，不承诺向调用方抛出业务异常。
- Handler 内部自行处理业务错误、重试、日志和状态写入。
- Poller 不理解 Assistant 课题、会话、hook 或神经元，只负责调度。
- Assistant 通过“注册系统轮询”方法把自身推进逻辑包装成 PollHandler 接入 Poller。

## Acceptance Criteria / 验收标准

- [ ] Assistant 业务逻辑收束在独立助手模式文件中，`engine.rs` 不直接实现 Assistant 流程。
- [ ] `engine.rs` 后续只需实例化 Assistant 并调用其对外方法即可接入 Assistant 模式。
- [ ] Assistant 对外暴露对话、步进、注册系统轮询三个方法。
- [ ] 用户输入、手动步进、系统轮询触发复用同一套 Assistant 每轮流程。
- [ ] 新 Assistant 会话直接输入，工具调用一次即结束，结果拼接回对话。
- [ ] Assistant 会话必须绑定课题，新建时自动关联或通过 beforehook 匹配/创建课题。
- [ ] 用户输入时 beforehook 可检索未完成课题，匹配时切换到对应会话转发输入。
- [ ] 用户输入时 beforehook 无匹配可创建新课题并关联当前会话。
- [ ] Hook 体系只暴露助手级 `beforehook` / `afterhook` 通用规范，不再以 PreHook0/1/2 作为通用协议。
- [ ] beforehook 能更新本轮对话入参，也能只作为业务调用时机运行。
- [ ] afterhook 能更新本轮对话出参，也能只作为业务调用时机运行。
- [ ] hook 获取的上下文与 Assistant 对话上下文同源，工具访问原则上与 Assistant 对话一致。
- [ ] 神经元 beforehook 完成神经元 7 选 1：不足时调用 LLM 创建，权重优先，同权重随机。
- [ ] 次生轮次以已选神经元为起点 BFS 取邻居。
- [ ] 选中神经元 content 作为本轮 system prompt 注入 Assistant 对话上下文。
- [ ] 神经元持久化记录允许使用的工具 ID 集合。
- [ ] Assistant 根据选中神经元的工具 ID 从系统工具注册表解析本轮授权工具集。
- [ ] Assistant 只向模型暴露本轮神经元授权的工具，并在执行前再次校验权限。
- [ ] beforehook、Assistant 对话核心和 afterhook 在权限确定后共享同一份授权工具集。
- [ ] 空工具 ID 集合表示无工具权限；未知工具 ID 不得导致回退到系统完整工具集。
- [ ] 用户介入时检测情感，对关联神经元 ±1 分。
- [ ] "重来"情感回滚对话至上一次介入点。
- [ ] afterhook 每轮后更新 scope_in 状态，重算 progress。
- [ ] scope_in 全部完成时课题自动标记 Done。
- [ ] Poller 是通用轮询调度器，通过 handler 注入业务逻辑，不包含 Assistant 专用分支。
- [ ] Poller 支持 pause/resume/trigger/status，并在 trigger 后的下一次 tick 调用所有 handler。
- [ ] `/poll pause` 暂停轮询，`/poll resume` 恢复，`/poll trigger` 手动触发一次。

## Constraints / 约束

- 本次只更新需求文档，不修改代码。
- 无新增外部依赖，复用现有 async_openai / rusqlite / tokio。
- Assistant 业务实现不得落在 `packages/agent-app/src-tauri/src/core/engine.rs`。
- Poller 后台任务使用 tokio，与 TUI 事件循环共存。
- session_id 写入 SQLite，运行中状态通过 session_tracker 判断。
- Session ID 格式复用现有 `conv_{now_ms}_{random}` 规则。

## Open Questions / 开放问题

- [x] Q1 Assistant 业务应该落在哪里？
  - 修正：不在 `engine.rs` 中实现，新增独立助手模式文件。`engine.rs` 后续只负责引入、实例化和调用。
- [x] Q2 Assistant 对外接口是什么？
  - 修正：对外只约束三个核心方法：对话、步进、注册系统轮询。
- [x] Q3 Hook 通用协议如何命名和分层？
  - 修正：助手级 hook 统一改为 `beforehook` / `afterhook`；TopicMatch、NeuronContext、Intervention、AfterRound 等只是具体 hook 能力，不再定义为 PreHook0/1/2 协议。
- [x] Q4 beforehook 和 afterhook 分别能做什么？
  - 默认：beforehook 可更新对话入参或仅使用调用时机；afterhook 可更新对话出参或仅使用调用时机。
- [x] Q5 Hook 能否使用工具？
  - 默认：hook 应拿到与 Assistant 对话同源的上下文，原则上使用与对话一致的工具能力。
- [x] Q6 Poller 是否是 Assistant 专用？
  - 修正：Poller 是系统级通用轮询调度器，通过 handler 注入业务逻辑，不返回业务值，不处理业务异常。
- [x] Q7 "次生轮次"如何判定？
  - 默认：Poller 对该课题发起的第 2 次及以后的处理即为次生轮次（而非按消息条数）。
- [x] Q8 在本轮神经元尚未选出前，负责候选准备和神经元选择的 beforehook 可以使用哪些工具？
  - 决策：不开放模型可调用的系统工具。beforehook 调用前置神经元迭代定义的内部候选准备与自举能力；选出神经元后再建立本轮授权工具集。

## Requirement Decisions / 需求决策

- 2026-07-26 21:30:
  - 决策：draft 阶段，需求复述已获确认，进入需求文档编写。
- 2026-07-28 00:57:
  - 决策：Reverse Sync。当前已执行技术方案不符合预期，PreHook1 从“提交参数预处理”修正为“神经元上下文准备”，业务 Hook 必须按通用协议实现。
- 2026-07-28 21:30:
  - 决策：重新约束需求。Assistant 业务必须收束到独立助手模式文件；对外接口限定为对话、步进、注册系统轮询；Hook 协议改为助手级 beforehook/afterhook；Poller 明确为系统级通用轮询调度器。
- 2026-07-28 22:07:
  - 决策：Assistant 工具权限改由神经元持有。神经元记录允许使用的工具 ID；Assistant、beforehook 和 afterhook 在权限确定后共享经神经元授权过滤的工具集，不得直接使用系统完整工具集。
- 2026-07-28 23:50:
  - 决策：神经元自举与工具契约拆分到 `docs/sdd-lab/2026-07-28_23-43_neuron-bootstrap/`，作为本迭代前置依赖；神经元选出前的 beforehook 只调用该模块内部能力，不开放系统完整工具集。
