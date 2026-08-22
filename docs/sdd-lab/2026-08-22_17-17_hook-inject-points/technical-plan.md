# Technical Plan / 技术方案: Hook 注入点契约与调度收编

## Requirement Baseline / 需求基线

- 对应需求文档：`docs/sdd-lab/2026-08-22_17-17_hook-inject-points/requirements.md`
- 需求确认状态：Q1/Q2/Q3 已确认（Q1 选型 = IP-1 上层注册 hook，非必装、非核心流程）
- 本方案覆盖范围：任务一（busy 必然释放，仅 RAII guard，无 TTL）+ 任务二（注入点契约 + 调度收编 + 账本扩展 + 可选压缩 hook）

## Current Project Facts / 当前项目事实

- 已读取文件/模块：
  - `packages/pulsar-app/src-tauri/src/core/conversation_runner.rs`（run_round / run_round_stream 编排，含约 13 处早退漏 end）
  - `packages/pulsar-app/src-tauri/src/core/session_coordinator.rs`（begin/end + 4 单测）
  - `packages/pulsar-app/src-tauri/src/core/hook.rs`（HookDef 静态清单 + call_judgement + hook_judgements 账本）
  - `packages/pulsar-app/src-tauri/src/core/assistant_session.rs`（AssistantHooks: before_round/after_round 编排）
  - `packages/pulsar-app/src-tauri/src/core/compactor.rs`、`model_call_input.rs`（project_history 不认 summary_of）
  - `docs/pulsar/architecture.md`、`docs/specs/2026-08-16_12-00_round-pipeline-split.md`（流水线权威描述）
- 当前实现事实：
  - `run_round` 编排：load_context → begin → before_round → resolve → write_state → append_input → persist_input → 占位 → call_model → 收敛 → execute_tools → persist_outcome → after_round → end。busy 依赖人工配对 begin/end，无 fail-safe。
  - `SessionCoordinator::begin(session_id, trigger) -> Option<Arc<CancellationToken>>`；`end` 用 `Arc::ptr_eq` 防误删；无 stale 兜底。
  - hook 概念被 `HookDef` 窄化为「裁决模型调用静态表」（system_type / label / response_format / neutral_fallback）；压缩等改写型能力无法表达。
  - `project_history` 不认 `summary_of`；`Compactor::ensure_fits` 只插摘要不裁剪 → 压缩对模型输入无效。
- 相关接口/数据结构：
  - `RoundTriggerKind`（User / Poller / ManualStep / AgentLoop）
  - `RoundContext`（session_id / seed / state / messages / model / mode / tool_override / model_input …）
  - `ModelResponse`（output / reasoning / tool_calls）、`RoundOutcome`（response / tool_calls / tool_results / reasoning / selected_neuron_id）
- 约束与风险：契约设计不参考既有 hook 实现；User 抢占与遇忙跳过语义不可破坏；`hook_judgements` 账本向后兼容。

## Open Questions / 开放问题

- [x] Q1 选型归属：用户确认——选型（resolver.resolve + write_session_state）= 上层注册到 IP-1 的 hook，非必装、非核心流程；不挂载核心流程仍可运行。
  - 触发来源：需求讨论（调度收编范围）
  - 无法确定的内容：选型是否属核心流程 / 是否必装
  - 影响范围：任务二 Step 3（选型封装为 IP-1 上层注册 hook；课题匹配 → IP-1、complete_scope/打分 → IP-5）
  - 候选处理：a) 必装 hook；b) 保留核心流程（均被否——选型是操作 msgs 的改写动作，业务层语义决定注册）
  - 用户回答/确认：选型本质是管理 msgs 的 System / [当前角色] 提示词，不挂载核心流程也能进行；「没它不行」仅业务层语义，底层不感知对话模式
  - 状态：已确认
- [x] Q2 busy 周期：begin 在 load_context 前，guard 覆盖全轮（含所有 IP hook），IP-5 后释放？—— 用户确认：是
- [x] Q3 TTL 语义：定为「整轮最大时长上限」而非 idle 超时，默认 30 分钟可配置？—— 用户确认：暂不做（stale TTL 移出范围，busy 修复仅 RAII guard；panic 由 Drop 栈展开兜底、进程崩溃由内存态重启清空兜底）

## Solution Options / 方案候选

### Option A / 方案 A：注入点契约 + 调度收编（选定）

- 推荐：是
- 方案摘要：契约优先——从核心流程 5 步上下文推导注入点 IP-1~IP-5 规格卡（能看见 / 能要求 / 失败策略）；busy 用 RAII guard 保证必然释放（Q3 暂不做 TTL）；既有调度按注入点封装为 hook（选型 → IP-1 上层注册、课题匹配 → IP-1、判定/打分 → IP-5）。
- 涉及模块：conversation_runner.rs / session_coordinator.rs / hook.rs / hook_judgement_store.rs / assistant_session.rs / resolver / assembler / executor
- 优点：概念还原（hook = 注入点，裁决 / 压缩都是实现）；能力边界类型安全（注入点即类型）；失败策略梯度自然；账本可观测扩展。
- 缺点：重构面大（runner 编排 + 调度迁移）。
- 风险：迁移期行为回归，依赖单测 + 人工验证兜底。

### Option B / 方案 B：仅修复 busy + 最小改动

- 推荐：否
- 方案摘要：只做任务一（guard + TTL），不做调度收编。
- 优点：改动小、止血快。
- 缺点：hook 概念窄化问题保留，压缩等能力无注入点可挂。
- 风险：后续仍要重构。

## Decision / 方案决策

- Selected / 选定方案：Option A（注入点契约 + 调度收编）
- Why / 选择原因：用户已在多轮讨论中确认「注入点即类型」「核心流程仅 5 步」「除核心流程外调度转 hook」；方案 A 一次性纠正概念边界，符合 K8s Admission / webpack plugin 等社区实践。
- Decision Owner / 决策人：（等待用户决策）
- Decision Time / 决策时间：
- Open Questions 状态：Q1/Q2/Q3 已关闭（Q1 选型 = IP-1 上层注册 hook；Q2 busy 周期覆盖全轮；Q3 TTL 暂不做）

## API Design / API 设计

### Contract Scope / 契约范围

- 变更类型：新增 + 扩展（无破坏性变更）
- 消费方：conversation_runner / assistant_session / gateway 装配 / 前端 RPC 消费方（账本扩展字段兼容）
- 真相源文件：`core/hook/defs.rs`（契约：InjectPoint 规格卡 + HookDef + HookRegistry）、`core/hook/judgement.rs`（裁决调用）、`core/hook/store.rs`（账本）、`core/session_coordinator.rs`（busy）

### HookDef（注册契约）

「注入点即类型」落到 Rust：**handler 签名由注入点决定**。放权原则——核心对 hook 尽可能放权：
- **上下文尽量给**：每个注入点都丢**当前轮完整上下文 `&mut RoundContext`**（load_context 返回值本体），不丢局部切片；hook 一眼看全 messages / state / seed / model / trigger / topic / reselect。
- **操作权限尽量给**：能 `&mut` 就 `&mut`，不设字段级精细权限；call_model / execute_tools 的局部产物（`ModelCallResponse`、`Vec<ToolResultItem>`）不在 RoundContext 里，就近作第二 `&mut` 参数追加。
- **边界只画在当前轮**：上下文只给 `RoundContext`（不跨会话、不跨轮、不给全局）；跨轮/跨库动作（落账本、写 state_store）由 hook 自捕获依赖执行，核心只负责把当前轮数据递到位。

#### 1. 注入点与 handler（丢当前轮完整上下文 + 就近局部产物）

```rust
/// 注入点：名字 = 「核心流程第几步之后」，读者一眼看懂挂在哪。
/// 文档讨论时可用简称 IP-1~IP-5（对应顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InjectPointId {
    /// 核心步① load_context 之后（IP-1）
    AfterLoadContext,
    /// 核心步② assemble + persist_input 之后（IP-2）
    AfterPersistInput,
    /// 核心步③ call_model 之后（IP-3）
    AfterCallModel,
    /// 核心步④ execute_tools 之后（IP-4）
    AfterExecuteTools,
    /// 核心步⑤ persist_outcome 之后（IP-5）
    AfterPersistOutcome,
}

/// 每个变体对应一个注入点；第一参 = 当前轮完整上下文（&mut 可改 / & 只读），后续参 = 就近局部产物。
/// handler 返回 `BoxFuture`（async）：核心流程 `run_round` 本身是 async，注入点分发天然在
/// async 上下文——既有调度（选型 / 课题匹配 / 判定 / 打分）全部含 async 模型调用，
/// 同步 handler 无法承载；`run_*` 分发器同步内部快照 hook 列表后逐个 `.await`（不跨 await 持锁）。
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub enum HookHandler {
    /// AfterLoadContext：整轮上下文全量可改（选型在此改 messages / state）。
    AfterLoadContext(Box<dyn Fn(&mut RoundContext) -> BoxFuture<'_, AppResult<()>> + Send + Sync>),
    /// AfterPersistInput：wire 已落库，改 ctx.messages 只影响本次发送、不动真相源。
    AfterPersistInput(Box<dyn Fn(&mut RoundContext) -> BoxFuture<'_, AppResult<()>> + Send + Sync>),
    /// AfterCallModel：追加 call_model 返回值，可改写响应 / 拦截工具调用。
    AfterCallModel(
        Box<dyn Fn(&mut RoundContext, &mut ModelCallResponse) -> BoxFuture<'_, AppResult<()>> + Send + Sync>,
    ),
    /// AfterExecuteTools：追加 execute_tools 产出的工具结果，可改写 / 丢弃。
    AfterExecuteTools(
        Box<dyn Fn(&mut RoundContext, &mut Vec<ToolResultItem>) -> BoxFuture<'_, AppResult<()>> + Send + Sync>,
    ),
    /// AfterPersistOutcome：产物已落库，只读整轮上下文；落账本等副作用由 hook 自办。
    AfterPersistOutcome(Box<dyn Fn(&RoundContext) -> BoxFuture<'_, AppResult<()>> + Send + Sync>),
}
```

| 注入点 | 位置 | 丢给 hook | 失败策略 |
|---|---|---|---|
| `AfterLoadContext`（IP-1） | 核心步① load_context 后 | `&mut RoundContext` | **fail**：Err 中止本轮（最早点未落库） |
| `AfterPersistInput`（IP-2） | 核心步② persist_input 后、call_model 前 | `&mut RoundContext`（改 messages 仅影响发送） | **ignore**：Err 按原 wire 发送 |
| `AfterCallModel`（IP-3） | 核心步③ call_model 后、execute_tools 前 | `&mut RoundContext` + `&mut ModelCallResponse` | **ignore** |
| `AfterExecuteTools`（IP-4） | 核心步④ execute_tools 后、persist_outcome 前 | `&mut RoundContext` + `&mut Vec<ToolResultItem>` | **ignore** |
| `AfterPersistOutcome`（IP-5） | 核心步⑤ persist_outcome 后 | `&RoundContext`（只读，副作用自办） | **ignore**（产物已入库） |

失败策略梯度：数据一旦入库（persist_input 后），中止会丢轮次产物——越靠前越硬（IP-1=fail）、越靠后越软（IP-2~IP-5=ignore）。

#### 2. HookDef（注册单元）

```rust
pub struct HookDef {
    pub id: &'static str,            // 唯一标识（重复注册拒绝）
    pub label: &'static str,         // 可观测 label（账本/日志）
    pub inject_point: InjectPointId, // 挂载点：决定 handler 变体 + 失败策略
    pub handler: HookHandler,        // 是否执行由 handler 内部自行判断（无独立 guard）
}
```

#### 3. HookRegistry（注册与执行）

```rust
pub struct HookRegistry {
    hooks: HashMap<InjectPointId, Vec<RegisteredHook>>, // 组内按注册顺序
}

impl HookRegistry {
    pub fn new() -> Self;

    /// 注册：同 id 重复 → Err。
    pub fn register(&mut self, def: HookDef) -> Result<(), RegisterError>;

    /// 卸载：按 id 移除。
    pub fn unregister(&mut self, id: &str) -> Result<(), UnregisterError>;

    /// 按注入点执行：组内按注册顺序逐个调用（async；内部快照 hook 列表，不跨 await 持锁）。
    /// AfterLoadContext 走 fail 策略（Err 上抛中止本轮）；其余走 ignore 策略（Err 记 warn，继续/用默认值）。
    pub async fn run_after_load_context(&self, ctx: &mut RoundContext) -> AppResult<()>;
    pub async fn run_after_persist_input(&self, ctx: &mut RoundContext); // 内部吞 Err 记 warn
    pub async fn run_after_call_model(&self, ctx: &mut RoundContext, response: &mut ModelCallResponse);
    pub async fn run_after_execute_tools(&self, ctx: &mut RoundContext, results: &mut Vec<ToolResultItem>);
    pub async fn run_after_persist_outcome(&self, ctx: &RoundContext);
}
```

#### 4. 链式传值语义（同注入点多 hook）

- 直接 `&mut` 当前轮上下文即链式：runner 按注册顺序调用，后注册 hook 自然看到前注册 hook 的修改，可继续改写。
- 各注入点 runner 侧接线（全部传 `ctx`，局部产物就近追加）：
  - AfterLoadContext（IP-1）：传 `ctx`（load_context 返回值本体），改后继续走 assemble；
  - AfterPersistInput（IP-2）：传 `ctx`（wire 已落库，hook 改 `ctx.messages` 仅影响 call_model 的输入投影）；
  - AfterCallModel（IP-3）：传 `ctx` + `model_response`（改后 execute_tools 消费改后的声明）；
  - AfterExecuteTools（IP-4）：传 `ctx` + `outcome.tool_results`（改后按新列表落 ToolResult 消息）；
  - AfterPersistOutcome（IP-5）：传 `&ctx`（产物已落库，hook 自落账本 / 触发外部动作）。

#### 5. 注册时机与调用方

- 所有 hook 均由上层在装配期 `register`（选型、课题匹配、压缩、打分等）；业务层语义由上层注入，runner 不感知。
- `HookRegistry` 由 runner 持有（`conversation_runner` 构造时传入装配好的 registry）。

### SessionCoordinator（busy 必然释放）

- `begin(session_id, trigger) -> ActiveRound`（RAII guard：持 `Arc<Self>` + session_id + token）
- `impl Drop for ActiveRound`：同步调用 end()（end 为非异步）
- `end()`：`Arc::ptr_eq` 防误删语义不变
- `ActiveRound::cancelled()`：供 run_round 的 select 取消分支使用
- stale TTL 不在本次范围（Q3 暂不做）

### hook_judgements 账本（扩展）

- 新增字段：`inject_point`（Text，NULL 兼容既有记录）
- 既有字段不变；`hook_judgements_list` 返回自动携带新字段（前端可选展示）

### 文件布局（core/hook/ 新目录）

hook 相关代码收拢到 `core/hook/`（参考 `core/neuron/` 模块组织）：

```
core/hook/
├── mod.rs          # 模块导出
├── defs.rs         # 契约：InjectPoint 规格卡 + HookDef + HookRegistry
├── judgement.rs    # 裁决调用：原 core/hook.rs（HookDef 静态清单 + call_judgement + 状态/锚点）
├── store.rs        # 账本：原 core/hook_judgement_store.rs（hook_judgements 表）
├── selection.rs    # 选型 hook（IP-1，上层注册，封装 round_resolver）
├── topic.rs        # 课题匹配/切换 hook（IP-1，原 before_round 逻辑）
├── compaction.rs   # 压缩 hook（IP-2，封装 Compactor，含 project_history 投影修正）
└── outcome.rs      # complete_scope / 打分 hook（IP-5，原 after_round 逻辑）
```

- 原 `core/hook.rs` → `core/hook/judgement.rs`（保留契约与 `call_judgement`，裁决调用是 hook 的实现手段）
- 原 `core/hook_judgement_store.rs` → `core/hook/store.rs`
- `round_resolver.rs` 留在 core 根（选型业务实现，被 selection hook 封装）
- `conversation_runner.rs` / `assistant_session.rs` 留在 core 根（核心流程 + 装配注册）

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：用户批准本方案；决策已确认（Q1 选型 = IP-1 上层注册 hook；Q2 busy 覆盖全轮；Q3 不做 TTL；mandatory/guard 机制移除；压缩 hook 本次做）
- 若执行前需求、API、范围或交互规则变化：先回写 requirements.md / technical-plan.md

### Step 1. 任务一：busy 必然释放

#### 文件：`packages/pulsar-app/src-tauri/src/core/session_coordinator.rs`

- 改动类型：修改
- 改动内容：
  - `begin()` 返回 `ActiveRound`（新 struct，持 `Arc<Self>` + session_id + token；`Drop` 同步 end；`cancelled()` 透传）
  - `end()` 的 `Arc::ptr_eq` 防误删语义不变
  - stale TTL 不在本次范围
- 设计约束：
  - API：遵循本方案 API Design
- 验收点：单测——「begin 后仅 drop guard → 会话不再 busy」「end ptr_eq 语义不变」；既有 4 个单测保持

#### 文件：`packages/pulsar-app/src-tauri/src/core/conversation_runner.rs`

- 改动类型：修改
- 改动内容：run_round / run_round_stream 用 `let _guard = self.coordinator.begin(...)?` 替代手动 begin/end；删除 13 处早退路径与 select 取消分支的手动 end（guard Drop 自动释放）
- 验收点：`cargo test --lib` 全绿；轮询会话不再永久阻塞

### Step 2. 任务二契约：注入点机制

#### 文件：`packages/pulsar-app/src-tauri/src/core/hook/defs.rs`（新，随目录 `core/hook/` 一并创建）

- 改动类型：新增
- 改动内容：`InjectPointId` 枚举 + `InjectPoint` 规格卡表（IP-1~IP-5）+ `HookRegistry`（按注入点分组、组内注册顺序、链式传值）
- 验收点：注册 / 顺序 / 链式单测

#### 文件：`packages/pulsar-app/src-tauri/src/core/conversation_runner.rs`

- 改动类型：修改
- 改动内容：run_round 编排改为核心 5 步 + 5 处注入点分发（IP-1~IP-5）；busy guard 覆盖全轮
- 验收点：核心流程不变量保持（进 wire 必落库、先落库再调模型）

### Step 3. 任务二封装：调度迁移

#### 文件：`packages/pulsar-app/src-tauri/src/core/hook/selection.rs`（新）

- 改动类型：新增
- 改动内容：选型 hook（IP-1，上层注册）：封装 resolver.resolve + write_session_state；操作 msgs（管理 System / [当前角色] RoleContext 提示词），非必装、非核心流程

#### 文件：`packages/pulsar-app/src-tauri/src/core/hook/topic.rs`（新）+ `core/hook/outcome.rs`（新）

- 改动类型：新增
- 改动内容：课题匹配/切换 hook（IP-1，原 before_round 逻辑）+ complete_scope / 打分 hook（IP-5，原 after_round 逻辑）

#### 文件：`packages/pulsar-app/src-tauri/src/core/assistant_session.rs`

- 改动类型：修改
- 改动内容：`RoundHooks` trait 退役；改为在装配期向 `HookRegistry` 注册上述 hook（选型 / 课题 / 打分）
- 验收点：行为等价（选型 / 切换 / 判定 / 打分时序不变）

### Step 4. 账本扩展

#### 文件：`packages/pulsar-app/src-tauri/src/core/hook/store.rs` + `net/rpc.rs` + `lib.rs`

- 改动类型：扩展
- 改动内容：hook_judgements 加 `inject_point` 字段（NULL 兼容）；`hook_judgements_list` 透传
- 验收点：既有面板不破坏；新字段可见

### Step 5. 压缩 hook（IP-2）

#### 文件：`packages/pulsar-app/src-tauri/src/core/hook/compaction.rs`（新）

- 改动类型：新增
- 改动内容：Compactor 封装为 AfterPersistInput hook——handler 内部估算 token，超阈值则替换 `ctx.messages`（提交压缩后的 wire；已落库，只影响本次发送）
- 验收点：超长会话（1.09M token）经压缩可完成轮询，不再 400

#### 文件：`packages/pulsar-app/src-tauri/src/core/model_call_input.rs` / `compactor.rs`

- 改动类型：修改
- 改动内容：`project_history` 遇 Compaction 跳过 `summary_of` 覆盖的旧消息（压缩对模型输入真正生效）
- 验收点：压缩后模型输入 token 显著下降

### Step N. 检查与回写

#### 命令

- 运行：`cargo check --lib`、`cargo test --lib`、`pnpm --filter pulsar-app check`
- 修复：按报错先回写方案，再改代码

#### 文件：`docs/sdd-lab/2026-08-22_17-17_hook-inject-points/lifecycle.md`

- 回写执行记录、实际改动摘要、验证结果、下一步状态

## Risk And Mitigation / 风险与缓解

- 风险：调度迁移行为回归（选型 / 切换 / 判定时序变化）
  - 缓解方式：Step 3 保持行为等价 + 既有 364 测试兜底 + 人工验证轮询恢复
- 风险：契约落地期间 hook_judgements 兼容
  - 缓解方式：扩展字段 NULL 兼容，不重建表

## Execute Checkpoint / 执行检查点

- 当前理解：契约优先（注入点规格卡从核心流程上下文推导，不参考既有 hook 代码），再封装既有功能；busy 必然释放为前置止血（仅 RAII guard，TTL 暂不做）
- 核心目标：①busy 不泄漏可证明；②runner 只剩核心 5 步，调度全部 hook 化
- 下一步动作：用户批准 technical-plan → Step 1 编码
- 风险：迁移回归，单测 + 人工验证兜底
