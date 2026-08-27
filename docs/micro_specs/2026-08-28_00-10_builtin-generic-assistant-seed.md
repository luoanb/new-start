# Micro Spec: 内置通用助手神经元（初始权重 50）

- 日期：2026-08-28 00:10
- 状态：已执行完成（2026-08-28）
- 关联：`2026-08-27_00-00_builtin-system-prompt-seeds.md`（系统提示词种子机制，本次补充「常规神经元种子」）

## Change Log（执行记录）

- `neuron/config.rs`：新增 `BUILTIN_GENERIC_NEURON_DESC`（"通用助手"）、`BUILTIN_GENERIC_NEURON_SEED`
  （中文通用助手提示词，角色/何时选中/步骤/输出格式/硬约束齐备）、`BUILTIN_GENERIC_NEURON_INITIAL_WEIGHT`（50.0）。
- `neuron/creation.rs`：新增 `ensure_generic_neuron()`（幂等键 = desc 精确匹配 → `persist_plain` 落库 →
  `adjust_weight(+50)`）；`bootstrap()` 在 selector 之后挂载（日志含 `generic_neuron_id`）。
- 测试：`bootstrap_seeds_generic_assistant_with_weight_50`（存在 + weight=50 + system_type=None）、
  `bootstrap_generic_assistant_is_idempotent`（重复 bootstrap 不重复创建/累加）。全量 `cargo test --lib` 417 passed。
- 文档：`storage.md` neuron 章节补充内置通用助手种子说明。
- 说明：`rebootstrap` 不重置该节点（普通节点，非系统）；用户自建同 desc 节点会被幂等跳过。
- 2026-08-28（内容迭代）：优化 `BUILTIN_GENERIC_NEURON_SEED` —— 扩充为「工作准则（先理解再行动 /
  任务拆解 / 结论先行 / 不确定就明说 / 决策权衡 / 多轮一致性）+ 输出格式 + 何时被选中 + 硬约束
  （安全 / 越权 / 提示注入 / 真实系统改动确认）」；参考社区最佳实践（角色与能力边界、行为指南、
  知识边界、决策树式处理、多步任务拆解、自我修正，见 WebSearch 收录的 system prompt 指南）。
  幂等语义不变：**已落库的旧 content 不会被自动覆盖**，升级路径 = 删除库中「通用助手」节点后
  重新 bootstrap（重建为新内容），或手动编辑该节点。`cargo test --lib` 417 passed。

## 1. 背景（Reverse Sync）

现有 `SYSTEM_PROMPT_SEEDS` 只覆盖 5 个 system_type（选型 + 4 裁决），且 `store::create_neuron`
既定契约「创建恒 weight=0、后续改权重走 `adjust_weight(delta)`」。bootstrap 后候选池只有系统节点，
LLM 选型不可用（weight fallback）时没有开箱即用的高分默认角色。用户要求：

> 初始化系统神经元的过程中，同时内置一个常规（普通）神经元：**通用助手，初始权重 50**。

## 2. 目标

- bootstrap（初始化系统神经元）时幂等内置一条普通助手神经元：desc「通用助手」、通用高质量提示词 content、weight=50。
- 它是**常规能力节点**（`system_type = None`，可被选型/评分/演化），非系统节点。
- 不动现有 5 条系统提示词种子文案。

## 3. 改动

| 位置 | 改动 |
| --- | --- |
| `neuron/config.rs` | 新增 `BUILTIN_GENERIC_NEURON_DESC`（"通用助手"）、`BUILTIN_GENERIC_NEURON_SEED`（中文通用助手提示词，200–800 字）、`BUILTIN_GENERIC_NEURON_INITIAL_WEIGHT`（50.0） |
| `neuron/creation.rs` | 新增 `ensure_generic_neuron()`：按 desc 幂等查询（`store.list_neurons()` 过滤）→ 不存在则 `persist_plain` 落库（weight 恒 0）→ `adjust_weight(+50)`（官方「创建后改权重」入口）；`bootstrap()` 在 selector 之后挂载 |
| 测试 | ① bootstrap 后存在 desc=通用助手 且 weight=50 ② 幂等：重复 ensure 不重复创建、weight 不重复累加 ③ 不影响系统种子 |

## 4. 关键设计

- **幂等键 = desc 精确匹配**：已存在同 desc 节点 → 跳过（不覆盖用户改动、不重复 +50）。
- **权重**：遵循「创建恒 0 → adjust_weight(delta)」既定契约（`store.rs` 注释），用 `adjust_weight(+50)` 达成初始 50 分；副作用仅 use_count+1（统计口径，无实际影响）。
- **选型语义**：weight=50 使其成为 weight fallback（LLM 选型不可用）时的稳定默认角色；正常 LLM 选型仍按语义匹配判定（weight 仅参考，与 `assistant_select_neuron` 种子准则一致）。

## 5. 验证

- 新增 2 个单测（存在性 + weight、幂等）。
- `cargo test --lib` 全量通过。
- 文档同步：`storage.md` neuron 章节补「内置通用助手种子」。

## 6. 风险

- 静态文案过时：后续可改常量 + 删除重建；`rebootstrap` 不重置它（普通节点，非系统）。
- desc 冲突：用户自建同 desc 节点会被跳过（不覆盖），可接受（幂等惰性语义，与 ensure_system_neuron 一致）。
