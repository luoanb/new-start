# Spec: 自述性原子能力（Insert Catalog · 方案 2）

## Goal

- 要解决什么问题：助手 hook / 模型原子缺少面向**模型读者**的「工具说明书」；说明书应写清工具职责与对模型输出的期待（标准格式），并与系统 neuron `content` 拼接后喂给模型。
- 验收结果：
  1. 生产对外 Tool 仍空注册；`register` ⇒ 必须有 insert（规范保留）。
  2. Insert 只覆盖**有决策型模型读者**的原子；正文是工具说明书（非实现说明、非无读者内部步骤）。
  3. Hook / 选型 / 草稿等模型调用处：`system = neuron.content + insert`（拼接顺序见契约）；禁止 `require` 后丢弃正文。
  4. 课题相关说明书以 **ScopeIn + Done Contract** 为输出核心（任务拆解）。

## Done Contract

- 什么算完成：
  1. Spec + `architecture.md` 写明：读者规则、工具说明书写法、拼接消费、本轮 insert 名单。
  2. 砍掉无读者文档：`neuron.ensure_system` / `neuron.bootstrap_system`（删 md + 去假挂载）。
  3. 本轮 insert 全部按「工具说明书」重写，并在对应 `call_model` 前与系统 neuron content **拼接消费**。
  4. 课题创建/匹配路径：模型输出契约含 ScopeIn（goal + done_contract）任务拆解；代码能消费该标准格式（相对现状若缺口则补齐最小接线）。
  5. `cargo test --lib` 通过。
- 由什么证明：prompt 组装审查 + insert 抽样（含课题 ScopeIn/Done Contract）+ 测试输出。
- 哪些情况仍算未完成：对外 Tool 重注册；把 hook 改成 ToolRegistry tool calling；热更新 inserts。

## Scope

- In:
  - Insert 内容范式纠偏为**工具说明书**（工具本身 + 对模型的期待/标准输出）
  - Hook 决策原子说明书 + 与系统 neuron content 拼接
  - `neuron.select_one` / `neuron.draft_from_model` 同上（模型读者）
  - 删除 ensure/bootstrap 假 insert
  - 课题：创建/拆解输出聚焦 `scope_in[].goal` + `scope_in[].done_contract`
  - 空对外注册与 register 门禁（已有则保持）
- Out:
  - 为无决策读者的内部自动步骤写 insert
  - 说明书写成架构/实现注释
  - Gateway 锁、全量删死 Tool 实现（可选）

## Facts / Constraints

- 已确认事实：
  - Hook 决策点：`score_feedback` / `match_topic` / `select_one`(LLM) / `complete_scope`；`ensure_*` 与落库执行不是决策。
  - Hook 模型调用现只用系统 neuron `content`；insert 需与之**拼起来**再喂模型。
  - 现状 `match_topic` 创建课题多半只建空壳 topic，**尚未**强制模型返回 ScopeIn/Done Contract → 本轮要按说明书契约补齐期待与最小消费。
  - `ScopeInItem` 已有字段：`goal`、`done_contract`（见 `models.rs`）。
  - 用户确认（2026-08-01 晚）：范围按 hook 决策更新；拼接消费；说明书聚焦工具与对模型期待；课题示例以 ScopeIn+Done Contract 做任务拆解。
- 技术约束：
  - 拼接默认：`system_prompt = concat(neuron.content, "\n\n", insert_body)`（insert 在后，强化输出契约；若实测需对调，Change Log 回写）。
  - 有决策型模型读者才配 insert；代码硬调代码不配。
- 已知风险：
  - prompt 变长；neuron content 与 insert 职责重叠 → insert 只写工具契约，角色文案留在 neuron。
  - 补齐课题创建消费可能改 hook 行为（空壳 → 带 scope_in）。

## Open Questions

- [x] Q1–Q6：见既有结论（方案 2、空对外注册、register 门禁等）。
- [x] Q7 ensure/bootstrap insert？ → **砍**（无决策读者）。
- [x] Q8 hook insert 怎么喂？ → **与系统 neuron content 拼接**。
- [x] Q9 说明书写什么？ → **工具说明书**：工具是什么 + 对模型的标准输出期待。
- [x] Q10 课题核心？ → **ScopeIn + Done Contract** 任务拆解。
- [x] Q11 拼接顺序？ → **content 前、insert 后**（已实现）。

## Restated Understanding

- 我理解当前任务是：把自述做成**给模型用的工具说明书**；hook 决策原子与选型/草稿原子要有 insert；喂模型时与系统 neuron content 拼接；课题工具以 ScopeIn+Done Contract 为输出核心；删掉无读者假文档。
- 当前核心目标是：名单、写法、拼接消费、课题契约对齐代码。
- 当前边界是：不重做 ToolRegistry 对外工具；不把 ensure/bootstrap 当自述原子。
- 暂不处理：全量对外工具上架。

## 范式（约定）

### 何时配 insert

有**决策型模型读者**才配。人选命令面若将来点名调用同一原子，可复用同一说明书。

### 说明书写什么（工具说明书）

聚焦两件事，其它少写：

1. **工具本身**：这个原子/工具解决什么决策或产出什么。  
2. **对模型的期待**：必须返回的**标准格式**、字段含义、对错边界。

栏目建议：

```markdown
# <id>

## 工具
（一句话：做什么决策/产出）

## 对模型的期待
（标准输出：JSON/字段表；必填；禁止项）

## 忌用
（什么情况不应调用或不应乱填）

## 注意
（与上下游工具关系、失败时系统行为）
```

**反例（禁止）：** 写内部锁、文件路径、ensure 流程、给无人读的启动编排说明。

### 课题示例（内容取向）

课题创建/拆解类工具：模型返回的核心不是散文摘要，而是任务拆解——

- `scope_in[]`：每项含 `goal` + `done_contract`（Done Contract）  
- 可选：课题名/描述等外壳字段  
- 目标：可执行、可验收的范围项，供后续 `complete_scope` 对照勾选  

### 消费（拼接）

```text
system_prompt = neuron.content  +  "\n\n"  +  InsertCatalog::get(id)
user_prompt   = 原有 payload / 候选 / 用户输入（不变，除非契约要求）
→ call_model(system_prompt, user_prompt)
```

- `call_system_prompt_json(system_type, …)`：ensure 出 neuron 后，拼接对应 insert 再调模型。  
- `try_llm_select`：selector neuron content + `neuron.select_one` insert。  
- `generate_drafts`：传入的 system（常为 creator/选中 neuron content）+ `neuron.draft_from_model` insert。

### id 与方案 2

- Hook 原子：`assistant.<动作>`（与 system_type 语义对齐，文件名用点分 id）。  
- Neuron 模型原子：`neuron.<动作>`。  
- 布局仍为 `src-tauri/inserts/<id>.md` + `InsertCatalog`。

## 本轮 Insert 名单

| id | 读者（模型） | 消费位置 | 说明书核心期待（摘要） |
|----|--------------|----------|------------------------|
| `assistant.score_feedback` | 打分模型 | `call_system_prompt_json(score_feedback)` 拼接 | 返回非 0 的 `score`∈[-5,5] |
| `assistant.match_topic` | 匹配/创建课题模型 | `call_system_prompt_json(match_topic)` 拼接 | `action`；**创建时**产出课题拆解：`scope_in`（goal+done_contract）等标准结构 |
| `assistant.complete_scope` | 勾选完成模型 | `call_system_prompt_json(complete_scope)` 拼接 | `completed_item_ids` 对照已有 ScopeIn |
| `neuron.select_one` | 选型模型 | `try_llm_select` 拼接 | 从候选中返回合法 `neuron_id` |
| `neuron.draft_from_model` | 草稿模型 | `generate_drafts` 拼接 | 标准草稿 JSON（desc/content/tool_ids…） |

**砍掉：** `neuron.ensure_system`、`neuron.bootstrap_system`。

## 架构调整（相对首轮偏差）

```text
错误（首轮）:
  require(id) → 丢弃；ensure/bootstrap 也写 md

正确（本轮）:
  inserts = 工具说明书（对模型的输出契约）
  call_model 前: system = neuron.content + insert
  仅决策型模型原子保留 insert
```

## 接口契约设计

```rust
// 组装系统提示（概念）
fn system_with_insert(neuron_content: &str, insert_id: &str) -> String {
    let insert = InsertCatalog::require(insert_id); // 必须存在且返回正文
    format!("{neuron_content}\n\n{insert}")
}

// Hook:
// call_system_prompt_json(system_type) {
//   let neuron = ensure_system_neuron(system_type);
//   let insert_id = map_system_type_to_insert(system_type);
//   let system = system_with_insert(&neuron.content, insert_id);
//   call_model(system, user_payload)
// }
```

`map_system_type_to_insert` 示例：

| system_type | insert id |
|-------------|-----------|
| `assistant_score_feedback` | `assistant.score_feedback` |
| `assistant_match_topic` | `assistant.match_topic` |
| `assistant_complete_scope` | `assistant.complete_scope` |
| `assistant_select_neuron`（选型 LLM） | `neuron.select_one` |

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是 — 重执行已对齐拼接与工具说明书。
- 偏差：已消除首轮假挂载/无读者文档。
- 是否需要调整范围：否。

## Checkpoint Summary

- 当前任务理解：工具说明书 + content∥insert 拼接；5 个模型原子；课题 ScopeIn+Done Contract。
- 当前核心目标：重执行落地拼接与课题消费。
- 当前进度：已重执行（用户「重新执行」）。
- 下一步 1: （完成）5 份工具说明书；删 ensure/bootstrap。
- 下一步 2: （完成）拼接消费 + match create 落 scope_in。
- 下一步 3: （完成）architecture / 本 spec 回写；跑测。
- 涉及文件：`inserts/`、`insert_catalog.rs`、`assistant_mode.rs`、`neuron_manager.rs`、`architecture.md`。
- 风险：模型未按 JSON 返回时 create 失败（符合契约）。
- 验证方式：`cargo test --lib`。
- Execution Approval: `Approved`（重新执行）

## Change Log

- 2026-08-01: 方案 2 + 空对外注册 + 首轮偏差实现。
- 2026-08-01: 重梳 + 范围更新（拼接、工具说明书、课题 ScopeIn）。
- 2026-08-01: **重执行完成**——5 insert 重写；`system_with_insert`；hook/select/drafts 拼接；match create 消费 scope_in；砍 ensure/bootstrap md。

## Validation

- Self-check: 假 insert 已删；拼接 API 已用；create 要求 scope_in
- Static checks: 编译通过
- Runtime / Test: `cargo test --lib` → **70 passed**
- Human confirmation: 重新执行已获
- 结果汇总: Done Contract 本轮项已满足
- 核心目标是否已由证据证明完成: **是**
- 若未完成，当前剩余差距: —
- 剩余风险: 模型不遵守 JSON/scope_in 时 create 会失败（预期）

## Resume / Handoff

- 当前状态: 重执行完成
- 当前卡点: 无
- 下一步唯一动作: 无（或按需打磨系统 neuron content 与 insert 分工）
- 下一轮核心目标: （新任务）按需上架有用对外工具
