# Technical Plan / 技术方案: 创建提示词自我迭代

## Restated Understanding / 方案复述

- 目标：为「创建提示词」（creator，`system_type=create_neuron`）建立自我迭代闭环：**常驻 N=7 变体 + 观察位 2 + 评分归因 + 门槛触发差分重写 + 退化回滚 + 手动锁定**。
- 地基：`lineage_parent_id` 把"子神经元的评分"归因到"生成它的变体"，使变体获得可靠权重信号。
- 明确不做：creator 本体 content 自动改写；评分 delta 语义变更；存量数据迁移；GUI `create_neuron_plain` 路径。

## Current Project Facts / 当前项目事实

代码落点（`packages/agent-app/src-tauri/`）：

- 表结构 `core/neuron_store.rs:36-72`：`neurons(id, desc, content, weight, created_at, updated_at, system_type, tool_ids)` + `connections(source, target, weight)`；迁移方式为 `has_column` + `ALTER TABLE`。
- `ensure_creator` `core/neuron_manager.rs:653`：缓存 → DB（按 system_type）→ 种子新建；幂等，落库后 content 永不变（固定根因）。
- `select_candidates` `core/neuron_manager.rs:186-282`：`min_new>0` 预填 → 复用 `list_direct_downstream`（`ORDER BY weight DESC, RANDOM()`）→ 不足补足到 n。
- `fill_candidates_batch` `core/neuron_manager.rs:847`：池空时用 `creator.content` 当 system 生成候选；否则按权重挑已有候选当 system。
- `select_one` / `select_one_from_with_history` `core/neuron_manager.rs:425-459`：`try_llm_select` 失败 → `pick_by_weight` 兜底。
- `create_neuron` `core/neuron_manager.rs:463-504`：`link_to==creator.id`（filling_creator）时直接用 `creator.content`；否则 `select_one(n=7, source_id=creator, min_new=0)` 选 1 变体 → `create_neuron_user_prompt` → `generate_drafts` → `persist_plain` 逐条落库。
- `persist_plain` `core/neuron_manager.rs:989-1003`：`system_type=None`、`weight=0`；`link_to=Some` → `create_downstream_neuron`（建边），`None` → `create_neuron`。
- 评分归因点 `core/assistant_mode.rs:1043-1126`（`score_feedback_hook`）：对 `intervention_neuron_ids` 逐条 `adjust_weight(delta)` + `adjust_connection_weight`；**当前不触及 content、不触及父变体**。
- 手动编辑 `lib.rs:332`（`update_neuron` → `update_content_for_admin`）；`NeuronUpdate{desc, content}`。
- 前端创建唯一入口 `NeuronManager.svelte` 的 `create_neuron_plain`（GUI 不走 LLM 批量创建，故 creator 池恒空——与"无子项"现象一致）。
- 已有测试佐证 `select_candidates` 会把 creator 池补足到 7：`neuron_manager.rs:1733`（`select_candidates_under_creator_returns_seven`）。

## Decision / 方案决策

### 对比

| 方案 | 做法 | 优点 | 缺点 |
|---|---|---|---|
| A. 只恢复候选池（策略 1 简化） | `ensure_creator` 末尾补 `ensure_own_candidate_pool`，7 选 1 | 改动最小 | 只是"多态"，内容仍不更新，非真正迭代 |
| B. 归因 + 累积信号内容回写（推荐） | lineage 归因 → 使用/评分统计 → 门槛触发差分重写 → 观察/回滚 | 正向有据、成本可控、可回滚 | 需加列与版本表；信号链路长需窗口聚合 |
| C. 全量多臂老虎机进化池 | 每次创建加权随机 + 频繁重写全部变体 | 探索最充分 | 成本高、稳定性差、实现量大 |

### 推荐：方案 B（归因 + 累积信号内容回写），吸收 A 的"变体池"与 C 的"加权随机选择"

依据：
- 用户明确否定"无可靠权重评价"的现状，B 通过 lineage 归因补上这一地基。
- B 的"更新参考数据"三层（表现 / 失败 / 场景）正是用户提出的"确保更新是正向的"所需。
- 与现有 `ORDER BY weight DESC, RANDOM()` 加权随机选择天然兼容，改动集中在 Manager/Store，不破坏 insert 契约与 Tauri command。

### 核心机制

1. **归因**：`create_neuron` 落库时把选中变体 id 写入子神经元 `lineage_parent_id`；`score_feedback_hook` 对每个介入神经元，若存在 `lineage_parent_id`，则对父变体 `accumulated_delta += delta`（并保留对介入神经元本身的既有评分）。
2. **统计**：`create_neuron` 选中变体后 `record_variant_usage(variant_id)`：`use_count+1`、`last_used_at=now`。
3. **触发门槛**（满足才更新，一次只更新 1 个）：
   - 重写候选：`use_count >= 3` 且 `|accumulated_delta| >= 2`。
   - 淘汰候选：`accumulated_delta <= -3`，或 `use_count >= 10` 且 `accumulated_delta < 0`。
4. **差分重写**：新 insert `creator.variant_evolve.md`，输入 `{current_content, usage_contexts, score_history, child_stats, failure_signals}`，要求只改失效部分、保留有效段落、输出与原 JSON 契约一致（`desc/content/tool_ids`）。
5. **观察位与回滚**：新变体 `variant_state='observing'` 不参与选择；下个周期转正需 `use_count >= 1`；若评估周期内 `accumulated_delta < 0` 则回滚旧版（`neuron_versions` 表可查历史）。
6. **手动锁定**：`update_content_for_admin` 置 `manual_edited=1`，锁定后不参与自动重写与淘汰。
7. **失败兜底**：重写 LLM 调用失败 → 保留旧版、失败计数 +1、跳过本轮；DB 错误静默跳过，不影响创建主流程。

## API Design / API 设计（内部契约变更）

- `neurons` 表新增列（全部 nullable/默认值，兼容存量）：
  - `lineage_parent_id TEXT`（生成来源：变体 id 或 creator id）
  - `use_count INTEGER NOT NULL DEFAULT 0`
  - `accumulated_delta REAL NOT NULL DEFAULT 0`
  - `last_used_at INTEGER`
  - `variant_state TEXT`（`'active'` / `'observing'`；NULL 视为 active）
  - `manual_edited INTEGER NOT NULL DEFAULT 0`
- 新表 `neuron_versions(id, neuron_id, content, source, created_at, prev_version_id)`：`source` ∈ {`seed`, `evolve`, `rollback`}。
- Store 新方法：`get_variants(creator_id, active_only)`、`increment_variant_usage`、`accumulate_variant_delta`、`set_variant_state`、`set_manual_edited`、`insert_neuron_version`、`latest_version_of(neuron_id)`。
- Manager 新方法：`record_variant_usage`、`maybe_evolve_creator_variants()`（评分 hook 后调用）、`rollback_variant_if_regressed`。
- `score_feedback_hook`：在既有 `adjust_weight` 循环内追加父变体归因。
- `select_candidates` 的 `list_direct_downstream` 查询追加 `variant_state != 'observing'` 过滤（仅对 creator 池生效，其余调用不变）。

## Execution Steps / 执行步骤

1. `core/neuron_store.rs`：`init_table` 迁移新列 + 建 `neuron_versions` 表；实现上述 Store 方法；`create_downstream_neuron` 支持写入 `lineage_parent_id`/`variant_state`。
2. `core/neuron_manager.rs`：
   - `create_neuron`：落库时写 `lineage_parent_id`（filling_creator → creator.id；否则 → 选中变体 id）；选中变体后 `record_variant_usage`。
   - `persist_plain` / `create_downstream_neuron` 调用链透传 `lineage_parent_id`。
   - 新增 `maybe_evolve_creator_variants`：门槛检查 → 差分重写 1 个 → 观察位 → 评估/回滚。
   - `ensure_creator`：补 `ensure_own_candidate_pool`（容量 7+2，含观察位），首启时填充种子变体池。
3. `core/assistant_mode.rs`：`score_feedback_hook` 追加父变体归因 + 触发 `maybe_evolve_creator_variants`。
4. 新增 insert：`inserts/creator.variant_evolve.md`（差分重写契约，见 Decision 第 4 条）。
5. `lib.rs`：`update_neuron` 置 `manual_edited=1`。
6. 单测（`core/neuron_manager.rs` tests）：归因、门槛触发、观察位转正、退化回滚、手动锁定、存量 NULL lineage 兼容。
7. `cargo test`（`packages/agent-app/src-tauri`）全量通过后 Reverse Sync：回写 `docs/sdd-lab` 本迭代 `lifecycle.md` 及 `bootstrap` / `neuron-system-prompt-ready` 相关文档。

## Risk And Mitigation / 风险与缓解

| 风险 | 缓解 |
|---|---|
| 评分信号稀疏、延迟（创建→启用→评分链路长） | 累积窗口聚合（`accumulated_delta` + `use_count`），不依赖单次评分 |
| 重写质量差导致提示词退化 | 差分重写（只改失效部分）+ 一次换一个 + 版本回滚 + 观察位不参与选择 |
| LLM 成本上升 | 门槛节流 + 一次只更新 1 个；创建路径不触发 |
| 存量数据兼容 | 新列全部 nullable/默认值；`lineage_parent_id IS NULL` 视为种子直生 |
| 自动迭代覆盖用户手动修改 | `manual_edited=1` 锁定，优先级最高 |
| 重写失败拖垮创建流程 | 失败保留旧版 + 失败计数，静默跳过，不影响主流程 |

## Open Questions / 开放问题

- 观察位的严格性：方案采用 `variant_state` 字段显式排除观察位参与选择；若你倾向"零新列、靠低权重自然观察"，可改走最小路径。
- 触发时机仅挂钩评分 hook（对话评分时评估一次），是否需要在创建流程后也挂一次评估？默认：仅评分 hook，避免创建路径增加延迟。

## Execute Checkpoint / 执行检查点

- 前置：`requirements.md` 已确认；本方案（尤其 Decision 推荐方案 B、N=7+2、门槛数值、观察位机制）请用户确认。
- 批准后进入 `executing`，按 Execution Steps 执行；执行中若发现方案不可行，先回写本文档再回退状态。
- 下一步唯一动作：用户确认本方案或提出修改意见。
