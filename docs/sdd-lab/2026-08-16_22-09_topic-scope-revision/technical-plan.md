# Technical Plan / 技术方案: topic-scope-revision

## Requirement Baseline / 需求基线

- 对应需求文档：[requirements.md](file:///home/lab/Documents/trae_projects/new-start-wt/docs/sdd-lab/2026-08-16_22-09_topic-scope-revision/requirements.md)
- 需求确认状态：已确认（用户 2026-08-16 22:09 批准；Q1~Q4 全部关闭）
- 本方案覆盖范围：
  1. 新增 `revise_topic` afterhook 裁决步骤（平行于 complete_scope），所有触发类型（User / ManualStep / Poller）执行，Poller 失败仅记录。
  2. 契约 `inserts/assistant.revise_topic.md`：结构化 diff（`add_items` / `remove_item_ids` / `update_items` / `reason`）。
  3. 存储层 `topic_store.rs` 新增 `update_scope_item`（复用 `mutate_scope`：事务 + Paused 保护 + 重算）；add / delete 复用现状。
  4. completed 项保护（无用户显式依据不 edit / remove）+ 审计留痕（`topic.extra.revisions`）。

## Current Project Facts / 当前项目事实

- 已读取文件/模块：
  - [assistant_session.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L39-L42)：`SYSTEM_TYPE_MATCH_TOPIC` / `SYSTEM_TYPE_COMPLETE_SCOPE` / `SYSTEM_TYPE_SCORE_FEEDBACK` 常量。
  - [assistant_session.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L97-L139)：`call_judgement` —— `ensure_system_neuron(system_type)` 懒创建 → `run_raw_round`（禁工具、无标签）→ `extract_json_object` 解析。所有裁决步骤复用同一入口。
  - [assistant_session.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L794-L926)：`complete_scope` afterhook 实现模式：解析 topic → 跳过守卫（无 topic / 空 scope / Paused / WaitingUser / WrappingUp）→ `call_judgement` → 逐 id 应用（`let _ =` 容错）→ 延迟关闭。**revise 复刻此骨架**。
  - [assistant_session.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L477-L504)：`after_round` 编排：`let completed = self.complete_scope(ctx).await;` 后按 trigger 处理错误（User/ManualStep 传播，Poller 仅记录）。**revise 插在 complete_scope 之前**。
  - [assistant_session.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L1105-L1141)：`read_assistant_state` / `write_assistant_state` —— `topic.extra` 读改写模式（读 → 改 JSON → `TopicUpdate{extra}` 写回）。**审计留痕复用此模式**。
  - [assistant_session.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L1042-L1076)：`build_topic_brief` —— Poller/ManualStep 轮简报；**revise 是 afterhook 自动裁决，简报无需提及**。
  - [topic_store.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/topic_store.rs#L336-L361)：`add_scope_item` / `delete_scope_item` / `complete_scope_item` / `mark_scope_item_blocked` 均走 `mutate_scope`。
  - [topic_store.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/topic_store.rs#L476-L528)：`mutate_scope` —— 事务内读 → Paused 拒绝 → 闭包变更 → `derive_topic_state` 重算 → 写回。**`update_scope_item` 直接复用**。
  - [topic_store.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/topic_store.rs#L643-L683)：`normalize_scope_items`（pending/completed/blocked 三态归一化）与 `derive_topic_state`（progress/status 唯一推导源）。
  - [models.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/models.rs#L441-L453)：`ScopeInItem{id, goal, done_contract, status}`；`Topic{…, scope_in, extra}`。
  - [neuron/manager.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/manager.rs#L50-L62)：`default_behavior_for_system_type` —— 裁决类系统神经元映射 `insert_id`（Fixed + 禁工具 + 契约段）；[neuron/manager.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/manager.rs#L39-L44)：`REBOOTSTRAP_SYSTEM_TYPES` 清单。
  - [conversation_runner.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/conversation_runner.rs#L43-L68)：`RoundContext` 字段（`topic_id` / `model_input` / `messages` / `model` / `trigger` / `outcome`）—— revise 输入全部可得。
  - [insert_catalog.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/insert_catalog.rs#L30-L47)：inserts 目录自动嵌入，新增 `.md` 即自动出现在 catalog；[insert_catalog.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/insert_catalog.rs#L173-L188) 测试 `require_known_model_atoms` / `list_returns_all_embedded_ids` 可补 `assistant.revise_topic`。
  - 前端：`types.ts` 的 `ScopeInItem.status` 为宽松字符串；TopicPanel 按 status 展示徽标。**revise 不改数据结构，前端零改动**。

- 当前实现事实：
  - 裁决步骤（match_topic / complete_scope / score_feedback）均为「系统神经元 + insert + `call_judgement`」模式；新增 revise 完全同构。
  - `mutate_scope` 事务化、Paused 拒绝、自动重算；`add_scope_item` / `delete_scope_item` 已就绪，`update_scope_item`（改文本）缺失。
  - `topic.extra` 已有 `assistant` 键（`AssistantTopicState`）；审计留痕另开 `revisions` 键，互不冲突。
  - completed 项保护无法程序校验「用户显式依据」，只能：①触发类型门禁（仅 User 轮可动 completed 项）②裁决提示词纪律 ③reason 留痕。

- 相关接口/数据结构：
  - 契约 JSON：`{"add_items":[{"goal","done_contract"}],"remove_item_ids":["scope_…"],"update_items":[{"id","goal?","done_contract?"}],"reason":"…"}`。
  - 存储新增：`update_scope_item(topic_id, item_id, goal: Option<&str>, done_contract: Option<&str>) -> AppResult<Topic>`。
  - 留痕：`topic.extra.revisions: [{at, trigger, reason, added_count, removed_ids, updated_ids, skipped_ids}]`。

- 约束与风险：
  - 新增系统神经元 `assistant_revise_topic` 需：`default_behavior_for_system_type` 映射 + `REBOOTSTRAP_SYSTEM_TYPES` 登记 + `SYSTEM_TYPE_REVISE_TOPIC` 常量；`ensure_system_neuron` 懒创建保证首轮即可用。
  - completed 项保护的确定性边界是触发类型；语义纪律靠 insert 提示词（折中方案，见 Q2）。
  - 编辑 completed 项后是否重置 pending（重新验收）与「revise 不写 status」冲突，见 Q1。

## Open Questions / 开放问题

- [x] Q1 编辑 completed 项后，该项是否自动重置为 `pending`（重新验收）？
  - 触发来源：需求「职责边界：revise 不直接写 status」与「completed 项可被用户显式要求修改」的碰撞。
  - 无法确定的内容：改契约后旧验收结论已失效，但严格「不写 status」会让该项保持 completed、课题维持 100% Done 且停止轮询，重新验收永远不发生。
  - 影响范围：`update_scope_item` 语义、requirements.md 验收标准 4。
  - 候选处理：A. 编辑 completed 项时自动重置 pending（有限 reopen 特例，编辑触发）；B. 严格不写 status（保持 completed，不重新验收，语义偏差）。
  - 用户回答/确认：2026-08-16 22:09 用户选择 **A（重置 pending 重新验收）**。requirements.md 已同步。
  - 状态：已关闭。

- [x] Q2 completed 项保护采用「触发类型门禁 + 提示词纪律 + reason 留痕」的折中是否可接受？
  - 触发来源：方案拟定。
  - 无法确定的内容：程序无法校验「用户显式依据」这一语义；只能确定性拦截非 User 轮的 completed 项变更（Poller/ManualStep 一律跳过），User 轮交给模型纪律并留痕 reason。
  - 影响范围：`revise_topic` 的 completed 项处理分支。
  - 候选处理：A. 采用门禁折中（推荐）；B. User 轮也一律不允许动 completed 项（更保守，但「用户显式要求修改 completed 项」将不可达）。
  - 用户回答/确认：2026-08-16 22:09 用户选择 **A（触发类型门禁）**。requirements.md 已同步。
  - 状态：已关闭。

## Solution Options / 方案候选

### Option A / 方案 A（推荐）

- 推荐：是
- 方案摘要：新增 `revise_topic` afterhook 裁决步骤（复刻 complete_scope 骨架），插在 complete_scope 之前；契约 `inserts/assistant.revise_topic.md`；存储层新增 `update_scope_item`；completed 项保护（触发类型门禁）+ `topic.extra.revisions` 留痕。系统神经元懒创建 + 登记 rebootstrap。不触碰 round 引擎、不改前端。
- 涉及模块：`assistant_session.rs`、`topic_store.rs`、`neuron/manager.rs`、`inserts/assistant.revise_topic.md`（新增）、`insert_catalog.rs`（测试可选补）。
- 优点：与既有裁决步骤完全同构，风险收敛；结构化 diff 可审计；留痕保住 Spec is Truth；新增项本轮即参与 complete_scope 验收。
- 缺点：每轮多一次裁决模型调用（Poller 轮与 complete_scope 同级成本）；契约与保护规则需靠提示词纪律支撑。
- 风险：模型返回非法 id / 空字段 → 逐项容错跳过；completed 项误改 → 门禁 + 留痕缓解。

### Option B / 方案 B（不采用）

- 推荐：否
- 方案摘要：注册 `topic_manager.rs` 现有 `add_topic_scope_item` / `delete_topic_scope_item` 为模型工具，由模型自主调用。
- 不采用原因：用户已确认采用 afterhook 步骤；工具方式模型自由 CRUD，无结构化 diff、无法强制溯源与留痕，违背需求「变更必须留痕」「completed 项保护」约束。

## Decision / 方案决策

- Selected / 选定方案：Option A（revise_topic afterhook 步骤）
- Why / 选择原因：与既有裁决步骤完全同构；结构化 diff 可审计、可留痕；Q1 / Q2 用户均已确认（编辑 completed 重置 pending；触发类型门禁）。
- Decision Owner / 决策人：user
- Decision Time / 决策时间：2026-08-16 22:09
- Open Questions 状态：全部关闭（Q1 / Q2 均已由用户确认并回写 requirements.md）

## API Design / API 设计

### Contract Scope / 契约范围

- 变更类型：扩展（非破坏性）
- 消费方：`call_judgement(SYSTEM_TYPE_REVISE_TOPIC)` 裁决模型、TopicStore 调用方（assistant_session hooks）
- 真相源文件：`inserts/assistant.revise_topic.md`（裁决契约）、`topic_store.rs`（存储方法）、`topic.extra.revisions`（留痕格式）

### `assistant.revise_topic` 裁决 JSON

```json
{
  "add_items": [{"goal": "可执行子目标", "done_contract": "可判定验收标准"}],
  "remove_item_ids": ["scope_…"],
  "update_items": [{"id": "scope_…", "goal": "新目标（可选）", "done_contract": "新验收（可选）"}],
  "reason": "变更理由（用户要求 / AI 修订），必填且非空"
}
```

- `add_items`：追加为 `pending`；goal / done_contract 均非空，否则该项跳过。
- `remove_item_ids` / `update_items`：id 必须来自当前课题 scope；`update_items` 至少携带一个非空字段（goal 或 done_contract），缺省字段保持不变。
- `reason`：必填，非空；变更必须能溯源到本轮（用户输入 / 模型输出 / 工具结果），否则跳过并留痕。
- completed 项：仅 User 触发轮可被 edit / remove（门禁）；Poller / ManualStep 轮对 completed 项一律跳过并记入 `skipped_ids`。
- 兼容：模型未返回任何字段时按空处理（`unwrap_or_default`），行为与现状一致。

### `TopicStore::update_scope_item`

- `update_scope_item(topic_id: &str, item_id: &str, goal: Option<&str>, done_contract: Option<&str>) -> AppResult<Topic>`
- 语义：仅更新非空文本字段；**不改 status，唯一例外：编辑 completed 项时该项自动重置为 `pending`**（重新验收，有限 reopen 特例）；走 `mutate_scope`（事务 + Paused 拒绝 + 重算）；id 不存在返回明确错误；goal / done_contract 均空返回错误。

### `topic.extra.revisions` 留痕

- 每次 revise 应用后追加一条：
  ```json
  {"at": 1789…, "trigger": "user|manual|poller", "reason": "…",
   "added_count": 1, "removed_ids": ["…"], "updated_ids": ["…"], "skipped_ids": ["…"]}
  ```
- 无变更（diff 全空 / 全部跳过）时不写留痕；`extra` 缺省为 `{}` 后写 `revisions` 数组。

### Compatibility Notes / 兼容说明

- 新增系统神经元懒创建（`ensure_system_neuron`），存量库无需迁移；`REBOOTSTRAP_SYSTEM_TYPES` 登记保证 reset 后重建。
- 存量模型不返回 revise 字段（或返回空 diff）→ 无副作用，路径与现状一致。
- `ScopeInItem` / `TopicStatus` 结构不变；前端零改动。

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：`requirements.md` 已确认；`technical-plan.md` 已批准（Q1 / Q2 均已确认：编辑 completed 自动重置 pending；触发类型门禁；requirements.md 已同步）。
- 若执行前需求、API、范围或交互规则变化：先回写文档，再动代码。

### Step 1. 裁决契约：新增 `inserts/assistant.revise_topic.md`

#### 文件：`packages/pulsar-app/src-tauri/inserts/assistant.revise_topic.md`

- 改动类型：新增
- 改动内容：
  - `## 工具`：一句话说明「判断当前课题 scope 是否需要在推进过程中增删改，输出结构化 diff」。
  - `## 对模型的期待`：JSON 契约示例（add_items / remove_item_ids / update_items / reason）；字段约束（add 项 goal/done_contract 非空；update 至少一字段；id 必须来自输入 scope_in；reason 必填非空）。
  - 变更依据：必须能溯源到本轮用户输入 / 模型输出 / 工具结果；completed 项仅在用户显式要求时 edit / remove（Poller/ManualStep 轮禁止）；未满足勿改。
  - `## 忌用`：不编造 id；不空 scope_in 精神（add_items 的 goal/done_contract 不得占位）；不把状态勾选混入（状态归 complete_scope）；不因「进度慢」改契约；completed 项无用户依据不改。
  - `## 注意`：revise 只改内容不改状态；无变更时返回空 diff；本步骤在 complete_scope 之前执行，新加项参与本轮验收。
- 验收点：文件存在且含 `## 工具` / `## 对模型的期待`；`cargo test` 中 insert_catalog 测试（如补入）通过。

### Step 2. 存储层：`topic_store.rs`

#### 文件：`packages/pulsar-app/src-tauri/src/core/topic_store.rs`

- 改动类型：修改
- 改动内容：
  1. 新增 `update_scope_item(topic_id, item_id, goal: Option<&str>, done_contract: Option<&str>)`：
     - goal / done_contract 全空 → `Err(InvalidInput)`。
     - `mutate_scope` 内找到 item，非空字段覆盖文本。
     - item 原 `status == "completed"` 时置 `"pending"`（编辑即重新验收，Q1 已确认）。
     - 未找到 item → `Err(InvalidInput("Scope item not found: {item_id}"))`（与 delete 一致）。
- 设计约束：
  - 复用 `mutate_scope`（事务 + Paused 拒绝 + `derive_topic_state` 重算），不新开旁路。
  - 不改 `add_scope_item` / `delete_scope_item`。
- 验收点：单测覆盖——仅改 goal / 仅改 done_contract / 改双字段 / 全空报错 / id 不存在报错 / Paused 拒绝 / completed 项编辑后状态（按 Q1 决策）与 progress 重算。

### Step 3. Hook 层：`assistant_session.rs`

#### 文件：`packages/pulsar-app/src-tauri/src/core/assistant_session.rs`

- 改动类型：修改
- 改动内容：
  1. 常量：新增 `pub const SYSTEM_TYPE_REVISE_TOPIC: &str = "assistant_revise_topic";`（L39-42 区域）。
  2. 新增 `async fn revise_topic(&self, ctx: &mut RoundContext) -> AppResult<()>`（复刻 complete_scope 骨架）：
     - 守卫：无 `ctx.topic_id` → skip；topic 不存在 → skip；`status ∈ {Paused, WaitingUser}` → skip（Paused 由 mutate_scope 兜底，此处提前记录）。
     - payload：`{topic_id, scope_in, model_output, tool_results, user_input, trigger}`。
     - `call_judgement(SYSTEM_TYPE_REVISE_TOPIC, …)`；解析 add_items / remove_item_ids / update_items / reason。
     - 应用（逐项容错，`let _ =` + warn 日志）：
       - `add_items`：goal / done_contract 非空才 `add_scope_item`。
       - `remove_item_ids`：completed 项且非 User 轮 → 记 `skipped_ids`；其余 `delete_scope_item`。
       - `update_items`：completed 项且非 User 轮 → 记 `skipped_ids`；其余（含字段过滤）`update_scope_item`。
     - 留痕：有实际应用或跳过时，`append_revision_log(topic_id, trigger, reason, added_count, removed_ids, updated_ids, skipped_ids)`；reason 缺失时用占位「（无 reason）」。
  3. 新增 `fn append_revision_log(&self, topic_id, event)`：复用 `write_assistant_state` 的 extra 读改写模式，维护 `extra.revisions` 数组。
  4. `after_round` 编排：在 `let completed = self.complete_scope(ctx).await;` 之前插入 `let revised = self.revise_topic(ctx).await;`；错误处理与 complete_scope 完全一致（User/ManualStep 传播，Poller 仅记录）。注意保留 complete_scope 的 WrappingUp 前置判断语义（revise 先跑，complete_scope 后跑，顺序即「先改内容再验收」）。
  5. 实现演进（Reverse Sync）：裁决解析（字段过滤 / completed 门禁 / skipped_ids 归集）抽为模块级纯函数 `parse_scope_revision` 与 `RevisionPlan`（`now_ms` / `append_revision_log` 同置模块级），行为与上列语义完全一致，仅为可单测性重构；`append_revision_log` 落地为自由函数（签名 `(topic_store, topic_id, event)`），调用处传 `&self.assistant.topic_store`。
- 设计约束：
  - 不改 `match_topic` / `complete_scope` / `score_feedback` / `build_topic_brief` / round 引擎。
  - 裁决同源：`ctx.model`（与现有 hooks 一致）。
- 验收点：revise 在 complete_scope 前执行；Poller 轮失败仅记录；空 diff 无副作用；completed 门禁（Poller 跳过 / User 放行）；留痕写入 `extra.revisions`。

### Step 4. 系统神经元登记：`neuron/manager.rs`

#### 文件：`packages/pulsar-app/src-tauri/src/core/neuron/manager.rs`

- 改动类型：修改
- 改动内容：
  1. `default_behavior_for_system_type` 增加分支：`"assistant_revise_topic" => Some("assistant.revise_topic")`（Fixed + 禁工具 + 契约段）。
  2. `REBOOTSTRAP_SYSTEM_TYPES` 数组追加 `"assistant_revise_topic"`。
- 验收点：`cargo test` 通过；`ensure_system_neuron("assistant_revise_topic")` 返回带 `insert_id` 的 behavior。

### Step 5. 测试与检查

#### 文件：`packages/pulsar-app/src-tauri/src/core/topic_store.rs`（测试模块）

- 新增：`update_scope_item` 单测（见 Step 2 验收点）。

#### 文件：`packages/pulsar-app/src-tauri/src/core/assistant_session.rs`（测试模块）

- 新增：revise 解析与应用（fake 裁决）、空 diff 无副作用、completed 门禁（Poller 跳过 / User 放行）、留痕写入、Paused 守卫；after_round 顺序（revise 先于 complete_scope）按现有 hook 测试模式（fake runner / 裁决）校验。

#### 文件：`packages/pulsar-app/src-tauri/src/core/insert_catalog.rs`（测试模块，可选）

- `require_known_model_atoms` / `list_returns_all_embedded_ids` 追加 `assistant.revise_topic`（可选，非强制）。

#### 命令

- 运行：`cargo test -p pulsar-app`（或工作区对应命令，确认后执行）。
- 前端检查：本次零前端改动，跳过（如仓库脚本要求全量 check 则运行 `pnpm check` 确认无新增 error）。
- 修复：按失败用例回改，遵守 Reverse Sync。

#### 文件：`docs/sdd-lab/2026-08-16_22-09_topic-scope-revision/lifecycle.md`

- 回写执行记录、实际改动摘要、验证结果、下一步状态（planned → executing → done）。

## Risk And Mitigation / 风险与缓解

- 模型返回非法 / 不存在 id 或空字段：
  - 缓解：逐项容错（`let _ =` + warn），结构性解析失败才传播；契约明示「id 必须来自输入 scope_in」。
- completed 项误改（语义无法程序校验）：
  - 缓解：触发类型门禁（非 User 轮跳过 + `skipped_ids` 留痕）+ 提示词纪律 + reason 必填留痕（Q2 折中）。
- 编辑 completed 项后旧验收失效：
  - 缓解：Q1 已确认——编辑 completed 项自动重置 pending 重新验收；requirements.md 已同步。
- 每轮额外裁决调用成本：
  - 缓解：与 complete_scope 同级成本，属已接受模式；空 diff 无副作用、不写留痕。
- `extra.revisions` 无界增长：
  - 缓解：本期仅留痕不提供查询 / 回滚；条目数受实际变更频次约束，未设硬上限（与 scope 文本容量口径一致）。

## Execute Checkpoint / 执行检查点

- 当前理解：新增 `revise_topic` afterhook 步骤，模型输出结构化 diff（add / remove / update + reason），复用 `mutate_scope` 原子应用并重算，completed 项触发类型门禁（编辑自动重置 pending），变更写入 `topic.extra.revisions` 留痕。
- 核心目标：契约（inserts/assistant.revise_topic.md）、存储（update_scope_item）、hook（revise_topic + after_round 顺序 + 留痕）、登记（manager.rs）四处落地，全量回归通过。
- 下一步动作：等待用户批准本技术方案后进入执行（executing）。
- 风险：跨存储层与 hook 层；单测是主要验证手段；Q1 / Q2 已确认并回写文档。
