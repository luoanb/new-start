# Technical Plan / 技术方案: Hook 周期管理（流程决策升级）

## Requirement Baseline / 需求基线

- 对应需求文档：`docs/sdd-lab/2026-09-12_19-57_hook-cycle-management/requirements.md`
- 需求确认状态：**部分确认**——§业务诉求清单 与 §周期字段集 已定；剩余 Open Questions O3（第 3、6 项是否可调）/ O4 / O5 / O6 由本方案给出选项待拍板
- 已定口径（必须遵守，不得违背）：
  1. **周期条件只能引用框架从 `RoundContext` 派生的周期字段**（§Cycle Fields）；
  2. **业务状态不进周期条件**——由 hook 内部按 `ctx.session_id` 自查；
  3. **计数源并存**——派生值仅服务周期判定，现有 `topic.extra` 计数与其消费方（轮询退避 / 暂停）不动；
  4. **维度正交**——`轮次来源(用户轮｜调度轮) × 产物形态(工具轮｜收尾轮)`；产物形态**可指定轮次**（本轮 | 上一轮）；
  5. **周期 = 调度（何时调用动作）**；动作被调用后的**内部决策留在 hook 内**（如简报刷新的「内容变化」、压缩是否压）；
  6. 业务侧要求保留且可调：用户轮裁决节奏、复盘仅收尾轮、简报刷新（N + 上轮非工具开关）、选型节奏。
- 本方案覆盖范围：周期字段派生 → 可调项声明 → 调用前判定 → 取值与持久化 → 命令/RPC → 面板 → 测试与回写。

## Current Project Facts / 当前项目事实

### 核心契约与注册表

- `core/hook/defs.rs`：
  - `HookDef { id, label, inject_point, handler }`（L89-L94）；`RegisterError { DuplicateId, UnknownId }`（L98-L103）；
  - `RegisteredHook { def, enabled }`（L124-L127）；`register` 默认关闭（L162-L175）；`set_enabled` / `is_enabled` / `is_registered`（L180-L204）；`snapshot` 只取 enabled（L208-L220）；
  - 5 个 `run_*` 仅按注入点分发，**没有任何门控求值**（L227-L308）。
- 装配 6 条 hook（注册期即 `set_enabled(true)`）：IP-1 `assistant.round.before` / `assistant.user-round-judgement` / `assistant.select-neuron`；IP-2 `core.compaction`；IP-5 `assistant.round-review` / `assistant.round.after`。休眠 4 条（定义·未注册）：`score_feedback` / `match_topic` / `revise_topic` / `complete_scope`。

### 周期相关逻辑今天散落在哪（全部是硬编码，需收敛/外置）

| 逻辑 | 位置 | 今天的值 |
|---|---|---|
| 模式窗口 | 各 hook `run` 首行 `matches!(ctx.mode, Assistant \| System)` | 助手 / 系统 |
| 触发来源窗口 | `user_round_judgement::run` L111 `matches!(ctx.trigger, User)` | 仅用户轮 |
| 裁决节奏 | `need_user_round_judgement(topic_bound, user_rounds)`（`assistant_session.rs` L1613-L1615） | 未绑定必跑；已绑定 `user_rounds % 3 == 0` |
| 复盘收尾轮门控 | `is_settling_round(outcome)`（L1620-L1622）+ `round_review.rs` L189 | 无 tool_calls 且无 tool_results |
| 简报刷新节奏 | `BRIEF_EVERY_N_ROUNDS = 3`（L1604）+ `should_refresh_brief(...)`（L1393-L1399） | 每 3 个推进轮（或内容变化 / 上轮非工具） |
| 选型节奏 | `SELECTION_EVERY_N_ROUNDS = 5`（L1626）+ `ctx.reselect = poll_count % 5 == 0`（L1427） | 每 5 个推进轮 |

### 周期字段的可用原料

- `RoundContext`（`core/round_service.rs`）：`session_id` / `mode` / `seed` / `state` / `messages` / `model_input` / `model` / `tool_override` / `trigger` / `topic_id` / `reselect` / `nudge_persist` / `selected_neuron` / `outcome`。
- `MessageRole { User, Assistant, System, Tool, Compaction }`（`core/models.rs` L9-L15）；`Message { role, body, timestamp, neuron_id }`（L63-L70）。
- `RoundProduct { response, model_output, tool_calls, tool_results, reasoning, selected_neuron_id }`（`core/round_types.rs` L49-L62）。
- **`SessionState` 只有 `last_selected_neuron_id` / `model`（L24-L31），不含任何轮次计数** —— 计数现状在 `AssistantTopicState`（`topic.extra`）。
- 现有「上轮是否以工具结束」判据：`runner.last_message_is_tool_result(session_id)`（`round_review.rs` L169-L172 在用）。

### 对外接口与前端

- `hook_defs_list()` 只回 2 条裁决的 `{ system_type, label }`（`lib.rs` L543-L547）。
- `hook_judgements_list(filters)` 回账本 `{ records, total }`（分页 + 后端过滤已具备）。
- 面板 `HookJudgementPanel.svelte`：`panel-toolbar` + `filter-bar` + 单层滚动 `.list` + 账本行展开；纯只读。
- 视图 `hook-judgements`（`views.ts`）、i18n `views.flowDecisions`；契约 `contracts.ts` / 类型 `types.ts`。
- 可复用组件：`Toggle.svelte`、`Select.svelte`；数值输入规格见 `ModelPicker.svelte` `.params-row`。
- 持久化范式：`config.json`（`ConfigStore::update` 原子写 + 写盘代数），先例 `git.dangerous_writes` / `poller.parallelism`。

## Open Questions / 开放问题

- [x] Q1 判定落点 = **方案 A（框架调用前统一判定）**（用户已确认）。
- [x] Q2 可调项表达 = **方案 C（声明式预设项）**（用户已确认）；**不做**字段自由组合（方案 D）。
- [x] Q3 全部动作均可调（用户裁定）：第 3 项（轮前准备）、第 6 项（轮次计数）**各暴露「模式窗口」**；`core.compaction` 因「是否压缩不属周期」**仅暴露启停、无周期可调项**。
- [x] Q4 `protected` 特殊化：**取消**（用户裁定）——全部动作统一可启停；对「关停会破坏业务语义」的动作给**风险提示**（`disable_hint`，不硬拦截、不在服务端拒绝）。
- [x] Q5 面板与视图：**统一管理**（用户确认）——一个面板统一列出并管理全部动作，沿用升级现有 `hook-judgements` 视图（视图 id 保留）。

## Solution Options / 方案候选

### 判定落点

#### 方案 A / 框架调用前统一判定（推荐）

- 推荐：是
- 方案摘要：`HookRegistry` 的 5 个 `run_*` 在调用 handler 前，用 `CycleFacts::derive(ctx)` 派生周期字段，对声明为「调用判定」的项求值，不满足则记 skip 并跳过该 hook。handler 内不再出现模式 / 触发来源 / 收尾轮 / 节奏判定。
- 涉及模块：`core/hook/cycle.rs`（新）、`core/hook/defs.rs`、各 hook 定义处、`assistant_session.rs` 的 `advance_brief`
- 优点：与「周期 = 调度」语义一致（判定与调用同处）；判定只此一处，可观测（skip 日志带原因）；新增动作零判定代码。
- 缺点：`CycleFacts` 需在每次分发时派生（内存扫描，成本可接受）；动作内部读的参数需另一条读取通道。
- 风险：把既有门控迁移到声明时遗漏 → 用「默认声明 ≡ 原行为」单测锁定。

#### 方案 B / 动作内部各自判定

- 推荐：否
- 方案摘要：框架只按 `enabled` 过滤，门控仍由各 hook `run` 自己判。
- 优点：改动最小。
- 缺点：**与用户否定的前稿同构**——判定散落在各动作，「周期」无法作为统一概念管理；新增动作要写判定；无法统一观测。
- 风险：回到「每新增维度就要改业务代码」的老问题。

### 可调项表达

#### 方案 C / 声明式预设项（推荐）

- 推荐：是
- 方案摘要：每个动作**声明**它提供哪些可调项（引用哪个周期字段、什么形态、候选取值或范围、默认值、用途 = 调用判定 / 动作内部）。消费方只能在声明的项上取值；面板按声明渲染。
- 优点：提供方（动作定义处）掌控对外可调面；面板与命令层零领域知识；校验有据；不会配出无意义的组合。
- 缺点：消费方不能组合出声明之外的条件（如需新组合须由提供方加一项）。
- 风险：声明粒度需拿捏（过细=配置项爆炸；过粗=不够灵活）。

#### 方案 D / 字段自由组合（原「自定义条件」）

- 推荐：否（除非明确要求）
- 方案摘要：面板允许在周期字段集上自由组合条件（AND/OR/比较），不受动作声明限制。
- 优点：最灵活。
- 缺点：容易配出无意义或与动作语义冲突的条件；校验与展示复杂；与「提供方给参数、消费方取值」的方向不一致。
- 风险：节奏/条件语义漂移，难排障。

### 持久化

- 沿用 `config.json`（原子写 + 写盘代数），不新建 SQLite 表 —— 无备选（需求已定：全局、config.json）。

## Decision / 方案决策

- Selected / 选定方案：
  1. Q1 = **方案 A**（框架调用前统一判定）
  2. Q2 = **方案 C**（声明式预设项；不做字段自由组合）
  3. 可调面 = **全部动作均可调**（第 3、6 项暴露「模式窗口」；`core.compaction` 仅启停，是否压不属周期）
  4. 启停 = **统一可启停**，无硬保护；高风险动作给 **`disable_hint` 风险提示**
  5. 管理面 = **统一管理**（一个面板列全部动作），沿用升级 `hook-judgements` 视图
- Why / 选择依据：A 才使「周期」成为统一、可观测的调度概念；C 才实现「提供方决定可调面、消费方仅取值」，并让面板/命令层零领域知识（满足通用性）；统一可启停 + 提示，与「统一管理」取向一致并避免特例分叉。
- Decision Owner / 决策人：user（Q1~Q5 全部确认）
- Decision Time / 决策时间：2026-09-12
- Open Questions 状态：**Q1~Q5 全部关闭**

## API Design / API 设计

### Contract Scope / 契约范围

- 变更类型：新增 + 扩展（对既有命令无破坏性变更）
- 消费方：前端「周期管理」面板；RPC；`gateway` 装配；各 hook handler
- 真相源文件：`core/hook/cycle.rs`（新）、`core/hook/defs.rs`、`infra/config.rs`、`lib.rs`、`net/rpc.rs`、`src/lib/types.ts`、`src/lib/api/contracts.ts`

### 1. 周期字段派生（框架侧，`core/hook/cycle.rs`）

```rust
/// 周期字段：框架从 RoundContext 派生的判定素材（业务状态一律不在其中）。
pub enum CycleField {
    Mode,             // ctx.mode
    Trigger,          // ctx.trigger
    RoundIndex,       // messages 中 role == Assistant 的条数
    UserRounds,       // messages 中 role == User 的条数
    RoundsSinceUser,  // 最后一条 role == User 之后的 role == Assistant 条数
    RoundOrigin,      // UserRound | ScheduledRound（由 trigger 派生）
    RoundShape,       // ToolRound | SettlingRound（由产物派生，可指定轮次）
}

/// 产物形态取哪一轮。
pub enum RoundRef { Current, Previous }

/// 周期事实快照：每个注入点分发前派生一次（前序 hook 改写 ctx 后重新派生）。
pub struct CycleFacts {
    pub mode: String,
    pub trigger: String,
    pub round_index: u64,
    pub user_rounds: u64,
    pub rounds_since_user: u64,
    pub round_origin: RoundOrigin,
    /// 本轮产物形态；IP-1 / IP-2 为 None（产物未产生）。
    pub round_shape_current: Option<RoundShape>,
    /// 上一轮产物形态；任意注入点可得（messages 末条是否 role == Tool）。
    pub round_shape_previous: RoundShape,
}

impl CycleFacts {
    pub fn derive(ctx: &RoundContext) -> Self;
    pub fn field_str(&self, f: CycleField, r: RoundRef) -> Option<Cow<'_, str>>;
    pub fn field_num(&self, f: CycleField) -> Option<i64>;
}
```

- 派生口径遵循需求 §派生口径约束：IP-1/IP-2 的 `round_shape_current = None`；`round_shape_previous` 由「`messages` 末条是否 `role == Tool`」得出。
- **派生值不写回、不落库**（计数并存原则）；仅本次分发内使用。

### 2. 可调项声明（动作提供方）

```rust
/// 可调项形态（决定面板控件与校验）。
pub enum CycleParamKind {
    Enum { values: &'static [&'static str], multi: bool }, // 单选 / 多选
    Bool,
    Number { min: i64, max: i64 },
}

/// 可调项用途：框架调用前判定，还是动作内部读取。
pub enum CycleParamUsage { CallGate, Internal }

pub struct CycleParamSpec {
    pub key: &'static str,
    pub label: &'static str,        // i18n key
    pub field: CycleField,          // 绑定的派生字段
    pub round_ref: RoundRef,        // RoundShape 时生效（其余字段忽略）
    pub kind: CycleParamKind,
    pub default: CycleValue,        // 默认值 = 默认判定/默认节奏
    pub usage: CycleParamUsage,
}
```

- **一个概念 + 一个属性**：所有可调项都是「周期参数」，`usage` 决定它在调用前判定还是在动作内部被读——不再拆成两套机制。
- 默认值即现状值 → 不改配置时行为与今天一致。

### 3. 判定与取值（`core/hook/defs.rs`）

```rust
pub struct HookDef {
    pub id: &'static str,
    pub label: &'static str,
    pub inject_point: InjectPointId,
    pub handler: HookHandler,
    pub group: &'static str,                    // 开放 i18n key（面板分组）
    /// 关停风险提示（i18n key）；`None` = 关停无风险提示。
    /// 不设硬保护：服务端不拒绝关停，仅由面板据此提示。
    pub disable_hint: Option<&'static str>,
    pub cycle_params: &'static [CycleParamSpec],
}

struct RegisteredHook {
    def: Arc<HookDef>,
    enabled: bool,
    values: BTreeMap<&'static str, CycleValue>, // 生效取值（初始 = 各 spec.default）
}

impl HookRegistry {
    pub fn register(&self, def: HookDef) -> Result<(), RegisterError>;
    pub fn set_enabled(&self, id: &str, on: bool) -> Result<(), RegisterError>;
    /// 取值：按 spec 形态/范围校验。
    pub fn set_value(&self, id: &str, key: &str, value: CycleValue) -> Result<(), RegisterError>;
    /// 动作内部读取（usage = Internal）。
    pub fn param_of(&self, id: &str, key: &str) -> Option<CycleValue>;
    pub fn snapshot_all(&self) -> Vec<HookEntry>;

    /// 5 个 run_*：每个 hook 前派生 facts → 调用判定 → 通过才调 handler；未通过记 skip 日志。
    pub async fn run_after_load_context(&self, ctx: &mut RoundContext, on_session_switch: ..) -> AppResult<()>;
    // run_after_persist_input / run_after_call_model / run_after_execute_tools / run_after_persist_outcome 同构
}

/// 调用判定：对所有 usage = CallGate 的项求「本次是否满足」。
/// Enum(multi)  → 派生字段值 ∈ 配置集合
/// Bool         → 派生字段值 == 配置
/// Number       → 派生字段值 % N == 0
fn gate_match(params: &[CycleParamSpec], values: &BTreeMap<..>, facts: &CycleFacts) -> bool;
```

- `CycleFacts` **每个 hook 前重新派生**（前序 hook 可能改写 `ctx`：会话切换 / `topic_id` / `messages`）；派生为内存扫描，无 IO。
- `RoundShape/Current` 在 IP-1/IP-2 为 `None` → 该判定按「不匹配」处理（不会误放行）；声明该项的动作需改用 `RoundRef::Previous`。

### 4. 各动作的默认声明（与原行为逐条等价）

| 动作 | 调用判定项（默认） | 动作内部项（默认） |
|---|---|---|
| `assistant.user-round-judgement` | `mode ∈ {assistant, system}`；`round_origin == user_round` | `review_every_n` 每 **3** 条用户消息（**注**：见下方落地偏差说明） |
| `assistant.round-review` | `mode ∈ {assistant, system}`；`round_shape(Current) == 收尾轮` | — |
| `assistant.round.before` | `mode ∈ {assistant, system}`（**可调**） | `brief_every_n` 每 **3** 个推进轮（简报刷新）；`brief_on_prev_settling` 开关（默认开） |
| `assistant.select-neuron` | — | `selection_every_n` 每 **5** 个推进轮（选型节流） |
| `assistant.round.after` | `mode ∈ {assistant, system}`（**可调**） | — |
| `core.compaction` | —（`All[]` 恒真，每轮调用） | —（是否压由内部按阈值检测，**不属周期**；仅暴露启停） |

- **全部动作均可调**（Q3）：第 3、6 项各暴露「模式窗口」一项；`core.compaction` 无周期可调项。
- `user-round-judgement` 的「**未绑定课题必跑**」是业务状态 → **不在此表**，留在 handler 内。
- 简报刷新的「**内容变化即刷新**」同为业务状态 → 留在 handler 内。
- 关停风险提示（`disable_hint`）首发给：`round.before`（丢课题解析 / 简报推进）、`round.after`（丢「末轮待续推」标记）、`core.compaction`（超长上下文可能请求失败）。**均不阻止关停**。

### 5. 持久化（`infra/config.rs`）

```rust
pub struct HooksSection {
    /// 启停覆盖：{ "<hook id>": true|false }
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<serde_json::Value>,
    /// 取值覆盖：{ "<hook id>": { "<param key>": value } }
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<serde_json::Value>,
}
// AppConfigFile 增 hooks: Option<HooksSection>
```

- 读写边界用各 `CycleParamSpec` 校验（key 是否声明、形态与范围是否合法），非法则回落默认 + warn。
- 复用 `ConfigStore::update`（原子写 + 写盘代数）；启动装配后应用覆盖，运行期改动「写盘 + 同步内存」。

### 6. 命令与 RPC

```rust
/// 清单：由 HookDef 派生（含可调项声明与当前取值），面板据此渲染。
hooks_list() -> Vec<HookEntry>;                                  // 新增
/// 启停：统一可切（无硬保护；高风险动作由面板按 `disableHint` 提示）。
hook_set_enabled(id: String, on: bool) -> Result<(), String>;    // 新增
/// 取值：按声明校验（形态 / 候选值 / 范围），CallGate 与 Internal 同一入口。
hook_set_value(id: String, key: String, value: CycleValue) -> Result<(), String>; // 新增
/// 账本沿用。
hook_judgements_list(filters) -> HookJudgementListResult;
```

- 命令层与配置层**不含任何具体 hook id 与领域选项**；校验依据全部来自 `HookDef.cycle_params`。

### 7. 前端契约

```ts
export type CycleParamKind =
  | { kind: "enum"; values: string[]; multi: boolean }
  | { kind: "bool" }
  | { kind: "number"; min: number; max: number };

export type CycleParam = {
  key: string; label: string; usage: "call_gate" | "internal";
  kind: CycleParamKind; default: boolean | number | string | string[];
  value: boolean | number | string | string[];
};

export type HookEntry = {
  id: string; label: string; group: string; inject_point: string;
  enabled: boolean;
  /** 关停风险提示（i18n key 解析后文案）；null = 无提示。不阻止关停。 */
  disableHint: string | null;
  params: CycleParam[];
};
```

- `contracts.ts` 增 `hooksList` / `hookSetEnabled` / `hookSetValue` 三条。
- 面板渲染规则（无领域硬编码）：`kind.kind = enum && multi` → chips；`enum && !multi` → `Select`；`bool` → `Toggle`；`number` → 数值输入（min/max）。

### Compatibility Notes / 兼容说明

- `hook_defs_list` 保留（面板过滤下拉改由 `hooks_list` 供给，可后续收拢）。
- `hook_judgements_list` 出参与账本表结构不变。
- 视图 id `hook-judgements` 不变；`config.json` 新增 `hooks` 节，缺省即回落默认，老配置不丢数据。

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：本方案经用户批准；Open Questions Q1~Q5 全部关闭。
- 若执行前需求 / API / 范围变化：先回写需求文档与本方案，再执行。

### Step 1. 周期字段与可调项契约（新增 `core/hook/cycle.rs`）

- 改动内容：`CycleField` / `RoundOrigin` / `RoundShape` / `RoundRef` / `CycleFacts`（含 `derive` / `field_str` / `field_num`）；`CycleParamKind` / `CycleParamUsage` / `CycleParamSpec` / `CycleValue`；`gate_match`；`core/hook/mod.rs` re-export。
- 设计约束：纯数据 + 纯函数，不依赖业务模块。
- 验收点：单测——`derive` 各字段口径（含 IP-1「不含本轮」、`round_shape_previous`）；`RoundShape(Current)` 在 IP-1/IP-2 为 `None`；`gate_match` 三种形态判定；越界/未知 key 校验失败。

### Step 2. 注册表判定与取值（`core/hook/defs.rs`）

- 改动内容：`HookDef` 增 `group` / `disable_hint` / `cycle_params`；`RegisteredHook` 增 `values`；新增 `set_value` / `param_of` / `snapshot_all`；5 个 `run_*` 改为「派生 facts → `gate_match` → 通过才调 handler，未通过记 skip」。
- 设计约束：判定只此一处；facts 每 hook 前派生；`enabled` 与判定互不覆盖。
- 验收点：单测——未开启不分发；`CallGate` 不满足则 handler 不执行且记 skip；`set_value` 后行为随之变化；`disable_hint` 仅为出参信息（不参与分发）；既有「注册未开启不分发」「运行期开关」测试保持绿。

### Step 3. 各动作声明可调项 + handler 去判定 + 内部参数改读

- 文件：`application/hook/instances/{user_round_judgement,round_review}.rs`、`application/assistant_session.rs`（`round.before` / `round.after` / `advance_brief`）、`application/gateway.rs`（`select-neuron` / `compaction`）
- 改动内容：
  - **全部 6 个动作**声明 `cycle_params`（默认值 = 现状值，见 §API-4 表）：第 1、2 条为裁决；第 3、6 条暴露「模式窗口」；第 4/5 条（简报刷新 / 选型）暴露 `Internal` 节奏项；`core.compaction` 无周期可调项；
  - 为 `round.before` / `round.after` / `core.compaction` 声明 `disable_hint`（关停风险提示文案 i18n key），**不阻止关停**；
  - `user_round_judgement::run` 删除 mode / trigger / `need_user_round_judgement` 判定（保留「未绑定必跑」业务判断）；
  - `round_review::run` 删除 mode / `is_settling_round` 判定；
  - `AssistantHooks::round_before` / `round_after` 删除 `matches!(ctx.mode, ..)`；
  - `advance_brief` / `should_refresh_brief` 的 `BRIEF_EVERY_N_ROUNDS`、`reselect` 的 `SELECTION_EVERY_N_ROUNDS` 改从 `param_of(...)` 读（常量保留为声明默认值来源）；
  - `AssistantSession` 持有 `Arc<HookRegistry>` 以读取 `Internal` 参数。
- 验收点：单测——默认声明 ≡ 原行为（原 `need_user_round_judgement` / `is_settling_round` 用例迁移为判定用例）；简报 N / 选型 N 改值后行为随之变化；未绑定必跑仍生效。

### Step 4. 持久化与启动覆盖（`infra/config.rs` + `application/gateway.rs`）

- 改动内容：新增 `HooksSection`（`enabled` / `values`）与 `AppConfigFile.hooks`；`Gateway` 持 `Arc<HookRegistry>` 并暴露 getter；装配完成后应用配置覆盖；新增 `set_enabled_persist` / `set_value_persist`（内存 + `ConfigStore::update` 双写）。
- 验收点：启动覆盖生效；重启保留；非法值回落默认 + warn；写盘代数递增。

### Step 5. 命令与 RPC

- 文件：`lib.rs`、`net/rpc.rs`
- 改动内容：新增 `hooks_list` / `hook_set_enabled` / `hook_set_value`（统一可启停、无硬保护；高风险动作仅由面板按 `disableHint` 提示）；RPC 三分支同步；`app.manage` 注册表句柄；命令注册表补三条。
- 验收点：命令层无具体 hook id / 领域选项硬编码；非法取值返回可读错误；RPC 与 Tauri 行为一致。

### Step 6. 前端面板升级

- 文件：`src/lib/types.ts`、`src/lib/api/contracts.ts`、`src/lib/components/HookJudgementPanel.svelte`、`src/lib/i18n/translations.ts`（三处）、`src/lib/layout/views.ts`
- 改动内容：
  - 契约与类型按 §API-7；
  - 面板改「周期管理」：Hook 列表（启停 + 分组 + 关停风险提示 `disableHint`）→ 展开按 `params[].kind` 渲染控件（无领域硬编码）；账本时间线区整体保留；
  - `views.flowDecisions` → `views.cycleManagement`（zh「周期管理」/ en "Cycle Management"），视图 id 保留。
- 验收点：新增动作（后端声明）后**前端零改动**即出现其可调控件；启停 / 取值即时生效并持久化；非法值被拒；账本区不回归。

### Step 7. 检查与回写

- 命令：`cargo check --all-targets`、`cargo test --lib`、`pnpm --filter pulsar-app check`
- 回写活文档：`docs/pulsar/hook/index.md`（新增周期字段 / 可调项 / 判定契约）、`docs/pulsar/architecture.md`
- 回写 `lifecycle.md`：执行记录 / 改动摘要 / 验证结果 / 下一步

## Risk And Mitigation / 风险与缓解

- 风险：门控从各 run 迁移到声明时遗漏或口径变化 → 行为回归
  - 缓解：逐条对照 §当前项目事实 的硬编码表迁移；「默认声明 ≡ 原行为」单测；既有 452 项测试全量回归。
- 风险：`CycleFacts` 派生口径（`messages` 扫描）与实际轮次语义不一致
  - 缓解：口径写入需求 §派生口径约束 并单测锁定；派生值不写回、不影响既有计数。
- 风险：IP-1/IP-2 本轮产物未知 → 误判「收尾轮」
  - 缓解：`round_shape_current` 在 IP-1/IP-2 为 `None`，判定按不匹配处理；需要判产物形态的动作使用 `RoundRef::Previous`。
- 风险：简报刷新 / 选型节奏改为读注册表后与既有 `poll_count` 语义错位
  - 缓解：默认值与原常量一致（3 / 5）并单测比对；`Internal` 参数只读不改业务计数。
- 风险：误关高风险动作（如 `round.before` / `round.after` / `compaction`）导致功能残缺
  - 缓解：声明 `disable_hint`，面板在关停时给风险提示；不做硬拦截（用户已裁定统一可启停）。
- 风险：`disable_hint` 被误当作硬保护实现（在服务端拒绝关停）
  - 缓解：明确 `disable_hint` 仅为出参信息，不参与分发与命令校验；单测锁定「任何动作都可 `set_enabled(false)`」。
- 风险：`Internal` 参数被误当作判定项（或反之）→ 语义错乱
  - 缓解：`usage` 为声明必填字段；单测覆盖两类行为差异。
- 风险：可调项声明粒度失控（配置项爆炸或过度粗糙）
  - 缓解：首发只声明必要项——裁决两项（模式 / 用户轮 + 节奏 N）、复盘两项（模式 / 收尾轮）、轮前准备三项（模式 + 简报 N + 上轮非工具）、选型两项（模式无关，仅节奏 N）、轮次计数一项（模式）；`compaction` 无可调项。

## Execute Checkpoint / 执行检查点

- 当前理解：周期 = **调度**——框架从 `RoundContext` 派生周期字段，动作声明「可调项」（引用字段 + 形态 + 默认 + 用途），框架在调用前判定（`CallGate`），动作内部读 `Internal` 项；业务状态与动作内部决策不进周期；配置写 `config.json`。
- 核心目标：①周期判定统一、可观测；②提供方决定可调面、消费方仅取值，面板/命令零领域知识（新增动作零界面开发）；③默认行为与今天完全一致。
- 下一步动作：Q1~Q5 已全部关闭（A / C / 全部动作可调 / 取消硬保护改风险提示 / 统一管理）→ **等待用户批准本方案**，批准后进入 executing（Step 1-7），并重做 `visual-design.md`。
- 风险：门控迁移等价性与派生口径一致性，靠单测 + 全量回归兜底。

## 落地结果（2026-09-12，已完成）

**已落地**：

- **Step 1** `core/hook/cycle.rs`（新）：`CycleField` / `RoundOrigin` / `RoundShape` / `RoundRef` / `CycleFacts::derive`（含 `field_str` / `field_num` / `field_bool`）；`CycleParamKind` / `CycleParamUsage` / `CycleParamSpec` / `CycleValue` / `validate_value` / `gate_check`（+`gate_match`）；单测 10 项（派生口径 / 三种形态判定 / 空集合=不限 / 缺失字段 fail-safe / 越界拒收）。
- **Step 2** `core/hook/defs.rs`：`HookDef { id, label, inject_point, handler, group, disable_hint, cycle_params }`；`RegisteredHook { def, enabled, values }`；`param_of` / `set_value` / `snapshot_all`（按 id 稳定排序）查看/取值；5 个 `run_*` 改为「派生 facts → `gate_check` → 通过才调 handler，未命中记 skip（`PHASE_HOOK_CYCLE_GATE`）」；`RegisterError` 增 `UnknownParam` / `InvalidParam`；单测含「门控未命中不执行」「取值校验」「任何动作都可关停」。
- **Step 3** 各动作声明 + handler 去判定：`round.before`（mode + `brief_every_n` / `brief_on_prev_settling`）、`round.after`（mode）、`select-neuron`（`selection_every_n`）、`user-round-judgement`（mode + `round_origin` + `review_every_n`）、`round-review`（mode + `round_shape(Current)`）、`compaction`（无周期项）；`AssistantSession` 持 `Arc<HookRegistry>`（`cycle_int` / `cycle_bool` / `selection_every_n` 读取）；handler 内删除 mode / trigger / `is_settling_round` / 频率常量判定。
- **Step 4** `infra/config.rs` 增 `HooksSection { enabled, values }`（结构无领域知识）+ `AppConfigFile.hooks`；`Gateway` 持 `hook_registry` + `hook_registry()` getter + `set_hook_enabled_persist` / `set_hook_value_persist`（内存 + 原子写双写）；`apply_hook_config` 启动应用覆盖，非法项跳过并 warn。
- **Step 5** 命令 `hooks_list` / `hook_set_enabled` / `hook_set_value`（`lib.rs`）+ RPC 三分支同步；命令层无具体动作 id 与领域选项。
- **Step 6** 前端：`types.ts` 增 `HookEntry` / `HookParamView` / `HookParamKind` / `CycleValue`；`contracts.ts` 增三条契约；`HookJudgementPanel.svelte` 改「周期管理」（分区 tab「动作 / 执行记录」），动作区**完全由 `hooks_list` 驱动渲染**（`enum && multi` → chips / `enum` → Select / `bool` → Toggle / `number` → 数值框）；i18n `views.flowDecisions` → `views.cycleManagement`（类型 / en / zh + `views.ts`），新增 `cycle.*` 文案；视图 id `hook-judgements` 保留。
  - **UI 迭代（用户反馈「太丑」后收敛）**：动作行改**两行结构**（名称 / 分组·注入点）＋ **hairline 行分隔**（不用卡片）＋ 状态仅在停用时显示；**启停开关移入展开面板**（折叠行无控件），风险提示随之移入展开面板；参数按 `usage` 分组（「触发」/「参数」）且宽控件（chips / Select）转上下布局。**i18n key 全部改为扁平命名**（`cycle.paramMode` / `cycle.groupShell` / `cycle.hintRoundBefore` / `cycle.hookRoundBefore` 等），与 `translations.ts` 的 `cycle.*` 扁平键一一对应（此前 Rust 侧误用点号命名导致显示原始 key）。

**落地偏差（Reverse Sync，已回写本表）**：

- `assistant.user-round-judgement` 的节奏项**未按原计划放入 `CallGate`**，而是作为 `Internal`（`review_every_n`）+ 在 handler 内与「未绑定课题必跑」取 OR。原因：该频率与**业务状态**（课题是否绑定）是 OR 关系，而按边界「业务状态不进周期条件」，`CallGate` 是严格 AND，无法表达该组合。行为与原实现等价（默认 3）。

**验证**：

- `cargo check --all-targets`：0 错 0 警告。
- `cargo test --lib`：**482 passed / 0 failed**（含新增周期契约与门控分发单测）。
- `pnpm --filter pulsar-app check`：**0 errors** / 20 warnings（既有基线，非本次引入）。

**未纳入本轮**：

- `visual-design.md` 仍需按新契约重做（现内容基于已撤销设计：`guard` / `params` / `factSpecs` 表述已过时）。
- `docs/pulsar/hook/index.md` 已补「周期契约」小节；`docs/pulsar/architecture.md` 未逐字更新。
