# Lifecycle / 生命周期: 创建提示词自我迭代

```yaml
status: done
result: done
created_at: 2026-08-04 21:49
updated_at: 2026-08-04 22:40
owner: user
```

## Current Summary / 当前摘要

- 批准状态：用户已确认 technical-plan.md 方案 B（归因+常驻更新，N=7+2，门槛 use_count>=3 且 |delta|>=2，一次更新 1 个，退化回滚，manual_edited 锁定），进入执行
- 当前状态：done（实现与单测全部完成，97 个测试通过）
- 当前核心目标：让「创建提示词」节点及其候选变体池具备自我迭代能力（常驻 N=7 变体 + 持续更新，更新依据评分归因 / 使用统计，保证正向而非随机）
- 结果：creator 自我迭代闭环已落地并通过全量测试；实现偏差已通过 Reverse Sync 回写（见下）

## Transition Log / 状态流转

- 1. 2026-08-04 21:49: `- -> planned`。需求经多轮对话对齐（策略 2 为主：N 常驻 + 持续更新；策略 1 的数量增长被吸收为探索机制），用户指示"出方案吧（按 sdd-light 轻量版）"。落盘 `requirements.md` 与 `technical-plan.md`。
  - 依据：sdd-lab `Requirement Before Plan`；需求边界已在对话确认。
  - 下一步：等待用户确认技术方案。
- 2. 2026-08-04 22:05: `planned -> executing`。用户确认方案 B 无修改，批准 Execute Checkpoint。
  - 依据：No Plan Approved, No Execute；用户明确"确认，开始执行"。
  - 下一步：按 Execution Steps 执行（store 迁移 → manager 归因/演化 → hook 归因 → insert 契约 → lib 锁定 → 单测 → cargo test → Reverse Sync）。
- 3. 2026-08-04 22:40: `executing -> done`。全部 7 步完成，`cargo test` 97 个测试全量通过（新增 8 个自迭代相关测试）。
  - 依据：Validation 通过；实现偏差按 Reverse Sync 规则回写本文件。

## Execution Log / 执行记录

- Step 1: `core/neuron_store.rs` 迁移与 Store 方法（完成）
  - neurons 表新增列：`lineage_parent_id TEXT`、`use_count INTEGER NOT NULL DEFAULT 0`、`accumulated_delta REAL NOT NULL DEFAULT 0`、`last_used_at INTEGER`、`variant_state TEXT`、`manual_edited INTEGER NOT NULL DEFAULT 0`（`has_column` + ALTER 迁移，存量数据兼容）
  - 新增 `neuron_versions` 表（id/neuron_id/content/source/created_at/prev_version_id）
  - 新增 Store 方法：`get_variants` / `increment_variant_usage` / `accumulate_variant_delta` / `set_variant_state` / `set_manual_edited` / `insert_neuron_version` / `latest_version_of` / `lineage_parent_id_of`
  - `create_downstream_neuron` 写入 `lineage_parent_id` / `variant_state`；`list_direct_downstream` 过滤 `variant_state='observing'`
- Step 2: `core/neuron_manager.rs` 归因与演化（完成）
  - `create_neuron`：seed 分支 lineage 指向 creator；变体分支先 `record_variant_usage` 再落库，child 记录 `lineage_parent_id`
  - 新增 `record_variant_usage` / `accumulate_variant_delta` / `maybe_evolve_creator_variants` / `rollback_variant_if_regressed` / `rewrite_variant` / `ensure_own_candidate_pool`
  - `bootstrap` 首启补 creator 池（7 个 active 变体）
  - `update_content_for_admin` 统一置 `manual_edited=1`（覆盖 GUI `lib.rs` 与 TUI 两条编辑路径）
- Step 3: `core/assistant_mode.rs` 评分 hook（完成）
  - `score_feedback_hook` 循环中追加 lineage 归因：`lineage_parent_id_of` + `accumulate_variant_delta`
  - 循环结束后调用 `maybe_evolve_creator_variants`，失败仅 warn，不打断反馈流
- Step 4: `inserts/creator.variant_evolve.md`（完成）
  - 差分重写契约：仅允许 `{"content": ..., "desc": ...}` JSON，保留有效段落、职责不变、失败兜底
- Step 5: `lib.rs` 手动编辑锁定（完成）
  - 无需改 lib.rs：在 `update_content_for_admin`（manager 层）统一置锁，两条调用路径（GUI/TUI）同时覆盖
- Step 6: 单测（完成，新增 8 个）
  - `create_neuron_attributes_lineage_and_usage`（归因 + use_count + last_used_at）
  - `create_neuron_filling_creator_links_lineage_to_creator`（seed 分支）
  - `observing_variant_promotes_after_use_and_rolls_back_on_regression`（观察位转正/退化）
  - `active_variant_rewrites_at_threshold_and_moves_to_observing`（门槛触发差分重写 + 版本归档）
  - `manual_edited_variant_is_locked_from_rewrite_and_elimination`（锁定）
  - `eliminated_variant_rolls_back_to_archived_version`（淘汰回滚 + rollback 版本）
  - `legacy_null_variant_state_is_treated_as_active`（NULL 兼容）
  - `admin_update_marks_variant_manual_edited`（管理员编辑置锁）
- Step 7: `cargo test` 全量通过 + Reverse Sync（完成）
  - lib 97 / 97 通过，无失败

## Reverse Sync / 实现偏差记录

与 technical-plan.md 的偏差（已按 `Spec is Truth` 判定为实现合理简化，回写于此）：

1. **观察位槽位**：方案"容量 7+2（含观察位）"。实现为：`ensure_own_candidate_pool` 只预建 7 个 active 变体；观察位由差分重写/退化动态产生（重写后 `variant_state='observing'`），不预建空槽。`select_candidates` 已过滤 observing，池容量自洽。
2. **重写输入字段**：方案理想输入 `{current_content, usage_contexts, score_history, child_stats, failure_signals}`。实现 payload 为 `{current_desc, current_content, current_tool_ids, use_count, accumulated_delta, last_used_at, parent_creator_content}`（取 store 实有数据，未新增失败信号列）。
3. **失败计数**：方案"重写失败计数 +1、跳过本轮"。实现仅 warn + 跳过（保留旧版，不影响主创建流程），未新增失败计数字段。
4. **锁定实现位置**：方案 Step 5 指向 `lib.rs update_neuron`。实现收敛到 `update_content_for_admin`（manager 层），GUI/TUI 编辑路径同时置 `manual_edited=1`，语义一致且更全。

## Validation / 验证

- 验收标准见 `requirements.md`。
- `cargo test`（packages/agent-app/src-tauri）：lib 97/97 通过，含 8 个新增自迭代测试。
- 关键场景均已覆盖：lineage 归因、门槛触发、观察位转正/退化、淘汰回滚、manual_edited 锁定、存量 NULL 兼容、重写失败兜底。
