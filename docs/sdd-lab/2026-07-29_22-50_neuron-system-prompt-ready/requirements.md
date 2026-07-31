# Requirements / 需求文档: 神经元系统提示词自举完备

## Restated Understanding / 需求复述

- 我理解当前需求是：在已有神经元自举与 Assistant 模式之上，让**神经元子系统**能不依赖业务侧预置，自举到「关键系统提示词可用」；把 **pool → 7 → 1** 的选神经元能力收束到 `NeuronManager`，Assistant 只调用；并提供「获取系统提示词，缺失则补齐」。
- 当前核心目标是：启动即可保证 `create_neuron` + `assistant_select_neuron` 可用；其它系统提示词由业务按需调用补齐方法创建。
- 当前边界是：改动以 `NeuronManager` / 神经元存储与配置为主，Assistant 改为消费新 API；不重做课题/Poller/工具权限模型。
- 暂不处理：为所有 `assistant_*` 在启动时一次性创建；在 `engine.rs` 内实现选神经元逻辑；给普通下游节点打与根相同的 `system_type`。

## Background / 背景

- 前置迭代：`docs/sdd-lab/2026-07-28_23-43_neuron-bootstrap/`（候选选择、`create_neuron` ensure）、`docs/sdd-lab/2026-07-26_21-30_assistant-mode/`（Assistant 消费固定 `system_type` 提示词）。
- 现状缺口：Assistant 对 `assistant_*` 仅 `get_by_system_type`，缺失即失败；7 选 1 裁决逻辑在 Assistant 内；`select_candidates` 仍带 `system_type` 解析来源。
- 目标：神经元图内闭环完成「系统提示词根节点」的创建与选择，业务只调 Manager API。

## Glossary / 术语

- **pool（候选池）**：进入「凑 7 → 选 1」之前抽取/补齐所用的神经元集合。
  - 有 `source_id`：仅该源的**直接下游**（下游本身有无 `system_type` 均保留，不另做过滤）。
  - 无源：**全域神经元**，**不排除**带 `system_type` 的系统节点。
- **pool → 7 → 1**：从 pool 取出/补齐到默认 7 个候选，再选出 1 个胜者。
- **`select_neuron_candidates`**：现有 AI 工具名，内部调用 `select_candidates`（凑候选，不是 7 选 1 裁决）。

## Scope / 范围

### In

- 将 **pool → 7 → 1**（默认凑 7 再选 1）封装为 `NeuronManager` 方法；Assistant 及其它业务只调用，不复制裁决流程。
- 7 选 1 在找不到 `assistant_select_neuron`，或 LLM / 解析失败时，启用备用规则：**权重高优先，同权重随机**。
- 新增系统启动初始化：先 `create_neuron` 自举，再经 pool → 7 → 1（必要时走备用规则），**优先 ensure 出 `assistant_select_neuron`**；多次调用幂等。
- 新建/补齐系统提示词采用策略 **C**：在 pool → 7 → 1 得到胜者后，**再调一次模型**，生成该 `system_type` 专用提示词 content，再落库。
- `create_neuron` 自举完成后，对外提供「创建普通神经元」方法：先通过 pool → 7 → 1 取得创建用提示词（来自 `create_neuron` 链路），再调模型生成并落库；入参为**待创建用途**或**对话上下文 msgs**。
- 新增「获取系统提示词（缺失则补齐）」API：已存在则返回；缺失则走补齐流程。业务侧首次需要其它 `assistant_*` 时可**同步阻塞**调用 init/ensure（幂等）。
- 默认 ensure **跳过已存在**的系统提示词；提供**重置**：删除该系统提示词根及其**仅一级子节点相关边**（及按重置范围处理的一级子节点，见决策），再重建；其它系统提示词不动。
- `system_type` 只标记**链路起始根节点**；其子节点不打同名 `system_type`。
- 图形态允许多根、无上游、游离节点；带 `system_type` 的系统提示词原则上是根节点。
- 启动 init **仅**保证 `assistant_select_neuron`；其它由业务 ensure。
- 有源取直接下游、无源取全域且**不排除系统节点**（已写入 pool 规则）。
- `select_candidates` **去掉 `system_type` 参数**；需要某根下候选时先拿根 id 再传 `source_id`。
- 全空库时：配置可覆盖；**代码内必须有默认 `create_neuron` 种子文案**，保证无配置也能写入第一颗系统神经元（文案可后续人工替换）。

### Out

- 启动时批量创建全部 `assistant_*` 系统提示词。
- 修改课题 / Poller / 工具注册表权限模型。
- 要求每个普通神经元都必须有上下游。
- 允许同一 `system_type` 对应多个神经元（仍保持全局唯一）。
- 在 `engine.rs` 实现选神经元或系统提示词补齐。

## Capability Contract / 能力契约（需求级）

### 1. 选一（pool → 7 → 1）

- 输入：可选 `source_id`，或已给定候选集合；默认先凑齐 7 再选 1。
- **pool 规则**：有源 → 仅直接下游；无源 → **全域神经元（含系统节点，不排除）**。
- 有可用的 `assistant_select_neuron` 时：以其 content 为 system，候选为上下文，LLM 返回选中 id。
- 无该系统提示词，或 LLM/解析失败：按权重降序，同权重随机选 1。
- 输出：选中的普通候选神经元。

### 2. 创建普通神经元（依赖 `create_neuron` 已自举）

1. 在 `create_neuron` 约定 pool 上执行 pool → 7 → 1，得到「创建用」提示词胜者（或其生成的创建提示词，见实现与 Q3 已定方向）。
2. 调用模型创建新神经元。
3. 入参：`purpose`（待创建用途）或对话上下文 `msgs`。
4. 落库为普通神经元（无 `system_type`）；若有来源则连为直接下游。

### 3. 确保系统提示词根

- 若 `system_type` 已存在且非重置：直接返回。
- 若缺失（或重置后）：
  1. 准备候选池并凑到 7；
  2. 「选一」得胜者；
  3. 再调模型生成专用 content（可走「创建神经元」能力，用途=该 `system_type` 的系统提示词）；
  4. 创建带该 `system_type` 的根并返回。

### 4. 启动初始化（幂等）

1. 确保 `create_neuron`（配置优先，否则代码默认文案）。
2. pool → 7 → 1（可无裁决提示词，走权重兜底）。
3. ensure `assistant_select_neuron`（已存在则跳过）。
4. 不自动创建其它 `assistant_*`。

### 5. 重置系统提示词

- 删除指定 `system_type` 根，以及与其相连的**一级边**；一级子节点**保留**（只断边，不删节点）。
- 然后按 ensure 流程重建该根；胜者与新根之间不自动建边。
- 其它 `system_type` 根不受影响。

### 6. `select_candidates` 调整

- 删除 `system_type` 查询字段/语义；只保留 `n` / `source_id` / `min_new`。
- pool 规则与选一相同：有 `source_id` → 直接下游；无源 → 全域。
- AI 工具 `select_neuron_candidates` 同步删除 `system_type` 参数。

## Default Seed / 默认种子文案（可后续替换）

代码内默认 `create_neuron` content（保证能产出可解析 JSON，并约束 `content` 为可执行的单职责提示词）：

```text
You are the Neuron Creator for an agent app.
A neuron is a reusable capability node. Its `content` will later be used as system/knowledge text for selection and execution — write it to be executed, not marketed.

Return ONLY one JSON object (no markdown fences, no commentary):
{"desc":"string","content":"string","weight":0.0,"tool_ids":["string"]}

Field rules:
- desc: ≤20 chars Chinese/English label of a single responsibility (verb+noun preferred).
- content: a complete, self-contained prompt/knowledge block that includes:
  1) Role & goal
  2) When this neuron should be selected / when not
  3) Procedure or decision steps
  4) Output format / success criteria
  5) Hard constraints (what not to do)
  Prefer 200–800 Chinese characters (or equivalent). Avoid slogans, placeholders, and vague advice.
- weight: finite number; use 1–3 for niche helpers, 4–6 for common skills, 7–10 for foundational routers/policies.
- tool_ids: only tools truly required for this role; otherwise []. Never invent tool names.

Quality bar:
- One neuron = one job. Do not merge unrelated responsibilities.
- Be concrete enough that another model can follow `content` without extra context.
- If the purpose is underspecified, make the safest useful specialist and state assumptions inside `content`.

Example (style only; do not copy blindly):
{"desc":"需求澄清","content":"你是需求澄清助手。当用户目标模糊时启用；目标已足够具体时可跳过。步骤：1) 用一句话复述目标 2) 列出缺口信息 3) 最多问 3 个关键问题 4) 给出可执行的下一版需求摘要。输出结构：目标/约束/待确认/下一步。禁止直接写实现代码或跳过澄清。","weight":6.0,"tool_ids":[]}
```

## Acceptance Criteria / 验收标准

- [x] `NeuronManager` 对外提供可测的「选一（pool → 7 → 1）」；Assistant 不再内联裁决。
- [x] 缺失或 LLM 失败时，选一降级为权重优先 + 同权随机；仅当池空且无法补齐时失败。
- [x] 无配置时仍可用代码默认文案创建第一颗 `create_neuron`。
- [x] 启动初始化（可多次）后存在可用的 `create_neuron` 与 `assistant_select_neuron`（入口会尝试 bootstrap；失败可警告后由 ensure 补救）。
- [x] 「获取系统提示词」缺失可补齐；已存在幂等返回；支持重置后重建。
- [x] 重置只影响目标系统根及其一级关联边（保留子节点），其它系统提示词不动。
- [x] 创建普通神经元 API：入参 purpose 或 msgs；内部先取创建提示词再调模型落库。
- [x] 系统提示词根带唯一 `system_type`；直接下游无强制同名 `system_type`。
- [x] `select_candidates` 不再使用 `system_type`。
- [x] Assistant 取 `assistant_*` 走新 API；需要时阻塞 ensure，不再因未预置直接查找失败。

## Constraints / 约束

- 本次迭代先对齐需求文档；未确认前不改代码。
- 无强制新增外部 crate；继续复用现有模型调用与 SQLite。
- `system_type` 全局唯一约束保持不变。
- 与已完成的 Assistant 迭代冲突时：先改本需求与技术方案，再反向同步。

## Open Questions / 开放问题

- [x] Q1 默认 **pool** 规则（已关闭）：
  - **有源节点（`source_id`）**：pool **只能**是该节点的直接下游；不够 7 则在该源下补齐。
  - **无源节点**：pool 为**全域神经元**，**不排除**系统提示词根（带 `system_type` 的节点）；不够 7 则按既有规则补齐。
  - 启动 init / 取创建提示词 / ensure：传入或解析出源则走直接下游，未指定源则走全域（含系统节点）。

- [x] Q2 代码内默认 `create_neuron` 种子文案：需要。已给初稿（见 Default Seed），后续你可替换。

- [x] Q3 策略 C / 创建协议：`create_neuron` 自举完成后即可调用「创建神经元」方法——内部先 pool→7→1 取创建提示词，再调模型；入参为用途或对话 msgs。系统提示词补齐可复用该创建能力（用途=目标 system_type）。

- [x] Q4 已存在默认跳过；允许重置：删系统提示词根 + 关联边（仅到一级子节点），再重建；其它提示词不管。

- [x] Q5 重置：只删与系统根相连的**一级边**，**保留**一级子节点本体；不递归更深。选一胜者与新系统根之间**不自动建边**（系统根原则上无上游）。

- [x] Q6 业务首次需要时：同步阻塞调用 init/ensure；多次调用幂等。

- [x] Q7 AI 工具 `select_neuron_candidates`：与 `select_candidates` 同步删除 `system_type` 参数。
  - 说明：该工具只负责「凑候选」，不是 7 选 1 裁决。

## Requirement Decisions / 需求决策

- 2026-07-29 22:50:
  - 决策：新开本迭代；完成后反向同步 bootstrap/assistant 文档。
  - 决策：`system_type` 仅标记根；子节点不打同名 system_type。
  - 决策：系统提示词 content 采用策略 C（胜者后再生成）。
  - 决策：启动只保证 `assistant_select_neuron`；其它由业务 ensure。
  - 决策：选一失败可降级为权重 + 同权随机。
  - 决策：流程为 pool → 7 → 1；`select_candidates` 去掉 `system_type`。
  - 决策：允许多根与游离节点；系统提示词根原则上无上游。
- 2026-07-29 23:17:
  - 决策：提供代码内默认 `create_neuron` 种子文案（初稿已写入，可后补）。
  - 决策：创建神经元方法 = 取创建提示词(pool→7→1) + 调模型；入参 purpose 或 msgs。
  - 决策：ensure 默认跳过已存在；支持重置（根 + 一级边）后重建。
  - 决策：业务缺提示词时阻塞 init/ensure，幂等。
  - 澄清：Q1 的 pool = 候选集合来源；Q7 的 `select_neuron_candidates` = 现有凑候选 AI 工具。
- 2026-07-29 23:42:
  - 决策：Q7 关闭——工具 schema 同步去掉 `system_type`。
- 2026-07-29 23:44:
  - 决策：Q1 关闭——有 `source_id` 则仅直接下游；无源则全域（**不排除系统神经元**）。
- 2026-07-29 23:49:
  - 决策：Q5 关闭——重置只删一级边、保留子节点；胜者与新系统根不自动建边。
