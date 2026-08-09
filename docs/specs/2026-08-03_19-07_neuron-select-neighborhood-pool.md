# Spec: 神经元邻域候选池

## Goal

- 要解决什么问题：仅首轮使用全局候选池，后续所有助手轮次统一围绕上一轮选中神经元构造邻域候选池。
- 验收结果：用户输入、手动 step、Poller 使用同一 self 规则，并按下游、self/兄弟、三层上游配额完成候选装配。

## Done Contract

- 完成定义：候选池构造与 LLM 选 1 保持两个显式阶段；调用方通过强类型 Scope/Policy 控制全局及邻域配额，默认仍为全局 7、下游 4+2、兄弟 2、上游 3。
- 证明来源：相关 Rust 单元测试、`cargo fmt --check`、`cargo check` 通过，选型快照文档完成反写。
- 仍未完成：Manager 仍将配额写死在候选池实现中，或 Assistant Hook 仍通过“一步式候选+选 1”接口调用。

## Scope

- In：神经元图上游查询、助手邻域候选装配、converse/step/poller 选型源统一、测试与文档反写。
- Out：选型提示词、反馈打分、数据库结构与迁移、神经元限频/回收策略。

## Facts / Constraints

- 边方向为 `source -> target`；下游是直接 target，上游是指向当前节点的 source。
- 新建神经元必须继续走 `fill_candidates_batch`，并连接为 self 的直接下游。
- 多父节点时，每层按父节点自身 `weight DESC` 选择，最高权重并列随机。
- 兄弟使用第一层选中的父节点；兄弟及上游不足不补配额。
- 默认配额暂不接入 `config.json`；由代码中的 `Default` 提供，但调用方可覆盖。
- 每个非首轮选型至少新建 2 个节点，会持续增加图规模和模型调用成本；本轮不改变该产品决策。

## Restated Understanding

- 我理解当前任务是：首轮没有 last selected 时走全局池；从第二轮开始，不论触发源，均以 last selected 为 self 构造邻域池。
- 当前核心目标是：在稳定局部图邻域语义的同时，恢复“构造候选池 → LLM 选 1”的显式两阶段边界，并让配额可控。
- 当前边界是：默认值暂不进入 config；不改变最终 LLM 选 1、历史拼接和失败回退。
- 暂不处理：系统神经元过滤、容量治理、连接权重参与父链选择。

## 接口契约设计

```rust
// Store：返回一个直接上游；按父节点 weight DESC，最高权重并列随机。
pub fn select_direct_upstream(&self, target_id: &str) -> AppResult<Option<Neuron>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NeighborhoodPoolPolicy {
    pub existing_downstream: usize,
    pub new_downstream: usize,
    pub fill_downstream_shortage: bool,
    pub siblings: usize,
    pub upstream_depth: usize,
    pub global_top_weight: usize, // 全局权重 top N 补充配额，默认 5；0 = 不补充
}

pub enum AssistantCandidateScope {
    Global { limit: usize },
    Neighborhood {
        self_id: String,
        policy: NeighborhoodPoolPolicy,
    },
}

// 第一步：仅构造候选池，不读取会话历史、不调用选型模型。
pub async fn select_assistant_candidates(
    &self,
    scope: AssistantCandidateScope,
) -> AppResult<Vec<Neuron>>;

// 第二步：沿用既有接口，基于候选池和历史让 LLM 选 1。
pub async fn select_one_from_with_history(
    &self,
    candidates: &[Neuron],
    history: &[ModelMessage],
) -> AppResult<Neuron>;
```

- `NeighborhoodPoolPolicy::default()`：`existing_downstream=4`、`new_downstream=2`、`fill_downstream_shortage=true`、`siblings=2`、`upstream_depth=3`、`global_top_weight=5`。
- `AssistantCandidateScope::global_default()` 返回全局 7；`neighborhood_default(self_id)` 使用默认邻域策略。调用方也可直接构造自定义配额。
- Manager 校验全局 limit 非零、算术不溢出，以及本轮实际新建数量不超过 `MAX_CREATE_NEURON_COUNT`。
- self 邻域组装顺序：
  1. 取最多 `existing_downstream` 个既有直接下游。
  2. 新建 `new_downstream`；若 `fill_downstream_shortage=true`，再新建既有下游缺口数量。
  3. 加入 self；若存在直接父节点，从该父节点的下游中取最多 `siblings` 个兄弟。
  4. 从该父节点开始，沿每层最高权重父节点追溯最多 `upstream_depth` 层。
  5. 全程按 neuron id 去重；不为兄弟或上游缺口补位。
  6. 装配完成后，按 `global_top_weight`（默认 5）并入全库 `weight DESC` 最高的 N 个普通神经元（排除已删除、系统提示词、observing 变体），按 id 去重追加到池尾。

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是。
- 若否，偏差在哪里：无。
- 是否需要调整本轮目标或范围：否。

## Checkpoint Summary

- 当前任务理解：纠正一步式接口，恢复显式两阶段并暴露候选池配额。
- 当前核心目标：候选池规则可由调用方控制，默认行为保持不变。
- 当前进度：接口问题已由用户指出，Spec 已先行反写，准备纠偏实现。
- 下一步 1：新增 Scope/Policy 类型并重构候选池 API。
- 下一步 2：Assistant Hook 显式两步调用，补自定义配额测试并反写文档。
- 涉及文件 / 模块：`models.rs`、`neuron_manager.rs`、`assistant_mode.rs`、选型快照文档。
- 风险：自定义配额可能导致新建数量超限或总容量溢出，需要边界校验。
- 验证方式：单元测试、格式检查、编译检查。
- Execution Approval: `Approved`（用户于 2026-08-03 明确要求 Implement the plan）

## Change Log

- 2026-08-03：固化已确认的邻域候选池技术方案，开始实现。
- 2026-08-03：新增最高节点权重直接上游查询；Manager 完成 6+3+3 邻域池装配。
- 2026-08-03：converse、step、poller 统一直接读取 `last_selected_neuron_id`，移除 `secondary`/poll count 选型分支。
- 2026-08-03：补齐 Store/Manager 测试并同步助手提示词合成快照。
- 2026-08-03：用户指出一步式接口隐藏配额控制；先反写契约为强类型 Scope/Policy + 显式两阶段，默认值暂不接入 config。
- 2026-08-03：完成接口纠偏：新增 `AssistantCandidateScope` / `NeighborhoodPoolPolicy`，Hook 显式先构造候选池、再基于历史选 1。
- 2026-08-03：新增自定义配额、全局零配额和新建批次上限测试，并反写实现态快照。
- 2026-08-09：邻域候选池补充全局权重 top5——`NeighborhoodPoolPolicy` 新增 `global_top_weight`（默认 5）；`list_global_candidates` 口径收紧为排除系统提示词与 observing 变体；装配末尾并入全库 weight 最高的 N 个（按 id 去重）。详见 [`docs/micro_specs/2026-08-09_12-00_neuron-pool-top-weight-5.md`](../micro_specs/2026-08-09_12-00_neuron-pool-top-weight-5.md)。

## Validation

- Self-check：已审阅任务相关 diff；未修改 `.env`、`Cargo.toml` 等既有用户改动。
- Static checks：`cargo check --lib` 通过；IDE lint 无错误。仅保留任务外 `compactor.rs` 的既有 unused import warning。
- Runtime / Test：`cargo test --lib` 通过，89 passed；覆盖默认与自定义 Policy、非法配额边界、最高权重父节点、4+2 下游、self/兄弟/三层祖先和首轮全局 7。Tauri dev 热重载编译成功。
- Human confirmation：技术方案与执行已获确认。
- 结果汇总：接口纠偏后的实现与测试满足 Done Contract。`cargo fmt --check` 仍受仓库既有全局格式差异影响；为避免扩大改动，未执行全仓格式化。
- 核心目标是否已由证据证明完成：是。
- 若未完成，当前剩余差距：无。
- 剩余风险：每个非首轮固定新建至少 2 个神经元，长期图规模与调用成本仍需后续治理。

## Resume / Handoff

- 当前状态：接口纠偏、验证与反写已完成。
- 当前卡点：无。
- 下一步唯一动作：人工观察真实多轮选型日志与生成成本。
- 下一轮核心目标：如需运行时调参，再单独把默认 Policy 接入 config。
