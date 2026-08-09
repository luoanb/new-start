# Spec: 邻域候选池补充全局权重 Top5

## Goal

- 要解决什么问题：非首轮助手选型使用 last selected 的邻域池（下游/self/兄弟/三层上游），该池按 `weight DESC, RANDOM()` 局部取数，**不保证全局高权重神经元进入候选**；而 LLM 选 1 只能从候选池内挑，导致长期高分节点在后续轮次"失联"，选型质量与权重反馈脱钩。
- 验收结果：邻域候选池在既有装配结果之上，**再并入全局 weight 最高的 5 个神经元（按 id 去重）**；高权重节点在任意轮次都有机会被 LLM 选中；既有全局池（首轮 7）与通用 `select_candidates` 行为不变；`cargo test` / `cargo check` 通过。

## Done Contract

- 完成定义：
  1. `NeighborhoodPoolPolicy` 新增 `global_top_weight: usize`（默认 `5`），调用方可覆盖（0 = 不补充）。
  2. `select_neighborhood_candidates` 在既有装配（下游/self/兄弟/祖先）完成后，追加 `store.list_global_candidates(policy.global_top_weight, &selected_ids)`，按 id 去重后并入候选池尾部。
  3. `list_global_candidates` 口径收紧为与邻域池一致：`deleted_at IS NULL` + `system_type IS NULL`（系统提示词不参与业务候选）+ `variant_state IS NULL OR variant_state != 'observing'`（观察中变体不参与），排序仍为 `weight DESC, RANDOM()`。
  4. 溢出校验链同步纳入 `global_top_weight`。
- 由什么证明：新增单测（高权重节点并入邻域池 + 去重 + 0 不补充）；既有邻域池数量断言测试保持通过（图中节点 weight 均为 0 且已在池内，top5 去重后不新增）；`cargo test --lib`、`cargo check` 通过。
- 哪些情况仍算未完成：全局池/通用 `select_candidates` 不额外补 top5（其本身按 weight DESC 取 N，已覆盖）；补充数量不做 config 化（沿用 Policy 可覆盖，默认值暂不进入 `config.json`）。

## 背景与根因

- 现有装配顺序（[spec 2026-08-03_19-07](file:///home/lab/Documents/trae_projects/new-start-wt/docs/specs/2026-08-03_19-07_neuron-select-neighborhood-pool.md)）：首轮全局 7；非首轮 = 既有下游（≤4）+ 新建（2，缺口补齐）+ self + 兄弟（≤2）+ 三层最高权重上游（≤3），全程按 id 去重。
- 缺口：邻域池完全由"与 self 的图距离"决定，与全局权重无关；一个被反复打分的高权重神经元若不在 self 邻域内，就永远不会出现在候选池中，选型模型无从选中它。
- 用户决策：仅助手邻域池补充全局 top5（全局池已按 weight DESC，无需补）；top5 语义 = 全局 weight 最高 5 个去重并入（排除已删除/系统神经元/观察中变体）。

## 接口契约设计

```rust
// models.rs
pub struct NeighborhoodPoolPolicy {
    pub existing_downstream: usize,
    pub new_downstream: usize,
    pub fill_downstream_shortage: bool,
    pub siblings: usize,
    pub upstream_depth: usize,
    pub global_top_weight: usize,   // 新增：全局权重 top N 补充配额，默认 5
}

// neuron_manager.rs select_neighborhood_candidates 末尾追加：
if policy.global_top_weight > 0 {
    let top = self.store()?.list_global_candidates(policy.global_top_weight, &selected_ids)?;
    for neuron in top {
        if selected_ids.insert(neuron.id.clone()) {
            selected.push(neuron);
        }
    }
}

// neuron_store.rs list_global_candidates（口径收紧）：
SELECT id, desc, content, weight, system_type, tool_ids, created_at, updated_at,
       use_count, last_used_at, deleted_at
FROM neurons
WHERE deleted_at IS NULL
  AND system_type IS NULL
  AND (variant_state IS NULL OR variant_state != 'observing')
ORDER BY weight DESC, RANDOM()
```

## 改动点

| 文件 | 改动 |
|---|---|
| `src-tauri/src/core/models.rs` | `NeighborhoodPoolPolicy` 新增 `global_top_weight` 字段 + `Default = 5` |
| `src-tauri/src/core/neuron_manager.rs` | 邻域装配末尾并入全局 top5（去重）；`_maximum_pool_size` 溢出链纳入新字段；补单测 |
| `src-tauri/src/core/neuron_store.rs` | `list_global_candidates` SQL 增加 `system_type IS NULL` 与 observing 变体过滤 |
| `docs/specs/2026-08-03_19-07_neuron-select-neighborhood-pool.md` | Change Log 反写本次变更 |
| `docs/agent-app/assistant-prompt-synthesis.md` | 4.3 默认邻域池描述补充"另并入全局权重 top5（去重）" |

## 兼容性

- 全局池（首轮 7）与通用 `select_candidates` 行为不变；`list_global_candidates` 仅收紧为排除系统神经元/观察中变体（与邻域口径统一，属行为修正而非回归）。
- 既有 `NeighborhoodPoolPolicy { ... }` 结构字面量调用点（测试两处）需补 `global_top_weight` 字段；默认构造 `neighborhood_default` 自动获得 5。
- 候选池尾部追加 5 个高权重节点 → 邻域池实际数量 = 原装配 ∪ top5（去重），模型 payload 略增，选型 prompt 语义不变。

## Validation

- `cargo test --lib`：新增用例——
  - 高权重孤立节点（不在邻域内）被并入邻域候选池；
  - 已入选节点去重（top5 与邻域重叠时不重复）；
  - `global_top_weight: 0` 时不补充；
  - `list_global_candidates` 排除系统神经元与 observing 变体；
  - 既有邻域池数量/结构断言保持通过。
- `cargo check`：0 error。
- 手动验证：App 内多轮对话，观察 `select_assistant_candidates` 日志中候选池包含全局高分节点。

## Change Log

- 2026-08-09：初始 micro-spec。决策：仅助手邻域池补充全局 weight top5（去重并入，尾部追加）；`list_global_candidates` 口径收紧（排除系统/观察中变体）；`global_top_weight` 入 Policy 默认 5，暂不进 config。
- 2026-08-09（实现）：`NeighborhoodPoolPolicy` 新增 `global_top_weight`（默认 5）+ 溢出校验链纳入；`select_neighborhood_candidates` 装配末尾并入 `list_global_candidates(n, &selected_ids)` 去重追加；`list_global_candidates` SQL 增加 `system_type IS NULL` 与 observing 变体过滤。新增单测：高权重孤立节点并入邻域池、`global_top_weight: 0` 不补充、`list_global_candidates` 排除系统/观察中变体。`cargo check` 0 error；`cargo test --lib` 150 passed。
