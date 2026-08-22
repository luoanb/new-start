# Lifecycle / 生命周期: Hook 注入点契约与调度收编

```yaml
status: done
result: success
created_at: 2026-08-22 17:17
updated_at: 2026-08-22 22:30
owner: user
```

## Current Summary / 当前摘要

- 批准状态：技术方案已落盘，用户批准「开始执行」，Step 1-6 全部完成，测试全绿（`cargo test --lib` 376 passed；`pnpm --filter pulsar-app check` 0 errors）
- 当前状态：done（需求 + 技术方案 + 编码 + 验收全部完成）
- 交付核心：①busy 必然释放（ActiveRound RAII guard，Drop 自动 end()，无 TTL）；②注入点契约机制（IP-1~IP-5，语义化命名，失败策略梯度，`&mut RoundContext` 放权 + 局部产物第二 `&mut` 参数，边界只画在当前轮）；③既有调度收编：选型 → IP-1 上层注册 hook、课题路由/简报/打分 → IP-1、课题修订/验收/计数 → IP-5、压缩 → IP-2（Compactor 封装 + `project_history` 认 `summary_of` 跳过旧消息）；④账本扩展 `inject_point` 列（NULL 兼容，既有面板不破坏）

## Execution Log / 执行记录

- 1. 2026-08-22 17:17: 新迭代创建。背景：`conv_1787253076882845861` 轮询永久阻塞（busy 泄漏；hook-judgement-recovery lifecycle 记录的「RoundGuard 已修」在代码中不存在，属文档失真）；hook 概念被 `HookDef`（裁决调用静态表）窄化，压缩等「改写会话」型能力无法表达。多轮方案讨论收敛：注入点即类型（hook 能力边界由注入点规格卡写死）、核心流程仅 5 步、失败策略梯度（越靠前越硬、越靠后越软）。需求与契约设计已对齐，落盘 `requirements.md` + `technical-plan.md`。
- 2. 2026-08-22 17:25: 用户回答开放问题：Q2 确认（busy 周期覆盖全轮，是）；Q3 暂不做（stale TTL 移出范围，busy 修复仅 RAII guard；panic 由 Drop 栈展开兜底、进程崩溃由内存态重启清空兜底）；Q1（选型归属）已解释待用户澄清后确认。已同步回写 requirements.md / technical-plan.md。
- 3. 2026-08-22 17:45: Q1 多轮澄清收敛。过程：用户追问「课题还是神经元？」→「选型的结果是什么？」→「它本质上还是操作 msgs 不是吗，没他不行？」→「我不选型，对话进行下去？？」→「上层怎么对话，关底层构建什么事情？」→ 最终明确：「神经元选型本质是管理 msgs 里的 System / [当前角色] 提示词，还是在操作 msgs；不挂载它核心流程也能进行；助手/系统对话模式没它不行是业务层语义」。结论：选型 = 上层注册到 IP-1 的 hook（操作 msgs 的改写型），非必装、非核心流程；底层不感知对话模式。Q1-Q3 全部关闭，已同步回写三份文档。
- 4. 2026-08-22 17:50: 用户决策：hook 相关代码在 core 下新开 `hook/` 文件夹（参考 `core/neuron/` 组织）。已把文件布局写入 technical-plan.md：`core/hook/`（mod.rs + defs.rs 契约 + judgement.rs 原 hook.rs + store.rs 原 hook_judgement_store.rs + selection.rs 选型 + topic.rs 课题 + compaction.rs 压缩 + outcome.rs 打分）；Step 2/3/4 文件路径已同步。
- 5. 2026-08-22: 契约多轮纠偏收敛：①handler 参数改为直接丢核心流程真实数据（load_context 返回 `RoundContext` 就直接丢，删掉自创的 Ip1Ctx/RoundContextPatch 等包装类型）；②注入点命名语义化——`Ip1~Ip5` → `AfterLoadContext`/`AfterPersistInput`/`AfterCallModel`/`AfterExecuteTools`/`AfterPersistOutcome`（名字 = 核心步名 + 之后，简称 IP-1~IP-5 保留）；③用户定调「核心对 hook 尽可能放权」——每注入点丢当前轮完整 `&mut RoundContext`（上下文尽量给）、不设字段级权限、局部产物（`ModelCallResponse`/`Vec<ToolResultItem>`）就近作第二 `&mut` 参数（操作权限尽量给）、边界只画在当前轮（IP-5 产物已落库改回只读）。
- 6. 2026-08-22: 方案遗留决策点拍板（AskUserQuestion）：①mandatory 必装机制——**移除**（无实例，HookDef 精简为 id/label/inject_point/handler）；②guard 触发守卫——**移除**（是否执行由 handler 内部自行判断）；③压缩 hook——**本次做**（Compactor 封装为 IP-2 hook + `project_history` 认 `summary_of` 投影修正，AC5 纳入验收）。三份文档已同步。下一步唯一动作：用户批准 → Step 1 编码。
- 7. 2026-08-22 22:00 起: Step 1-6 编码执行。Step 1 `session_coordinator.rs` 引入 `ActiveRound` RAII guard（持有 session_id + token，Drop 自动 end()，`Arc::ptr_eq` 防误删语义保留）；Step 2 `core/hook/` 注入点契约（defs.rs `InjectPointId` const fn、`HookHandler` 5 变体 HRTB、`HookRegistry` Mutex+snapshot）；Step 3 核心流程收编——`conversation_runner` 删 `RoundHooks` 改持 `Arc<HookRegistry>`、`persist_input` 真相源边界改由 store 推导、选型（resolve+角色拼接+锚点写回）迁出为 IP-1 上层 hook、`assistant_session` 课题 hook（IP-1/IP-5）经 `install_hooks` 装配、gateway 装配注册 select-neuron + compaction hook；Step 4 账本 `hook_judgements` 加 `inject_point` 列（幂等迁移 + NULL 兼容 + 测试断言）；Step 5 压缩 hook——`core/hook/compaction.rs` 封装 Compactor（超阈值生成摘要替换本次 wire，ignore 策略兜底恢复原 messages），`project_history` 遇 Compaction 跳过 `summary_of` 覆盖的旧消息（新增测试 `project_history_skips_summarized_old_messages`），`providers::model_context_window` 支持查询窗口。
- 8. 2026-08-22 22:20: 回归修复——全量测试发现 5 个 gateway 测试失败（stream/agent_loop，ModelNotSelected）。根因：旧实现 Chat/Agent 模式传 `None` hooks（不跑课题逻辑），hook 全局注册后 `round_before`/`round_after` 未还原模式边界，Chat/Agent 也执行 match_topic → `ensure_system_neuron` 无系统神经元需 LLM 创建 → 无默认模型报错。修复：`round_before`/`round_after` 加 `ConversationMode::Assistant | System` 模式 gate（业务层语义，非核心流程）。`cargo test --lib` 376 passed / 0 failed；`pnpm --filter pulsar-app check` 0 errors / 20 warnings（既有）。

