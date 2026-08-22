# Requirements / 需求文档: Hook 注入点契约与调度收编

## Restated Understanding / 需求复述

- 我理解当前需求是：
  1. 任务一：会话轮「必然释放」——busy 泄漏修复。`begin()` 返回 RAII guard，`Drop` 自动 `end()`，所有早退路径自动释放；`end()` 的 ptr_eq 防误删语义不变。（stale TTL 暂不做）
  2. 任务二：除核心流程（load_context / assemble+persist_input / call_model / execute_tools / persist_outcome）以外的所有调度统一转为注入点 hook。**先约定 hook 契约（注入点规格卡），再从核心流程上下文出发封装既有功能**；契约不参考既有 hook 代码。
- 当前核心目标是：让 `run_round` 只保留核心 5 步编排 + 注入点分发，其余调度（选型、课题匹配、判定、打分、未来的压缩）以 hook 形式可插拔；busy 语义可证明不泄漏。
- 当前边界是：注入点即类型（hook 的能力边界由注入点规格卡写死，无独立 kind 分类）；busy 周期覆盖全轮（含所有 hook 执行）。
- 暂不处理：IP-3/IP-4 不造 hook（保留空注入点，规格卡已定义）；流式占位落库属 call_model 内部实现细节，不转 hook；mandatory/guard 机制不做（无实例，需要时再引入）。

## Scope / 范围

- In:
  - 任务一：`SessionCoordinator::begin` 返回 RAII guard；`run_round` / `run_round_stream` 全部早退路径自动释放（含 select 取消分支）。stale TTL 不在本次范围。
  - 任务二（契约）：注入点 IP-1~IP-5 规格卡（能看见 / 能要求 / 失败策略）；`HookRegistry` 注册与顺序执行（前一 hook 产出可链式传给后一）；busy 周期定义。
    - 注入点命名即语义（挂在核心流程第几步之后）：`AfterLoadContext`（IP-1）/ `AfterPersistInput`（IP-2）/ `AfterCallModel`（IP-3）/ `AfterExecuteTools`（IP-4）/ `AfterPersistOutcome`（IP-5）。
    - 放权原则：上下文尽量给（每注入点丢当前轮完整 `&mut RoundContext`）、操作权限尽量给（不设字段级权限，局部产物就近作第二 `&mut` 参数）、边界只画在当前轮。
  - 任务二（封装）：选型（resolver.resolve + write_session_state）→ IP-1 上层注册 hook（本质是操作 msgs：管理 System / [当前角色] RoleContext 提示词；不挂载核心流程仍可运行，「没它不行」仅业务层语义）；既有 before_round 逻辑（课题匹配/切换）→ IP-1；既有 after_round 逻辑（complete_scope / 打分）→ IP-5；`RoundHooks` trait 退役。
  - 账本：`hook_judgements` 扩展 `inject_point` 维度（NULL 兼容，不破坏性重建）。
  - 压缩 hook：Compactor 封装为 IP-2 hook（handler 内部估算 token，超阈值替换发送 wire）+ `project_history` 认 `summary_of` 投影修正。
- Out:
  - `hook_judgements` 表结构不做破坏性重建；RPC / 前端消费方不受破坏性影响。
  - 不新增 UI / 前端改动。
  - 不改变会话并发策略（维持 B 方案会话级串行：User 抢占 / 非 User 遇忙跳过）。

## User Interaction / 用户交互

- 触发入口：无新增用户入口（后端架构重构）。
- 用户操作路径：无变化。
- 系统反馈：无变化。
- 状态变化：轮询/对话不再出现永久阻塞；hook 判定面板可看到 `inject_point` 维度（后端扩展，前端兼容）。
- 异常/边界交互：超长会话（如 1.09M token）经压缩后可恢复轮询。
- 不应发生的交互：hook 早退遗留 busy 导致后续轮询永久跳过。

## Acceptance Criteria / 验收标准

- [ ] AC1：begin 后任何早退（`?` 错误 / select 取消 / 直接 return）都不遗留 busy 状态；单测覆盖「begin 后仅 drop guard → 会话不再 busy」。
- [ ] AC2：`run_round` 只剩核心 5 步编排 + 注入点分发；选型 / 课题匹配 / 判定 / 打分不再硬编码在 runner。
- [ ] AC3：同一注入点多 hook 按注册顺序执行，前一 hook 产出可链式传给后一。
- [ ] AC4：既有测试全部保持通过（后端 364 测试 + 前端 svelte-check 0 errors）。
- [ ] AC5：压缩 hook 挂 IP-2 后，超长会话经压缩可正常完成轮询，不再 400；`project_history` 认 `summary_of`（压缩对模型输入生效）。

## Constraints / 约束

- 业务约束：User 轮抢占语义、非 User 轮遇忙跳过语义不变；核心流程「进 wire 必落库、先落库再调模型」不变量保持。
- 技术约束：契约设计不参考既有 hook 代码（从核心流程上下文推导）；注入点即类型（无独立 kind 分类）；`end()` 的 ptr_eq 防误删语义不变。
- 时间/兼容性约束：`hook_judgements` 账本扩展而非重建；RPC/前端兼容。

## Open Questions / 开放问题

- [x] Q1 选型归属：用户确认——选型 = 上层注册到 IP-1 的 hook（操作 msgs 的改写型：管理 System / [当前角色] RoleContext 提示词），非必装、非核心流程；不挂载它核心流程仍可运行，「没它不行」仅业务层语义。
  - 澄清过程：用户先后追问「课题还是神经元？」「选型的结果是什么？」「它本质上还是操作 msgs 不是吗，没他不行？」「我不选型，对话进行不下去？？」「上层怎么对话，关底层构建什么事情？」——收敛结论：选型 = neuron 选择（非 topic），本质是操作 msgs 的改写动作；底层不感知对话模式；业务层决定是否注册。
- [x] Q2 busy 周期：begin 在 load_context 前，guard 覆盖全轮（含所有 IP hook），IP-5 后释放？—— 用户确认：是
- [x] Q3 TTL 语义：定为「整轮最大时长上限」而非 idle 超时，默认 30 分钟可配置？—— 用户确认：暂不做（stale TTL 从范围移除，busy 修复仅保留 RAII guard 1a）

## Requirement Decisions / 需求决策

- 2026-08-22 17:17:
  - 决策：按「注入点即类型」设计契约；核心流程仅 5 步；失败策略梯度（越靠前越硬、越靠后越软——数据一旦入库，中止会丢轮次产物）。
  - 原因：多轮方案讨论收敛；K8s Admission（mutating/validating 由阶段决定）、webpack/Vite plugin（hook 挂点决定能力）等社区实践验证。
- 2026-08-22 17:25:
  - 决策：Q2 确认（busy 周期覆盖全轮）；Q3 暂不做（stale TTL 移出范围，busy 修复仅 RAII guard，panic 由 Drop 栈展开兜底、进程崩溃由内存态重启清空兜底）；Q1 待用户澄清后确认。
  - 原因：用户逐一回答开放问题。
- 2026-08-22 17:45:
  - 决策：Q1 关闭——选型（resolver.resolve + write_session_state）= IP-1 上层注册 hook，非必装、非核心流程；不挂载核心流程仍可运行。
  - 原因：用户确认「神经元选型本质是管理 msgs 里的 System / [当前角色] 提示词，还是在操作 msgs；不挂载它核心流程也能进行；助手/系统对话模式没它不行是业务层语义」——底层不感知对话模式，业务层决定注册。
- 2026-08-22:
  - 决策：①契约放权——每个注入点丢当前轮完整 `&mut RoundContext`（上下文尽量给）、不设字段级权限、局部产物（`ModelCallResponse`/`Vec<ToolResultItem>`）就近作第二 `&mut` 参数（操作权限尽量给），边界只画在当前轮；②mandatory/guard 机制移除（无实例）；③压缩 hook 本次做（AC5 纳入验收）。
  - 原因：用户确认「核心对 hook 尽可能放权，能给上下文尽量给、操作权限尽量给；但要局限于当前轮对话，其它内容放开没有意义」；三个决策点经 AskUserQuestion 逐项拍板。
