# Technical Plan / 技术方案: Hook 裁决纠偏与透明化

## Requirement Baseline / 需求基线

- 对应需求文档：`docs/sdd-lab/2026-08-22_11-10_hook-judgement-recovery/requirements.md`
- 需求确认状态：已确认（用户选定 A+B+C 组合、面板形态、消息卡形态）
- 本方案覆盖范围：
  - hook 级统一纠偏（挂统一入口 `call_judgement`，不区分 system_type）
  - 裁决全量持久化（新表 `hook_judgements`）
  - 独立全局「Hook 判定」sidebar view（与「会话/文件」同级）
  - 主消息列表内联裁决卡（锚点消息附属渲染块）
  - 数据通道（`hook_judgements_list` / `hook_defs_list` command + 两阶段事件推送 + i18n）

## Current Project Facts / 当前项目事实

- 已读取文件/模块：
  - `src-tauri/src/core/assistant_session.rs`
  - `src-tauri/src/core/neuron/manager.rs`（`default_behavior_for_system_type` L50-63）
  - `src-tauri/src/core/neuron/model.rs`（`extract_json_object` L70）
  - `src-tauri/src/core/neuron/creation.rs`（`ensure_system_neuron` 幂等）
  - `src-tauri/src/core/openai_compat.rs`（`extra` 透传 `response_format`）
  - `src-tauri/src/core/topic_store.rs`（存储范式参考）
  - `src-tauri/src/core/events.rs`（`StateChange` 枚举）
  - `src/lib/layout/views.ts`、`src/lib/layout/layoutTypes.ts`
  - `packages/pulsar-app/.pulsar/logs/pulsar.log`（事故证据）
- 当前实现事实：
  - `call_judgement`（[assistant_session.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L98-L157)）：`ensure_system_neuron` → `run_raw_round`（`SessionSeed::Neuron` + 空标签工具 + `ThinkingConfig{enabled:false}`）→ `extract_json_object`；**解析失败仅 warn 留痕 500 字符 `response_preview` 后 `?` 上抛**。
  - 4 个 hook 调用点：`score_feedback_hook`（L735-761，**已有 fail-soft 先例**：`Err => warn + return Ok(())`）、`match_topic`（L790）、`revise_topic`（L955）、`complete_scope_hook`（L1120-1134，`.await?` 硬上抛）。
  - 裁决神经元固定：4 个 system_type 常量 → `ensure_system_neuron` 幂等获取 → `SelectionPolicy::Fixed`，不经过候选池 LLM 选择。
  - 模型跑偏根因（4 叠加）：flash 低档模型 JSON 遵循弱；神经元 role content 与 insert「只返回 JSON」指令冲突；payload 大 JSON 带偏；无 `response_format` 约束。
  - `score_feedback_hook` 是项目内「失败降级」既有先例，本次将其泛化到统一入口。
- 相关接口/数据结构：
  - `run_raw_round` 走 `ConversationRunner`（无 `coordinator.begin`），裁决调用天然可重试。
  - `openai_compat.rs` L136-139：`response_format` 经 `extra` 扁平透传，协议层**零改动**即可下发。
  - `topic_store.rs` 范式：`conn: Arc<Mutex<Connection>>` + `on_change: Option<StateEmitter>` + `init_table`（含迁移）+ `emit_change` 统一广播。
  - `events.rs` `StateChange`：现有 `Topics` / `Conversations{affected}` / `MessageDelta{...}` / `Poller` / `Sessions` / `Neurons` / `Tools` / `Providers` / `Workspaces` / `Git` / `GitConfirm`；`StateEmitter = Arc<dyn Fn(StateChange) + Send + Sync>`。
  - `views.ts` `viewRegistry`：`{id, title(i18n key), icon, component, movableTo}`；sidebar 默认 `["sessions","files","topics","tools"]`（layoutTypes.ts）。
  - 事故链路证据（pulsar.log）：`judgement JSON parse failed` → `send_model_message_stream failed error_code="invalid_input"`。
- 约束与风险：
  - 不引入新外部依赖；不改变主对话管道；裁决记录不混入消息数组。
  - 裁决调用禁工具、显式关闭深度思考（保持现有语义）。

## Open Questions / 开放问题

- [x] Q1 是否重跑历史裁决：不重跑（需求已定，面板仅查看）。
- [x] Q2 重试次数上限：固定 1 次带失败反馈（B 方案）；重试超时无独立机制（裁决轮天然短）。
- [x] Q3 能力探测结果缓存：按 provider/model 内存缓存（进程内），provider/model 切换时重探测。
- [x] Q4 消息卡与虚拟滚动冲突：裁决卡为锚点消息**附属渲染块**（独立于消息数组的旁路列表），不影响 `message_index`。
- 状态：全部关闭。

## Solution Options / 方案候选

### Option A / 方案 A：失败中性降级

- 推荐：是（入选）
- 方案摘要：`call_judgement` 解析失败不再 `?` 上抛，改为返回中性默认值 + `tracing::warn` 留痕；4 个 hook 各自消费中性语义。泛化 `score_feedback_hook` 既有先例到统一入口。
- 涉及模块：`assistant_session.rs`（`call_judgement` + 4 个 hook 调用点）
- 优点：主轮次永不因裁决失败中断；实现直接；已有先例可循。
- 缺点：失败时裁决副作用被跳过（语义上接受，中性默认值保证流程连续）。
- 风险：低。中性默认值语义须按 hook 明确（complete_scope 空判定 / match_topic 不创建不切换 / revise_topic 不修订 / score_feedback 跳过打分）。

### Option B / 方案 B：有限重试（带反馈自愈）

- 推荐：是（入选）
- 方案摘要：`extract_json_object` 解析失败时，重试 1 次；重试轮 user payload 追加失败反馈（原输出 + 「仅返回 JSON」纠偏指令），仍失败才进入 A 降级。
- 涉及模块：`assistant_session.rs`（`call_judgement` 内部重试逻辑）
- 优点：偶发跑偏可自愈，减少降级次数；重试轮复用 `run_raw_round`（无 coordinator 锁，天然安全）。
- 缺点：多一次模型调用延迟（约 1 次裁决耗时）。
- 风险：低。重试次数固定 1，不会指数放大。

### Option C / 方案 C：结构化输出预防

- 推荐：是（入选）
- 方案摘要：裁决调用下发 `response_format`（经现有 `extra` 扁平透传，协议层零改动）；能力探测降级链：`json_schema`（4 个 hook 各自 JSON Schema 契约）→ 不支持则 `json_object` → 再不支持无约束；探测结果按 provider/model 内存缓存。
- 涉及模块：`assistant_session.rs`（构建 `response_format`）、`openai_compat.rs`（验证 `extra` 透传，预计零改动）、`neuron/model.rs`（Schema 常量）
- 优点：源头预防格式错误；协议层已支持，成本低；按 provider/model 缓存避免重复探测。
- 缺点：需维护 4 份 JSON Schema；网关不支持时静默降级。
- 风险：中低。能力探测需覆盖 openai_compat 网关行为（新增测试）。

### Option D / 方案 D：解析修复管线（Re-Ask 修复器）

- 推荐：否
- 方案摘要：解析失败后单独起一轮「修复轮」，将散文输出 + 原 schema 交给模型重新输出 JSON。
- 优点：修复能力强于简单重试。
- 缺点：多一轮独立模型调用、成本更高；与 B 重叠（B 已带反馈重试）。
- 风险：功能边界与 B 重合，维护成本高；用户未选。

### Option E / 方案 E：超时隔离 / 异步化

- 推荐：否
- 方案摘要：裁决调用改异步执行，带超时与队列隔离，不阻塞主轮次。
- 优点：彻底不阻塞。
- 缺点：裁决结果无法同步进 hook 副作用（complete_scope 需要同步判定结果）；架构改动大。
- 风险：与 hook 同步语义冲突；用户明确暂不处理。

## Decision / 方案决策

- Selected / 选定方案：**A（失败降级）+ B（有限重试 1 次）+ C（结构化输出）组合**
- Why / 选择原因：
  - C 源头预防格式错误；B 偶发跑偏带反馈自愈；A 兜底保证主轮次不中断，三者互补形成「预防 → 自愈 → 兜底」闭环。
  - 复用 `run_raw_round` 无 coordinator 锁的天然重试安全；`score_feedback_hook` 降级先例可泛化。
  - D 与 B 功能重叠，E 与 hook 同步语义冲突且成本高，均不选。
- 组织方式决策（补充）：hook 概念收拢为 `HookDef` 静态清单（策略 5，而非就近常量 / 独立注册表 / 外部文件 / 类型生成）：
  - `response_format` 是每个 hook 自带的属性，schema 就近定义，随 hook 走
  - `call_judgement` 签名收 `&HookDef`，不查全局表、不感知 system_type
  - 中性降级语义（`neutral_fallback`）与展示名（`label`）一并收拢，新增 hook = 表加一行 + 一个函数
- Decision Owner / 决策人：user（已确认）
- Decision Time / 决策时间：2026-08-22
- Open Questions 状态：全部关闭

## API Design / API 设计

### Contract Scope / 契约范围

- 变更类型：新增 + 扩展
  - 新增：`hook_judgements` 表、`StateChange::HookJudgements` 变体、`hook_judgements_list` / `hook_defs_list` command、`views.hookJudgements` 与状态/字段 i18n key
  - 扩展：`call_judgement` 签名（`system_type: &str` → `def: &HookDef` + `anchor: JudgementAnchor`）、`run_raw_round` 新增 `response_format` 参数、`StateChange` 枚举扩展
- 消费方：RPC/命令层 → 前端「Hook 判定」面板与消息裁决卡；事件系统 → 前端实时刷新
- 真相源文件：`src-tauri/src/core/hook_judgement_store.rs`（新建）、`src-tauri/src/core/events.rs`、`src-tauri/src/core/commands.rs`（或对应 command 模块）、`src/lib/layout/views.ts`、`src/lib/i18n/*`

### `hook_judgements` 表

| 字段 | 类型 | 含义与约束 |
| ---- | ---- | ---- |
| `id` | TEXT PK | 记录 id（`hj_<timestamp>_<seq>` 或 uuid） |
| `session_id` | TEXT | 会话 id（可空，无会话态裁决如启动时为空） |
| `conversation_id` | TEXT | 锚点会话 id |
| `anchor_message_index` | INTEGER | 锚点消息索引（裁决卡挂载位置；未绑定消息时 NULL） |
| `hook_type` | TEXT | system_type：`assistant_complete_scope` 等 4 类 |
| `status` | TEXT | `pending` / `ok` / `retried_ok` / `downgraded`（三态终态；降级原因见 `error`） |
| `attempts` | INTEGER | 尝试次数（1 或 2） |
| `attempts_detail` | TEXT | 每轮尝试明细（JSON 数组：`[{attempt, raw, error}]`，重试两轮原文均全量保留） |
| `payload` | TEXT | 用户侧裁决输入（JSON 序列化） |
| `raw_response` | TEXT | 最终轮模型原始输出（全文保留） |
| `decision` | TEXT | 解析出的 JSON 决策（成功时） |
| `error` | TEXT | 失败/降级原因摘要（如 `LLM response missing JSON object`） |
| `duration_ms` | INTEGER | 总耗时（含重试） |
| `model_provider` | TEXT | 调用模型 provider |
| `model_id` | TEXT | 调用模型 id |
| `created_at` | INTEGER | 开始时间戳（ms） |
| `updated_at` | INTEGER | 结束时间戳（ms） |

索引：`(conversation_id, anchor_message_index)`、`(hook_type, status)`、`(created_at)`。

### `StateChange::HookJudgements`

- 事件变体（两阶段，锚点驱动）：
  - 开始：`HookJudgements { conversation_id, anchor_message_index, id, status: "pending" }` → 前端就地渲染「裁决中」卡
  - 结束：`HookJudgements { conversation_id, anchor_message_index, id, status: <终态> }` → 前端原地收敛为终态
- 事件源收敛于 `HookJudgementStore::emit_change`（对齐 `topic_store` 单点广播）。

### `hook_judgements_list(filters)` command

- 参数：`filters { hook_type?, status?, conversation_id?, limit?, offset? }`
- 返回：`Vec<HookJudgementRecord>`（按 `created_at` 倒序）
- 配套：RPC 分发（与既有 `topics_list` 同模式），供面板列表与消息卡锚点查询共用。

### `hook_defs_list()` command

- 参数：无
- 返回：`Vec<HookDefMeta>`（`{ system_type, label }`），由 `HOOK_DEFS` 静态表生成
- 用途：面板过滤下拉选项与记录展示名（label）的数据源——前端不感知 Rust 静态表

### `call_judgement` 返回契约与调用锚点

- 返回类型：`call_judgement` 改为返回 `AppResult<JudgementOutcome>`：

```rust
pub enum JudgementStatus { Ok, RetriedOk, Downgraded }

pub struct AttemptRecord {           // 每轮尝试明细（全量保留）
    pub attempt: u32,                // 1 = 首轮，2 = 重试轮
    pub raw: String,                 // 该轮模型原始输出（全文）
    pub error: Option<String>,       // 该轮解析失败原因
}

pub struct JudgementOutcome {
    pub status: JudgementStatus,
    pub decision: serde_json::Value,    // 成功 = 解析决策；降级 = def.neutral_fallback()
    pub raw_response: String,           // 最终轮原始输出
    pub attempts_detail: Vec<AttemptRecord>,
    pub error: Option<String>,
    pub duration_ms: u64,
}
```

- 调用锚点：`call_judgement` 新增 `anchor: JudgementAnchor` 参数（由调用 hook 传入，落库 `conversation_id` / `anchor_message_index`）：

```rust
pub struct JudgementAnchor {
    pub conversation_id: String,
    pub anchor_message_index: Option<usize>, // 触发裁决轮的用户消息索引；未绑定消息为 None
}
```

- 落库映射：`JudgementOutcome` → `hook_judgements` 记录（`status/decision/raw_response/attempts_detail/error/duration_ms`）。

### `HookDef`：hook 概念收拢表（核心组织决策）

- **本质**：代码内静态清单（`static HOOK_DEFS: &[HookDef]`），**不是数据库表、不落盘**；运行时仅读取。与 `hook_judgements`（账本，数据库表）明确区分——HookDef 是「规则」，hook_judgements 是「账本」。
- **位置**：新建 `src-tauri/src/core/hook.rs`（hook 概念唯一收拢点）。
- **结构**：

```rust
pub struct HookDef {
    pub system_type: &'static str,        // 引用 assistant_session.rs 的 SYSTEM_TYPE_* 常量（常量保留原位）
    pub label: &'static str,              // 展示名 i18n key（面板过滤下拉由表驱动）
    pub response_format: Option<ResponseFormatSpec>, // hook 自带结构化输出契约（schema 就近定义）
    pub neutral_fallback: fn() -> serde_json::Value, // 中性降级默认值
}
pub static HOOK_DEFS: &[HookDef] = &[ /* 4 个 hook 各一行 */ ];
pub fn hook_def(system_type: &str) -> Option<&'static HookDef> { ... }
```

- **收拢概念**：`system_type` 标识、`response_format` 契约（每个 hook 自带 JSON Schema，就近定义在 hook.rs）、`neutral_fallback` 中性降级值（complete_scope → `{"completed_item_ids":[]}`、match_topic → `{"action":"none"}` 等）、`label` 展示名。
- **收益**：新增 hook = 表加一行 + 一个 hook 函数；面板过滤项从 `HOOK_DEFS` 遍历生成；`neutral_fallback` 可单测。
- **边界**：`SYSTEM_TYPE_SELECT_NEURON`（候选选择）非裁决 hook，不收拢；`SYSTEM_TYPE_*` 常量保留原位，`core/mod.rs` re-export 不破坏。

### `response_format` 注入（裁决调用）

- 契约来源：`call_judgement` 签名改为接收 `def: &HookDef` + `anchor: JudgementAnchor`，`response_format` 从 `def.response_format` 取（hook 自带属性，不查全局表）。
- 注入路径（与 `thinking_override` 完全对称）：
  1. `call_judgement` 从 `def.response_format` 得 `ResponseFormatSpec`（`JsonSchema(&str)` / `JsonObject` / `None`，值类型定义于 `openai_compat.rs`，不持有 hook 数据）
  2. `run_raw_round` 新增参数 `response_format: Option<ResponseFormatSpec>`，透传 `executor.execute`
  3. providers 构建 `ChatRequest` 时，与 `apply_thinking` 同级 `req.extra.insert("response_format", ...)`（`#[serde(flatten)]` 展平为请求体顶层字段，协议层零改动）
- 能力探测降级链：`json_schema` → `json_object` → 无约束；探测结果按 `(provider_id, model_id)` 内存缓存（`OnceLock` 或 `Mutex<HashMap>`，避免每轮重复探测）。
- 明确不放的位置：不存 `conversation.extra`（裁决调用无会话态）；不塞 payload；不进神经元定义。

### Compatibility Notes / 兼容说明

- 与现有 API 的关系：`response_format` 走 `extra` 通道，协议层零改动；`run_raw_round` 新增 `response_format` 参数（既有调用点传 `None`，仅裁决调用传入）；`call_judgement` 签名由 `system_type: &str` 改为 `def: &HookDef` + `anchor: JudgementAnchor`（4 个调用点同步改）。
- 明确不做的能力：不新增手动重跑命令；不引入 `json_schema` 到主对话；不做历史数据回填迁移（表新建即空）；`SELECT_NEURON` 不收拢。

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：本 technical-plan 经用户批准；`requirements.md` 验收标准可执行。
- 若执行前需求、API、范围或交互规则变化：回写 `lifecycle.md` 并重新确认后再动代码。

### Step 1. 裁决记录存储层（Rust）

#### 文件：`src-tauri/src/core/hook_judgement_store.rs`（新建）

- 改动类型：新增
- 改动内容：`HookJudgementStore`（`conn: Arc<Mutex<Connection>>` + `on_change: Option<StateEmitter>`），含：
  - `init_table`（建表 + 索引 + 预留迁移函数）
  - `insert_start` / `finish`（两阶段写入，更新 `status/decision/raw_response/attempts_detail/error/attempts/duration_ms/updated_at`）
  - `list(filters)` 查询
  - `emit_change` 统一广播 `StateChange::HookJudgements`
  - `HookJudgementRecord` 序列化结构（对齐既有 Record 风格）
- 设计约束：
  - 对齐 `topic_store` 范式与错误处理（`AppError::StorageError`）
  - `raw_response` / `payload` 全文落库，不截断
- 验收点：`cargo test` 新增存储写入/两阶段更新/列表过滤/锚点查询测试全绿

#### 文件：`src-tauri/src/core/mod.rs`（或 `lib.rs`）

- 改动类型：修改
- 改动内容：模块注册 `hook_judgement_store`；`HookJudgementStore` 注入 managed state（setup 阶段，与 `TopicStore` 同处构造）
- 验收点：`cargo check` 通过

### Step 2. Hook 定义收拢层（Rust）

#### 文件：`src-tauri/src/core/hook.rs`（新建）

- 改动类型：新增
- 改动内容：
  - `HookDef` 结构体（`system_type` / `label` / `response_format` / `neutral_fallback`）
  - 4 个 hook 的 JSON Schema 常量（就近定义，每个 hook 一份，约束其决策输出）
  - `static HOOK_DEFS: &[HookDef]`（4 行）+ `hook_def(system_type) -> Option<&'static HookDef>` 查找
  - 4 个 `neutral_fallback` 函数（中性降级默认值）
- 设计约束：
  - 静态清单，非数据库表；不引入动态注册/trait
  - `SYSTEM_TYPE_*` 常量保留在 `assistant_session.rs`，此处引用
  - `SELECT_NEURON` 不收拢
- 验收点：`cargo test` 新增 hook_def 查找 / fallback 返回值单测全绿

### Step 3. 裁决统一纠偏（Rust 核心）

#### 文件：`src-tauri/src/core/assistant_session.rs`

- 改动类型：修改
- 改动内容：
  1. `call_judgement` 重构（签名改为 `def: &HookDef`）：
     - `response_format` 从 `def.response_format` 取；新增能力探测：按 `(provider_id, model_id)` 缓存支持级别（json_schema → json_object → 无约束），经 `run_raw_round` 新增参数透传，providers 与 `apply_thinking` 同级注入 `req.extra`
     - 新增 B 重试：`extract_json_object` 失败 → 重试 1 次，重试 payload 追加失败反馈（原输出 + 「仅返回 JSON」指令）→ 仍失败进入 A
     - 新增 A 降级：重试仍失败返回 `JudgementOutcome::Downgraded`（`def.neutral_fallback()` 中性决策 + error 摘要 + 耗时），不再 `?` 上抛
     - 全链路落库：`insert_start`（payload、model、锚点）→ `finish`（status/decision/raw/error/attempts/duration）
  2. `run_raw_round` 新增参数 `response_format: Option<ResponseFormatSpec>`（既有调用点传 `None`），透传 `executor.execute`
  3. 4 个 hook 调用点改造（`complete_scope_hook` / `match_topic` / `revise_topic` / `score_feedback_hook`）：
     - 改为 `let def = hook_def(SYSTEM_TYPE_*).expect("known hook")` → `call_judgement(def, payload, &model, &ctx.messages, JudgementAnchor { conversation_id: ctx.session_id.clone(), anchor_message_index: <触发裁决轮的用户消息索引> })`
     - 消费 `JudgementOutcome`，按中性语义处理（complete_scope 空判定 / match_topic 不创建不切换 / revise_topic 不修订 / score_feedback 跳过打分）
     - `score_feedback_hook` 既有 fail-soft 分支被统一入口接管，逻辑收敛
- 设计约束：
  - hook 级统一：纠偏逻辑只挂在 `call_judgement` / hook 基线，不按 system_type 特判
  - 重试安全：`run_raw_round` 无 coordinator 锁，直接复用
  - 中性语义与 `requirements.md` 逐条对齐
- 验收点：
  - 单测：解析失败重试 1 次成功 → `retried_ok`；重试仍失败 → `downgraded` + 主轮次不报错；能力探测降级链正确
  - 人工验证：用弱模型（如 flash）触发散文输出，主对话不中断，面板/消息卡出现降级记录

### Step 4. 事件与命令通道（Rust）

#### 文件：`src-tauri/src/core/events.rs`

- 改动类型：修改
- 改动内容：`StateChange` 新增 `HookJudgements { conversation_id: String, anchor_message_index: Option<usize>, id: String, status: String }`（开始 pending / 结束终态两阶段）
- 验收点：新增序列化测试（`hook_judgements` 变体 + tag/字段正确）

#### 文件：`src-tauri/src/core/commands.rs`（或对应 command 模块）

- 改动类型：修改
- 改动内容：新增 `hook_judgements_list(filters)` 与 `hook_defs_list()` command（前者透传 `HookJudgementStore`，后者读 `HOOK_DEFS`），注册进 invoke_handler；RPC 分发（与 `topics_list` 同模式）
- 验收点：`cargo check` + 命令层测试

### Step 5. 前端独立面板（Svelte）

#### 文件：`src/lib/layout/views.ts`

- 改动类型：修改
- 改动内容：注册 `hook-judgements` view（title `views.hookJudgements`，icon 取裁决/列表风格 SVG，component `HookJudgementPanel`，movableTo `"*"`）
- 验收点：sidebar 可见可拖拽

#### 文件：`src/lib/layout/layoutTypes.ts`

- 改动类型：修改
- 改动内容：默认 sidebar views 追加 `"hook-judgements"`（与 sessions/files/topics/tools 同级）
- 验收点：默认布局含「Hook 判定」

#### 文件：`src/lib/components/HookJudgementPanel.svelte`（新建）

- 改动类型：新增
- 改动内容：
  - 时间线列表（`created_at` 倒序）+ hook_type/status/conversation 过滤 + 空态（过滤下拉选项来自 `hook_defs_list`）
  - 记录详情展开：payload / attempts_detail（每轮原文）/ raw_response（最终轮）/ decision / error / attempts / duration_ms / model
  - 「在会话中定位」→ 切换到会话视图并滚动高亮锚点消息
  - 订阅 `HookJudgements` 事件实时插入/更新；进入时全量拉取 `hook_judgements_list` + `hook_defs_list`
- 设计约束：
  - 与「会话/文件」同级布局；不插入消息数组
  - 状态着色对齐 `ok`(绿) / `retried_ok`(蓝) / `downgraded`(黄)
- 验收点：面板列表、过滤、详情、定位可用；运行中裁决实时出现

### Step 6. 前端消息内联裁决卡（Svelte）

#### 文件：`src/lib/components/MessageList.svelte`（或对应消息列表组件）+ 新建 `src/lib/components/JudgementCard.svelte`

- 改动类型：修改 + 新增
- 改动内容：
  - `JudgementCard.svelte`：锚点消息下方附属渲染块——「裁决中」spinner → 终态徽标（✓/⚠/✕）+ 结果摘要 + 展开完整输入/输出/决策
  - 消息列表按 `anchor_message_index` 组装附属块：从 `hook_judgements_list` 拉取该会话记录 + 订阅 `HookJudgements` 事件实时更新
  - 独立旁路列表渲染，**不插入消息数组、不影响 `message_index` 与虚拟滚动**
- 设计约束：
  - 两阶段事件驱动进度（pending → 终态）
  - 重启后照常渲染（数据落库）
  - AI 侧透明：模型输入/输出/降级原因均可见
- 验收点：消息列表锚点消息下出现裁决卡；裁决中实时转终态；虚拟滚动与滚动定位正常

### Step 7. i18n 与检查回写

#### 文件：`src/lib/i18n/*`（zh/en）

- 改动类型：修改
- 改动内容：新增 `views.hookJudgements`、状态标签（`judgement.status.ok` 等）、字段名（payload/raw/decision/error/attempts/duration）、空态/过滤文案 key
- 验收点：面板与消息卡文案中英完整

#### 命令

- 运行：`cargo check`、`cargo test`、`pnpm --filter pulsar-app check`
- 修复：按编译器/类型错误逐一修复至 0 error

#### 文件：`docs/sdd-lab/2026-08-22_11-10_hook-judgement-recovery/lifecycle.md`

- 回写执行记录：
- 记录实际改动摘要：
- 记录验证结果：
- 记录下一步状态：

## Risk And Mitigation / 风险与缓解

- 风险：能力探测结果误判（网关声称支持 json_schema 但行为不一致）→ 缓解：探测走 openai_compat 既有 extra 通道并新增网关行为测试；失败时沿降级链落到 json_object / 无约束，B/A 兜底。
- 风险：重试增加裁决耗时（约 1 次模型调用）→ 缓解：重试次数固定 1；重试轮为纯模型调用无额外副作用。
- 风险：事件风暴（每轮多次裁决开始/结束事件）→ 缓解：两阶段粒度 + 按会话/锚点收敛；前端合并更新不重拉全量。
- 风险：消息卡破坏虚拟滚动/滚动定位 → 缓解：附属旁路渲染，不修改消息数组与 message_index；验收含滚动回归。
- 风险：降级语义被误扩展 → 缓解：中性默认值语义在 requirements 中逐 hook 固化，实现逐一对照。
- 风险：`raw_response` 全量落库体积增长 → 缓解：用户明确全量保留；查询按分页 + 详情按需展开（列表不加载全文）。

## Execute Checkpoint / 执行检查点

- 当前理解：以 hook 级统一纠偏（A+B+C）为核心，裁决全量落库，通过独立面板 + 消息内联裁决卡实现用户侧透明，AI 侧裁决过程不黑盒。
- 核心目标：裁决失败永不阻断主轮次；每次裁决可查、可见、可定位。
- 下一步动作：等待用户批准本 technical-plan → 进入 executing（按 Step 1-7 执行）。
- 风险：主要集中在能力探测兼容性与消息卡对既有滚动的影响，均有明确缓解与验收项。
