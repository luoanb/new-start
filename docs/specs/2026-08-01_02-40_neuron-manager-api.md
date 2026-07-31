# Spec: NeuronManager 对外 API 契约

## Goal

- 要解决什么问题：把「神经元管理」收成清晰正门 API——除 `create_neuron` 种子外，所有系统提示词走同一创建流；`create` / `select` 为其服务；系统提示词存在则不再创建。
- 验收结果：正门 API 落地；旁路收敛；AI 工具替换；`neuron-init.md` 反写；测试通过。

## Done Contract

- 什么算完成：契约 API 在 `NeuronManager` 落地；旧创建旁路移除；`create_downstream_neuron` 工具删除并由 `create_neuron` 替换；文档反写；`cargo test --lib` 通过。
- 由什么证明：测试输出 + 代码/文档对照本 spec。
- 哪些情况仍算未完成：GUI 运维面板、课题匹配运行时重构（Out）。

## Scope

- In:
  - `NeuronManager` 业务正门 API（类型 + 方法语义）
  - 旁路收敛与 AI 工具替换
  - 消费方调用更新（Gateway / Assistant / TUI）
  - `neuron-init.md` 反写
- Out:
  - 课题匹配运行时是否改成「对课题 7 选 1」（另开需求）
  - GUI 运维面板实现细节
  - Provider / 模型调用合并

## Facts / Constraints

- 已确认事实：
  - 除创建提示词 `create_neuron` 外，系统提示词必须走同一套创建流：`ensure_creator` → pool→7→1 → 生成专用 content → 落库。
  - `create_neuron` 与 `assistant_select_neuron` 是底座，服务于其它系统提示词创建、普通神经元创建、运行时能力选一。
  - 系统提示词：确认存在后不再执行创建（`reset` 除外）。
  - 现状主路径已有 `ensure_system_neuron` / `select_one` / `create_generated` / `bootstrap_ready`；旁路仍存在（`create_for_admin`、`set_system_type_for_admin`、AI `create_downstream_neuron` 直写）。
  - 无跨包 crate 消费者；GUI Tauri 仅 list/get/update/connections/network。
- 技术/业务约束：
  - 不改 SQLite schema；已有系统根 ensure 命中继续有效。
  - `No Approval, No Execute`：未批准不改代码。
- 已知风险：
  - 禁旁路会影响 TUI 手工建根与 AI 直写下游工具行为。
  - 方法重命名 alone 影响很小；行为收紧影响中等。

## Restated Understanding

- 我理解当前任务是：把已对齐的产品口径固化为 `NeuronManager` 对外 API 契约文档（sdd-light），供后续实现对照。
- 当前核心目标是：正门五动词清晰——`ensure_creator` / `select_*` / `create_neuron` / `ensure_system_neuron` / `bootstrap`；旁路不进业务正门。
- 当前边界是：只落 spec，不写 technical-plan 执行、不改代码。
- 暂不处理：运行时 JSON 裁决类 hook 的算法重构；预热策略实现。

## 接口契约设计

### 常量

```rust
const SYSTEM_CREATE: &str = "create_neuron";
const SYSTEM_SELECT: &str = "assistant_select_neuron";
const DEFAULT_N: usize = 7;
```

### 类型

```rust
struct CandidateQuery {
    n: usize,                    // 默认 7
    source_id: Option<String>,   // Some=直接下游；None=全域（含系统节点）
    min_new: usize,
}

enum CreateNeuronInput {
    Purpose(String),
    Messages(Vec<ModelMessage>),
}

struct EnsureSystemOpts {
    reset: bool,                 // false=幂等；true=删根后按统一流重建
}

struct BootstrapReport {
    create_neuron_id: String,
    select_neuron_id: String,
}

struct SystemPromptStatus {
    system_type: String,
    neuron_id: Option<String>,   // None=尚未 ensure
}

struct NeuronUpdate {
    desc: Option<String>,
    content: Option<String>,
}

// 实体沿用：Neuron / Connection / NeuronSubgraph
// 业务正门不暴露可写 system_type 的 NeuronCreate
```

### 正门 API

```rust
impl NeuronManager {
    // ── 底座（创建流的服务者）────────────────────────────────

    /// 确保创建提示词根存在。不调模型；配置/默认种子落库。幂等。
    fn ensure_creator(&self) -> AppResult<Neuron>;

    /// 凑候选：pool → n；不足则经创建流**一次批量**补齐（`generate_drafts(count=缺口)` + 挂到源下），禁止循环单条创建。
    /// 补齐不经 `create_neuron`→`select_one`（避免空池递归）；写稿 system 用 creator 种子或已有 creator 下游权重选一。
    /// 有 `source_id`：pool / 补齐目标 = **该源自己的直接下游**（不得借用其它根的下游）。
    /// 无源：全域；补齐落库为无上游普通节点。
    async fn select_candidates(&self, q: CandidateQuery) -> AppResult<Vec<Neuron>>;

    /// 选一：先按同一 `CandidateQuery` 凑候选，再用 SYSTEM_SELECT 裁决；
    /// 无 selector / LLM 失败 → 权重兜底（同权随机）。
    /// `source_id` 语义与 `select_candidates` 相同；**不**暗含 creator。
    async fn select_one(&self, q: CandidateQuery) -> AppResult<Neuron>;
    async fn select_one_from(&self, candidates: &[Neuron]) -> AppResult<Neuron>;

    // ── 统一创建流 ──────────────────────────────────────────

    /// 普通神经元（批量）：ensure_creator → 取创建用 system（creator 已有下游则选一；否则用 creator 种子，避免递归补齐）
    /// → 模型一次返回列表 → 落库（无 system_type）。
    /// `count` ∈ 1..=10；模型须返回 JSON 数组（count=1 时也可用单对象）。
    /// link_to=Some 则全部挂为该源直接下游。节点/边初始 weight=0。
    /// 当 `link_to == creator.id`（正在填充 creator 自己的下游）时，直接用 creator 种子作 system，不再 `select_one`。
    async fn create_neuron(
        &self,
        input: CreateNeuronInput,
        link_to: Option<&str>,
        count: usize,
    ) -> AppResult<Vec<Neuron>>;

    /// 系统提示词根（任意 system_type，含业务自定义）：
    /// - 已存在且 !reset → 在**本根**下 `select_candidates(n=7)` 补齐子池后返回；
    /// - 否则：用 creator 种子生成专用 content → 落库系统根 → 在**本根**下 `select_candidates(n=7)` 批量补齐直接下游。
    /// 禁止用其它根的下游充当本系统根的候选池；禁止用其它 API「贴」system_type。
    async fn ensure_system_neuron(
        &self,
        system_type: &str,
        opts: EnsureSystemOpts,
    ) -> AppResult<Neuron>;

    /// 启动完备：仅底座 —— ensure_creator + ensure_system_neuron(SYSTEM_SELECT)。
    /// 其它系统提示词（含未来自定义 system_type）一律由外部调用
    /// `ensure_system_neuron` 创建/补齐，不进 bootstrap。
    async fn bootstrap(&self) -> AppResult<BootstrapReport>;

    /// 运维：reset+重建已知 assistant_*（select/match/complete/score），再 bootstrap。
    /// 不重置 create_neuron 种子。
    async fn rebootstrap(&self) -> AppResult<BootstrapReport>;

    // ── 查询 / 图 / 权重 ───────────────────────────────────

    fn get(&self, id: &str) -> AppResult<Option<Neuron>>;
    fn get_by_system_type(&self, system_type: &str) -> AppResult<Option<Neuron>>;
    fn list(&self) -> AppResult<Vec<Neuron>>;
    fn connections(&self, id: &str) -> AppResult<Vec<Connection>>;
    fn network(&self, id: &str, max_depth: usize) -> AppResult<NeuronSubgraph>;
    fn adjust_weight(&self, id: &str, delta: f64) -> AppResult<Neuron>;
    fn adjust_edge_weight(&self, source: &str, target: &str, delta: f64) -> AppResult<Connection>;
    fn list_system_prompt_status(&self, types: &[&str]) -> AppResult<Vec<SystemPromptStatus>>;

    // ── 内容修订（不改变「如何创建」）──────────────────────

    /// AI：禁止更新 system_type 非空节点。
    fn update_content_for_ai(&self, id: &str, u: NeuronUpdate) -> AppResult<Neuron>;
    /// Admin：可改 content/desc；不能借此新建 system_type。
    fn update_content_for_admin(&self, id: &str, u: NeuronUpdate) -> AppResult<Neuron>;
}
```

### 调用关系（API 级）

```text
bootstrap()                              // 仅两底座
  └─ ensure_creator()
  └─ ensure_system_neuron("assistant_select_neuron")
        └─ generate(system=creator种子) + persist 系统根
        └─ select_candidates(source=本根, n=7)  // 不足则 create_neuron 批量补齐本根下游

ensure_system_neuron(任意 system_type)
  └─ 存在 → select_candidates(source=本根) 后返回
  └─ 缺失 → 同上：落根后在本根下游凑满 7

create_neuron(purpose|msgs, count=1..=10)
  └─ ensure_creator()
  └─ 取创建用 system：link_to==creator 或 creator 无下游 → 种子；否则 select_one(source=creator)
  └─ generate_drafts(count) 一次 + persist（可选 link_to）

select_candidates(source=X)
  └─ 取 X 直接下游；缺口一次 generate_drafts(count) + persist(link_to=X)

Assistant 选能力神经元
  └─ 首轮 select_one(source=assistant_select_neuron)
  └─ 次生 select_one(source=上一轮选中神经元)
```

### 消费方允许面

| 消费者 | 允许调用 |
| --- | --- |
| Gateway | `bootstrap`（可保留 `Gateway::bootstrap_neurons` 薄封装） |
| Assistant hooks | `ensure_system_neuron`、`select_one`、`adjust_weight`、查询 |
| AI tools | `create_neuron`、`select_candidates`、查询、`update_content_for_ai` |
| GUI | 现有查询/更新；可选 `list_system_prompt_status` / ensure（后续） |
| TUI 运维 | 查询、status、`bootstrap`、`ensure_system_neuron`、`update_content_for_admin` |

### 旧符号对照

| 现状 | 目标 |
| --- | --- |
| `create_generated` | `create_neuron` |
| `ensure_system_neuron` | **保留原名**（曾短暂改为 ensure_system_prompt，已改回） |
| `bootstrap_ready` | `bootstrap` |
| `ensure_creator_neuron` / `ensure_creator_for_admin` | `ensure_creator` |
| `create_for_admin` / `create_downstream`（可写 system_type） | **收敛**：业务正门删除；测试可用 Store 内部或窄测试 helper，不保留并行业务 API |
| `set_system_type_for_admin` | **收敛删除**；赋系统类型只许 `ensure_system_neuron` |
| AI `create_downstream_neuron`（直写 content） | **替换**：工具改为调 `create_neuron(..., link_to=source)`；不保留旧工具/旧实现 |

### 对外影响（评估摘要）

| 面 | 影响 |
| --- | --- |
| 跨包消费者 | 无 |
| DB / 已有数据 | 无 |
| Gateway / Assistant / CLI | 极小（重命名/薄封装） |
| GUI Tauri | 无～极小（可不改） |
| TUI `/neuron new`、贴 system_type | 中～大（收敛后改走 ensure / create_neuron） |
| AI `create_downstream_neuron` | 中（直接替换，无并存） |

收敛策略（已决）：旁路一刀切收敛；AI 工具直接替换，不留冗余兼容层。

## Open Questions

- [x] Q1 首次创建 `SYSTEM_SELECT` / 任意系统根：候选池在哪？
  - **2026-08-01 修正**：候选池 = **系统根自己的直接下游**；不足经 `create_neuron` 批量补齐。
  - 生成系统根 content 用 creator **种子**作写稿 system，不再把 creator 下游当成该系统根的 pool。
  - `select_one` 无 selector 时仍权重兜底。
- [x] Q2 旁路：`create_for_admin` / `set_system_type` 等？
  - **收敛**：不进业务正门，删除/收回，不做 deprecated 双轨。
- [x] Q3 AI `create_downstream_neuron`？
  - **替换**：改为 `create_neuron`；不要冗余旧代码/旧工具。
- [x] Q4 `bootstrap` 范围？
  - **仅底座**（creator + select）。允许外部通过 `ensure_system_neuron` 创建自有系统神经元（含未来未知 `system_type`）；不在 bootstrap 预热全部 assistant_*。

## Requirement Decisions

- 2026-08-01 02:52:
  - Q1–Q4 关闭，口径见上。
- 2026-08-01 02:53:
  - 用户批准执行。

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是——正门 API 与收敛策略已落地。
- 若否，偏差在哪里：—
- 是否需要调整本轮目标或范围：否。

## Checkpoint Summary

- 当前任务理解：按契约收敛 NeuronManager API。
- 当前核心目标：正门五动词 + 旁路收敛 + AI 替换 + 文档反写。
- 当前进度：实现完成；`cargo test --lib` 63 passed。
- 下一步 1: 无（本轮 Done Contract 已满足）。
- 涉及文件 / 模块：`neuron_manager.rs`、`models.rs`、`assistant_mode.rs`、`gateway.rs`、`tui/app.rs`、`lib.rs`、`neuron-init.md` 等。
- 风险：已有库内坏系统提示词仍需 `reset-system` 运维，非本轮范围。
- 验证方式：`CARGO_TARGET_DIR=.../target cargo test --lib` → 63 passed。
- Execution Approval: `Approved`

## Change Log

- 2026-08-01 02:40: 落盘标准 spec；固化 NeuronManager 正门 API、对照表与影响评估；开放问题 Q1–Q4。
- 2026-08-01 02:52: 关闭 Q1–Q4（保留选下游设计；旁路收敛；AI 替换；bootstrap 仅底座 + 外部可 ensure）。
- 2026-08-01 02:53+: 执行落地：重命名正门 API；删除 `create_for_admin`/`set_system_type_for_admin` 业务旁路；AI `create_neuron` 替换 `create_downstream_neuron`；反写 `neuron-init.md` 等。
- 2026-08-01 03:04: 补充运维 API/TUI：`rebootstrap`（全量 reset 已知 assistant_* + bootstrap）。
- 2026-08-01 03:38: 方法名改回 `ensure_system_neuron`（不用 ensure_system_prompt）。
- 2026-08-01 03:41: `create_neuron` 支持 `count` 1..=10，模型返回 JSON 列表，API 返回 `Vec<Neuron>`。
- 2026-08-01 04:27: Reverse Sync——`select_candidates` 补齐改一次批量 `create_neuron`；`ensure_system_neuron` 在本根下游凑池；Assistant 首轮选神经元 source=`assistant_select_neuron`。

## Validation

- Self-check: 调用面已无旧创建旁路符号。
- Static checks: `cargo test --lib` 编译通过。
- Runtime / Test: 63 passed, 0 failed。
- Human confirmation: 用户批准执行。
- 结果汇总: Done Contract 满足。
- 核心目标是否已由证据证明完成: 是。
- 若未完成，当前剩余差距: —
- 剩余风险: 旧库坏 content 需 reset；GUI 运维面板未做。

## Resume / Handoff

- 当前状态: 实现完成
- 当前卡点: 无
- 下一步唯一动作: 无（可选：坏系统提示词 reset、GUI 就绪面板）
- 下一轮核心目标: 按需处理运行时匹配质量或 GUI 运维
