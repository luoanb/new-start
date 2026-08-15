# Spec: 模选后回挂边（source → target）

## Goal

- 要解决什么问题：模选（LLM 选 1）的候选池是邻域池，可命中兄弟、祖先或全局 topN 补充节点，这些节点**不是候选池锚点（source）的直接下游**；选中后当前轮只写入 `last_selected`，**不建立任何图关联**，导致被复用节点与锚点之间缺乏显式边，图语义与"消费即关联"脱节。
- 验收结果：模选完成后，若 target 不是 source 的直接下游，则自动新建一条 `source → target` 边（权重 0，幂等）；source 为候选池锚点（`Neighborhood` 的 `self_id` = last\_selected，首轮 = 发起神经元）；首轮 Global（无锚点）跳过；既有选型结果、`mark_used`、权重回退行为不变；`cargo test --lib` / `cargo check` 通过。

## Done Contract

- 完成定义：

  1. `store` 新增直接下游存在性检查 `connection_exists(source, target) -> AppResult<bool>`（`SELECT COUNT(*) FROM connections WHERE source=? AND target=?`）。
  2. 模选命中 target 后，执行回挂规则：`source` 为 None → 跳过；`source == target.id` → 跳过（不自环）；`connection_exists` 为真 → 跳过（幂等）；否则 `store.link(source, target.id, 0.0)`。
  3. 覆盖**所有模选入口**：`select_one_with_history` / `select_one`（锚点 = `query.source_id`）、`select_role`（锚点 = scope 的 Neighborhood `self_id`；Global → None）、`select_one_from` / 外部直接调用 `select_one_from_with_history`（无锚点 → None，不建边）。
  4. 单节点短路路径（`candidates.len() == 1`，未真正调模选）不建边，行为与现状一致。

- 由什么证明：新增单测（锚点命中非直接下游 → 新建边；已存在边 → 不重复；target==source → 不自环；锚点 None → 不建边；Global 首轮 → 不建边）；既有选型测试保持通过；`cargo test --lib`、`cargo check` 通过。

- 哪些情况仍算未完成：仅显式回挂 source→target 一条边；不做多父回挂、不做 config 化开关、不改变权重语义（新边恒 0）。

## 背景与根因

- 现有流程（[spec 2026-08-03_19-07](file:///home/lab/Documents/trae_projects/new-start/docs/specs/2026-08-03_19-07_neuron-select-neighborhood-pool.md) + [micro_spec 2026-08-09 top5](file:///home/lab/Documents/trae_projects/new-start/docs/micro_specs/2026-08-09_12-00_neuron-pool-top-weight-5.md)）：`select_role` 先装配候选池（既有下游/新建下游/self/兄弟/祖先/全局 topN），再 `select_one_from_with_history` 让 LLM 选 1；命中后仅 `mark_used` + 写 `state.last_selected_neuron_id`。

- 缺口：兄弟、祖先、全局 topN 候选与锚点之间**不一定有直接边**（兄弟是父节点的下游、祖先是上游、topN 甚至与锚点无路径）。选中它们后，锚点并没有在图上"捕获"该能力，下次邻域池仍可能不含该节点。

- 用户决策（2026-08-15）：模选完成后，若 target 不是 source 的直接下游，新建 `source → target` 边。source = 候选池锚点；覆盖所有模选入口；首轮 Global 无锚点则跳过。

## 术语澄清（与 2026-08-15 术语统一一致）

- **发起神经元**：用户选中发起会话的神经元（`spec_neuron_id` 锚定，`SessionSeed::Neuron(id)`）；会话首轮邻域池锚点 = 发起神经元自身。
- **候选池锚点（source）**：`Neighborhood` scope 的 `self_id`——非首轮 = `last_selected`，首轮 = 发起神经元。
- 本 spec 中"规格神经元"一律不再使用（已随 [micro_spec 2026-08-15_remove-assistant-dialogue-and-terminology](file:///home/lab/Documents/trae_projects/new-start/docs/micro_specs/2026-08-15_remove-assistant-dialogue-and-terminology.md) 移除）。

## 接口契约设计

```rust
// store.rs：直接下游存在性检查（回挂规则前置判断）
pub fn connection_exists(&self, source: &str, target: &str) -> AppResult<bool>;

// selection.rs：模选入口携带可选锚点；命中后回挂
pub(crate) async fn select_one_from_with_history(
    &self,
    candidates: &[Neuron],
    history: &[ModelMessage],
    link_source: Option<&str>,   // 新增：回挂边锚点（候选池 self_id）
) -> AppResult<Neuron>;

// 内部回挂规则（选中结果确定后、返回前统一调用一次）
fn maybe_link_to_source(&self, source: Option<&str>, target: &Neuron) -> AppResult<()> {
    let Some(source) = source else { return Ok(()); };
    if source == target.id { return Ok(()); }                    // 不自环
    if self.store()?.connection_exists(source, &target.id)? { return Ok(()); } // 幂等
    let _ = self.store()?.link(source, &target.id, 0.0)?;        // 新边恒权重 0
    Ok(())
}
```

各入口锚点来源：

| 入口 | 锚点 | 说明 |
|---|---|---|
| `select_one_with_history` / `select_one` | `query.source_id` | 有源时模选命中即为源直接下游，通常空操作；幂等安全 |
| `select_role` | scope 的 `Neighborhood { self_id, .. }`；`Global` → None | 核心生效路径（兄弟/祖先/topN 命中 → 回挂锚点） |
| `select_one_from` / 外部直接调用 `select_one_from_with_history` | None | 调用方无锚点信息，不建边 |

## 改动点

| 文件 | 改动 |
|---|---|
| `src-tauri/src/core/neuron/store.rs` | 新增 `connection_exists(source, target)` |
| `src-tauri/src/core/neuron/selection.rs` | `select_one_from_with_history` 增加 `link_source: Option<&str>`；`select_one_with_history` 传 `query.source_id`；`select_one_from` 传 None；新增私有 `maybe_link_to_source` 并在命中后调用 |
| `src-tauri/src/core/neuron/manager.rs` | `select_one_from_with_history` 转发加参；`select_role` 从 scope 提取锚点传入 |
| `src-tauri/src/core/neuron/manager/tests.rs` | 新增回挂边单测 |
| `docs/specs/2026-08-03_19-07_neuron-select-neighborhood-pool.md` | Change Log 反写本次变更 |

## 兼容性

- `select_one_from_with_history` 签名变化（新增 `Option<&str>` 参数）：manager 转发与 selection 内部调用点同步更新；参数为 None 时行为与现状完全一致。

- 新边权重恒 0，`link` 为 `INSERT OR REPLACE`，重复回挂不产生副作用、不覆盖既有权重变化路径（`adjust_connection_weight` 不受影响）。

- 不改选型结果、`mark_used`、LLM 失败权重回退；`last_selected` 写入逻辑不变。

- 已知取舍：选中系统神经元（如 selector/creator 进入候选时）也会被回挂，不额外过滤——不改变当前"邻域池不排除系统节点"的口径，保持最小改动；如后续需要可单独加过滤。

## Validation

- `cargo test --lib`：新增用例——

  - 锚点命中非直接下游（兄弟/孤立 topN）→ 新建 `source → target` 边；

  - 已存在直接边 → 不重复插入（`connection_exists` 跳过）；

  - `target == source` → 不自环；

  - 锚点 None（`select_one_from` / Global 首轮）→ 不建边；

  - 既有选型/候选池测试保持通过。

- `cargo check`：0 error。

- 手动验证：App 内多轮对话，观察选中非直接下游节点后的日志出现回挂边事件。

## Change Log

- 2026-08-15：初始 micro-spec。决策：模选后若 target 非 source 直接下游则新建 `source → target` 边（权重 0，幂等）；source = 候选池锚点（Neighborhood self\_id = last\_selected / 首轮发起神经元）；覆盖所有模选入口；首轮 Global 无锚点跳过；单节点短路路径不建边。
- 2026-08-15：重梳。同步 2026-08-15 术语统一——「规格神经元」改为「发起神经元」；明确 source = 邻域池 `self_id`（非首轮 last\_selected / 首轮发起神经元）；补充术语澄清小节与接口锚点来源表。
