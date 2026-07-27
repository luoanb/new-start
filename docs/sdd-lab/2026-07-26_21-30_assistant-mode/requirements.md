# Requirements / 需求文档: Assistant 运行模式

## Restated Understanding / 需求复述

新增 `ConversationMode::Assistant` 模式，用于自动化课题推进。核心逻辑：

整个流水线（用户输入）：**PreHook0(TopicMatch) → [PreHook2 (仅次生轮次)] → PreHook1(NeuronContext) → Assistant Engine → AfterHook**
Poller 轮询（无用户输入）：**Poll → [PreHook2 (仅次生轮次)] → PreHook1(NeuronContext) → Assistant Engine → AfterHook**

- **一次工具调用即结束**：与 Agent 模式的迭代循环不同，Assistant 模式调一次 LLM + 一次工具执行后即结束，不继续轮询 LLM
- **外层轮询驱动**：Poller 系统启动时扫描进行中课题，按规则发起/复用会话推进，支持暂停/恢复/手动触发
- **神经元动态选为 system prompt**：PreHook1 每轮前选 7 个神经元 → LLM 7 选 1 → 选中 neuron 的 content 作为 system prompt 插入对话
- **课题与会话绑定**：Assistant 会话必须绑定课题，一个课题一个绑定会话
- **AI 和用户均可全权管理课题**（已验证：Agent 工具 + TUI `/topic` 已覆盖）
- **TopicMatch Hook（PreHook0）**：用户输入时，检索未完成课题 → 匹配则切换到对应会话转发输入 → 无匹配则创建新课题关联当前会话
- **AfterHook**：每轮完成后检查 scope_in 完成情况 → 更新 scope_in 状态 → 重新计算 progress → 全部完成则自动标记 Done
- **Hook 系统**：对话模式共享通用 PreHook / AfterHook 协议；Assistant 业务所需的 TopicMatch、NeuronContext、Intervention、AfterRound 都按该协议实现
- **课题关联会话**：Topic 增加 `session_id` 字段，轮询时判断是否已有活跃会话

## Scope / 范围

### In

- `ConversationMode` 新增 `Assistant` 变体
- Engine 内 `assistant_mode()` 方法（一次工具调用 + 拼接结果，不循环）
- 轮询模块 `Poller`：后台任务扫描进行中课题，创建/复用会话，支持 pause/resume/trigger
- Topic 表新增 `session_id` 字段（SQLite 迁移）
- PreHook1（NeuronContext）：获取候选神经元并选择本轮 system prompt；7 选 1（可配置），权重优先 + 同权重随机，不足时调用 LLM 创建
- 次生轮次选择：以已选神经元为起点 BFS 取邻居，不足时补充创建
- TopicMatch Hook（PreHook0）：用户输入时检索未完成课题，匹配则切换会话，无匹配则创建新课题
- 前置 Hook（PreHook1）：获取神经元信息，输出本轮 Assistant Engine 需要注入的 system prompt，Hook 结果不记录对话
- 前置 Hook2（PreHook2）：用户介入检测 + 神经元打分（满意/纠偏 +1，大怒/重来 -1）
- AfterHook：每轮完成后检查 scope_in 状态 → 更新 scope_in → 重算 progress → 全部完成则自动标记 Done
- TUI 命令：`/new_assistant`、`/poll`（status/pause/resume/trigger）
- 会话列表显示 `[Assistant]` 标签

### Out

- Assistant 模式与 Chat/Agent 模式共享会话（不支持 mode 切换）
- 多课题并行轮询的并发上限控制
- 持久化神经元评分历史
- Hook 结果持久化（除 PreHook2 的打分写入 neuron weight）

## Acceptance Criteria / 验收标准

- [ ] Assistant 会话必须绑定课题，新建时自动关联或走 PreHook0
- [ ] PreHook0 检索未完成课题，匹配时切换到对应会话转发输入
- [ ] PreHook0 无匹配时创建新课题关联当前会话
- [ ] 新 Assistant 会话直接输入，工具调用一次即结束，结果拼接回对话
- [ ] Poller 启动后扫描进行中课题，为无关联会话的课题创建 Assistant 会话
- [ ] Poller 识别已有活跃会话时跳过，不重复创建
- [ ] `/poll pause` 暂停轮询，`/poll resume` 恢复，`/poll trigger` 手动触发一次
- [ ] PreHook1 完成神经元 7 选 1：不足时调用 LLM 创建，权重优先，同权重随机
- [ ] 次生轮次以已选神经元为起点 BFS 取邻居
- [ ] PreHook1 输出选中神经元 content，Assistant Engine 将其作为 system prompt 插入本轮对话
- [ ] 用户介入时检测情感，对关联神经元 ±1 分
- [ ] "重来"情感回滚对话至上一次介入点
- [ ] AfterHook 每轮后更新 scope_in 状态，重算 progress
- [ ] scope_in 全部完成时课题自动标记 Done

## Constraints / 约束

- 无新增外部依赖，复用现有 async_openai / rusqlite / tokio
- Poller 后台任务使用 tokio，与 TUI 事件循环共存
- session_id 写入 SQLite，运行中状态通过 session_tracker 判断
- Session ID 格式复用现有 `conv_{now_ms}_{random}` 规则

## Open Questions / 开放问题（已关闭，技术方案中按默认值处理）

- [x] Q1 前置 Hook（PreHook1）具体处理什么提交参数？
  - 修正：PreHook1 主要用于获取神经元信息。它接收当前课题、会话快照、轮次信息和候选策略，完成“候选神经元准备 + LLM 7 选 1”，输出本轮 system prompt 及选中 neuron 标识。
- [x] Q2 用户介入检测（PreHook2）中的"不相关"判断标准是什么？
  - 默认：由 LLM 判断，将用户输入与关联课题的 name + scope_in 描述一起交给 LLM 判断是否相关。
- [x] Q3 Poller 轮询间隔是否可配置？
  - 默认：硬编码 30s，不做 TUI 配置。
- [x] Q4 "次生轮次"如何判定？
  - 默认：Poller 对该课题发起的第 2 次及以后的处理即为次生轮次（而非按消息条数）。

## Requirement Decisions / 需求决策

- 2026-07-26 21:30:
  - 决策：draft 阶段，需求复述已获确认，进入需求文档编写
- 2026-07-28 00:57:
  - 决策：Reverse Sync。当前已执行技术方案不符合预期，PreHook1 从“提交参数预处理”修正为“神经元上下文准备”，业务 Hook 必须按 `docs/design/hook-spec.md` 的通用协议实现。
