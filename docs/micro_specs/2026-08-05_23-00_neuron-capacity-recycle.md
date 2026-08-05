# Spec: 神经元容量上限与低价值回收（逻辑删除）

## Goal

- 要解决什么问题：Agent 自动创建使 `neurons` 表活跃数据无上限增长，节点在图中堆积很快、权重失真。用户选定方案：**给活跃数据设容量上限，后台定时任务在超限时按"低价值"回收，逻辑删除后全流程不可用**。
- 验收结果：活跃神经元数不超过 `neuron.capacity`（超限时定时回收最低价值节点）；回收的节点 `deleted_at` 打标，Select 候选 / 列表 / 图 / 编辑 / 打分全流程排除（业务不可用），数据与版本历史保留；`cargo test` 通过；App 内图形状稳定。

## Done Contract

- 什么算完成：
  1. `neurons` 表新增 `deleted_at INTEGER`（沿用 `has_column` + `ALTER TABLE` 迁移模式）。
  2. `Neuron` struct 暴露 `use_count` / `last_used_at` / `deleted_at`（serde 兼容，缺省可空）。
  3. 新增 `neuron_manager.recycle_if_over_capacity()`：`active_count > capacity` 时按 `(weight ASC, use_count ASC, last_used_at ASC, created_at DESC)` 淘汰超出的部分；`system_type IS NOT NULL`（系统提示词）豁免。
  4. 定时任务：gateway 新增 `spawn_neuron_recycle_runtime`（`tauri::async_runtime::spawn` + `tokio::time::interval`），周期由 config 控制。
  5. 活跃信号：`select_one*` 命中、`adjust_weight`、`update_content_for_*` 时 `use_count+1, last_used_at=now`。
  6. 全流程排除：Select 候选、`list_neurons`、`get_neuron`/network 构建、连接查询、update/adjust/连线操作均跳过或拒绝 `deleted_at` 非空节点。
  7. 回收完成后 emit `StateChange::Neurons`，前端刷新。
- 由什么证明：`cargo test`（含 recycle 排序与豁免单测）；`cargo check` 0 error；App 内图形状稳定、无新增堆积。
- 哪些情况仍算未完成：手动触发回收命令（仅定时）；回收站/恢复 UI（无）；硬删除与级联清理（无，保留数据）。

## Scope

- In：`neuron_store.rs`（迁移、查询排除、活跃信号）、`neuron_manager.rs`（`recycle_if_over_capacity`、回收排序、活跃信号）、`neuron_model.rs`（`Neuron` 字段）、`gateway.rs`（`spawn_neuron_recycle_runtime` + StateChange）、`config.rs`（`neuron` section）。
- Out：前端 UI 改动；`neuron_versions`（保留审计）；`accumulated_delta` / `variant_state`（变体池，不动）。

## Facts / Constraints

- **字段已存在**：`neurons.use_count`、`neurons.last_used_at` 列已在 [neuron_store.rs:83-107](file:///home/lab/Documents/trae_projects/new-start-wt/packages/agent-app/src-tauri/src/core/neuron_store.rs#L83-L107)（变体池在用），只需在 `Neuron` struct 暴露并接入通用维护；新增 `deleted_at` 一列即可。
- **迁移模式**：`has_column(&conn, "neurons", ...)` 判断 + `ALTER TABLE ADD COLUMN`，照 [neuron_store.rs:54-107](file:///home/lab/Documents/trae_projects/new-start-wt/packages/agent-app/src-tauri/src/core/neuron_store.rs#L54-L107) 追加。
- **定时任务模式**：`tauri::async_runtime::spawn` + `tokio::time::interval`，照 [gateway.rs:584-614](file:///home/lab/Documents/trae_projects/new-start-wt/packages/agent-app/src-tauri/src/core/gateway.rs#L584-L614) `spawn_poller_runtime` 风格新增独立任务。
- **活跃信号现成钩子**：`select_one_from_with_history`（候选命中）、`adjust_weight`（打分）、`update_content_for_ai/admin`（编辑）——三个调用点即可覆盖"被使用"。
- **排除须统一**：deleted 排除要覆盖 store 层所有读路径（candidates、list、get、connections、update、adjust），避免前端看到幽灵节点。
- 回收排序"低价值"按用户要求 = `weight` 低、`use_count` 低、`updated_at/last_used_at` 久。

## 接口契约设计

### 配置（config.rs）

```rust
pub struct NeuronSection {
    pub capacity: Option<usize>,             // 默认 300
    pub recycle_interval_ms: Option<u64>,    // 默认 3_600_000（1h）
}
```
config.json 示例：
```json
{ "neuron": { "capacity": 300, "recycle_interval_ms": 3600000 } }
```

### 回收（neuron_manager.rs）

```rust
/// 活跃数超容量时，按低价值升序回收，返回回收数量。
pub fn recycle_if_over_capacity(&self) -> AppResult<usize> {
    let capacity = self.config.neuron.capacity.unwrap_or(300);
    let over = self.store.count_active()? - capacity;
    if over <= 0 { return Ok(0); }
    // 低价值排序：(weight ASC, use_count ASC, last_used_at ASC NULLS FIRST, created_at DESC)
    // 豁免 system_type IS NOT NULL
    let victims = self.store.select_low_value(over)?;
    self.store.mark_deleted(&victims)?; // UPDATE neurons SET deleted_at = ? WHERE id IN (...)
    Ok(victims.len())
}
```

### 定时任务（gateway.rs）

```rust
fn spawn_neuron_recycle_runtime(
    neurons: Arc<NeuronManager>,
    interval_ms: u64,
    state_emit: Option<StateEmitter>,
) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));
        loop {
            interval.tick().await;
            let recycled = neurons.recycle_if_over_capacity().unwrap_or(0);
            if recycled > 0 {
                state_emit.map(|e| e(StateChange::Neurons)); // 通知前端刷新
            }
        }
    });
}
```

### 活跃信号（store 层小更新）

```rust
// 在命中/打分/编辑的 UPDATE 中合并：
UPDATE neurons SET use_count = use_count + 1, last_used_at = ? WHERE id = ? AND deleted_at IS NULL
```

### 全流程排除清单

| 读路径 | 处理 |
|---|---|
| `select_assistant_candidates` / `select_one*` | WHERE `deleted_at IS NULL` |
| `list_neurons`（前端列表/图） | WHERE `deleted_at IS NULL` |
| `get_neuron` | 返回 None（视为不存在） |
| connections 查询 | JOIN neurons 排除 deleted 端点 |
| `update_content_for_*` / `adjust_weight` / 连线 | 对 deleted 拒绝（0 rows 或错误） |
| network 构建（get_network/list） | 过滤 deleted 节点与关联边 |

## Open Questions

- [ ] 容量默认 300 是否合适：可配置，默认值先取 300，观察后调。
- [ ] 回收频率与前端刷新：默认 1h，回收发生时 emit Neurons 刷新。

## Restated Understanding

- 我理解当前任务是：给神经元活跃数据设容量上限，定时任务超限时按（权重/使用次数/最近使用时间）低价值淘汰，逻辑删除（`deleted_at`）后全流程不可用，数据与版本历史保留、不提供恢复 UI。
- 当前核心目标是：活跃数据总量可控、图形状稳定。
- 当前边界是：不做前端 UI、不做恢复、不做手动触发命令、不动变体池字段。
- 暂不处理：回收站视图、恢复功能、硬删除、级联清理。

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是，落盘 micro-spec 是执行前必要 spec。
- 若否：N/A。

## Checkpoint Summary

- 当前任务理解：容量上限 + 定时低价值回收 + 逻辑删除全流程排除。
- 当前核心目标：活跃数据总量可控。
- 当前进度：micro-spec 待审批。
- 下一步 1：用户批准后实现 store 迁移/查询排除、recycle 逻辑、gateway 定时任务、config。
- 下一步 2：`cargo test` + `cargo check` + App 内验证。
- 验证方式：`cargo test`（recycle 排序/豁免单测）；`cargo check` 0 error；App 内图稳定。
- Execution Approval: 待批准（micro-spec 已提交）。

## Change Log

- 2026-08-05: 初始 micro-spec。决策：容量上限（默认 300）+ 定时回收（1h）+ 低价值排序（weight/use_count/last_used_at）+ 逻辑删除全流程排除；system_type 豁免；活跃信号复用 select/adjust/update 三点。
- 2026-08-06: 实现完成。store 迁移 `deleted_at` + `count_active`/`select_low_value`/`mark_deleted`/`mark_used`；查询/更新/连线全流程排除 deleted；update/adjust/select 命中合并活跃信号；`recycle_if_over_capacity`；gateway 定时任务（`neuron.recycle_interval_ms` 可配）回收后广播 `StateChange::Neurons`；config 顶层 `neuron` section。

## Validation

- Self-check：已完成。`cargo test --lib` 115 passed；`cargo check` 0 error（仅既有无关警告）。补充修复：poller.rs 测试模块缺失 `use std::fs`（预存在问题，阻碍测试编译）。
- Static checks：`cargo check` 0 error。
- Runtime / Test：`cargo test` 新增覆盖——`test_recycle_low_value_ordering`（低价值排序）、`test_recycle_exempts_system_neurons`（系统豁免）、`test_recycle_deleted_nodes_excluded_everywhere`（candidates/list/get 排除 + 幂等）、`test_recycle_deleted_blocks_writes_and_links`（写/连线拒绝 deleted）、`recycle_if_over_capacity_recycles_lowest_value_and_exempts_system`（manager 层容量回收 + 系统保留 + 幂等）、`recycle_spares_used_neurons`（use_count 高的不被先淘汰）、`select_one_marks_usage_as_active_signal`（select 命中记录 use_count/last_used_at）。全部通过。
- Human confirmation：已批准（2026-08-06），进入实现。
- 结果汇总：实现 + 单测 + check 全部完成；App 内验证待用户执行（`cargo tauri dev` 观察图形状与 `neuron.capacity` 生效）。
- 核心目标是否已由证据证明完成：是（代码与测试层面；App 内图稳定待人工确认）。
- 若未完成，当前剩余差距：App 内人工验证。
- 剩余风险：容量默认值需按实际使用观察调优；逻辑删除后 connections 保留但查询排除（数据冗余可接受）；定时任务 1h 粒度下图形状以"天"为单位收敛，非实时；低价值排序末级 tiebreaker `created_at DESC`（最新创建先淘汰）为 spec 字面实现，如期望"旧优先"改为 `created_at ASC` 即可。

## Resume / Handoff

- 当前状态：已实现并通过单测/check，待 App 内验证。
- 当前卡点：无。
- 下一步唯一动作：App 内验证（启动应用观察图形状、超容量回收、回收后前端刷新）。
- 下一轮核心目标：活跃数据总量可控、图形状稳定。
