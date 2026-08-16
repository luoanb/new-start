# Technical Plan / 技术方案: topic-mgmt-edge-cases

## Requirement Baseline / 需求基线

- 对应需求文档：[requirements.md](file:///home/lab/Documents/trae_projects/new-start-wt/docs/sdd-lab/2026-08-16_16-33_topic-mgmt-edge-cases/requirements.md)
- 需求确认状态：已确认（Q1 最终确认：`WaitingUser` / `WrappingUp` 均新增为 `TopicStatus` 枚举成员）
- 本方案覆盖范围：
  1. 边界 1：`ScopeInItem.status` 新增 `blocked`（等待用户）；全部非 completed 项均为 blocked 时课题状态为 `WaitingUser`；PollAll 过滤显式跳过；User 轮自动解除。
  2. 边界 2：`TopicStatus` 新增 `WrappingUp`；工具轮结束时不关闭课题，置 `WrappingUp` 保持轮询；下一轮文本收尾（非工具轮）后置 `Done`。

## Current Project Facts / 当前项目事实

- 已读取文件/模块：
  - [models.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/models.rs#L441-L463)：`ScopeInItem{id, goal, done_contract, status}`（status 默认 `"pending"`，可 `"completed"`）；`TopicStatus{Todo, InProgress, Paused, Done, Cancelled}`（`#[serde(rename_all = "snake_case")]`）。
  - [topic_store.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/topic_store.rs#L139-L162)：`list_unfinished` 用 `status NOT IN ('done','cancelled')` —— **`waiting_user` 会被列出，必须在 PollAll 过滤显式排除**。
  - [topic_store.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/topic_store.rs#L358-L447)：`complete_scope_item` → `mutate_scope`（事务内改 `status="completed"` → `derive_topic_state` → UPDATE `scope_in/progress/status`）；`mutate_scope` 在 `Paused` 时拒绝修改；`resume` 重新 derive。
  - [topic_store.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/topic_store.rs#L569-L586)：`derive_topic_state(items) -> (u8, TopicStatus)`：全部 completed → `(100, Done)`；无 completed → `(0, Todo)`；否则 InProgress；空 → `(0, Todo)`。
  - [topic_store.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/topic_store.rs#L626-L631)：`status_to_string` 用 serde snake_case 序列化，新枚举成员自动支持 `"waiting_user"` / `"wrapping_up"`；`row_to_topic` 反序列化同样自动支持。
  - [topic_store.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/topic_store.rs#L555-L567)：`normalize_scope_items` 把非 `completed|done` 一律归并为 `pending`——**新增 `blocked` 需在此保留**。
  - [assistant_session.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L261-L282)：PollAll 过滤 `matches!(topic.status, TopicStatus::Paused | TopicStatus::Cancelled)` —— **需加 `WaitingUser`**。
  - [assistant_session.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L781-L850)：`complete_scope` afterhook 仅解析 `completed_item_ids` → `complete_scope_item`。
  - [assistant_session.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L457-L502)：`before_round`（User 轮：score_feedback + match_topic）；`after_round` 按 trigger 处理 `complete_scope` 错误（Poller 仅记录）。
  - [assistant_session.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L966-L994)：`build_topic_brief` 已含"若所有事项均已完成，输出完成总结"，需为 blocked / WrappingUp 细化。
  - [conversation_runner.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/conversation_runner.rs#L249-L256)：`last_message_is_tool_result(session_id)` 已存在——**afterhook 中调用反映本轮最后一条**（persist_outcome 先于 after hooks）。
  - [round_executor.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/round_executor.rs#L146-L179)：单轮 = 一次模型调用 + 执行全部工具，工具后无二次总结——边界 2 根因。
  - [assistant.complete_scope.md](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/inserts/assistant.complete_scope.md)：裁决契约仅 `completed_item_ids`。
  - 前端 [types.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/types.ts#L204-L218)：`TopicStatus = "todo"|"in_progress"|"paused"|"done"|"cancelled"`；`ScopeInItem.status: string`。
  - 前端 [TopicPanel.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/components/TopicPanel.svelte#L200-L224)：状态徽标 `tMap("topicPanel.topicStatus", topic.status)`；暂停按钮仅 `todo/in_progress/paused`；scope 项完成按钮 `item.status !== "completed"`。
  - [translations.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/i18n/translations.ts#L615-L619)：`topicStatus` 映射 + `scopeStatusDone`。

- 当前实现事实：
  - `derive_topic_state` 是状态唯一推导源，但无法表达"等待用户"——需扩展为可产出 `WaitingUser`。
  - `mutate_scope` 在 Paused 时拒绝修改；`WaitingUser` 课题的写入路径（User 轮 before hook 解除 blocked）走独立方法，不经 `mutate_scope`。
  - 边界 2 的关闭时机在 `complete_scope_item` 内 `derive_topic_state` 完成：最后一项勾选 → `Done` → 被 `list_unfinished` 排除 → 轮询停止。
  - 前端 `Topic` 类型无 `extra` 依赖需求（WaitingUser 是独立状态，直接展示标签）。

- 相关接口/数据结构：
  - `ScopeInItem.status`：`"pending" | "completed" | "blocked"`。
  - `TopicStatus`：新增 `WaitingUser`（`"waiting_user"`）与 `WrappingUp`（`"wrapping_up"`）。
  - 裁决 JSON：`{"completed_item_ids": [...], "blocked_item_ids": [...]}`。

- 约束与风险：
  - 存量 neuron content（`assistant.complete_scope`）未更新时不返回 `blocked_item_ids` → 代码 `unwrap_or_default` 容错，行为与现状一致。
  - `normalize_scope_items` 必须保留 `blocked`，否则重启/迁移会把 blocked 归并回 pending，丢失语义。
  - `list_unfinished` 天然包含 `waiting_user`，**PollAll 过滤必须显式排除**，否则边界 1 复现（等待用户课题仍被轮询）。

## Open Questions / 开放问题

- 无待确认项。Q1 已由用户最终确认；其余实现细节由本方案落定。

## Solution Options / 方案候选

### Option A / 方案 A（推荐，唯一完整路径）

- 推荐：是
- 方案摘要：数据层引入 `blocked` + `WaitingUser` + `WrappingUp`（均独立枚举成员）；存储层把"等待用户"作为推导信号统一落库；hook 层扩展 complete_scope（双通道裁决 + 延迟关闭）、User 轮解除 blocked、PollAll 显式跳过 WaitingUser；简报与前端同步展示。不触碰 round 引擎。
- 涉及模块：`models.rs`、`topic_store.rs`、`assistant_session.rs`、`inserts/assistant.complete_scope.md`、前端 `types.ts` / `TopicPanel.svelte` / `translations.ts`。
- 优点：状态语义清晰，无 flag 区分；`WaitingUser` 与手动 `Paused` 互不干扰；前端直接按状态渲染。
- 缺点：枚举 +2 成员；PollAll 过滤需显式加 `WaitingUser`（原本 Paused 方案零改动）；前端筛选/标签 +1。
- 风险：WrappingUp 若模型持续工具调用会滞留（需求接受，见验收标准 4）。

### Option B / 方案 B（不采用）

- 推荐：否
- 方案摘要：复用 `Paused` + `extra.assistant.waiting_user` 标志区分"等待用户"；executor 工具后二次模型调用。
- 不采用原因：用户已变更决策（Q1 最终确认 WaitingUser 独立成员）；复用 Paused 需 flag 区分、处理"手动暂停被误恢复"、mutate_scope 拒绝路径等额外复杂度；executor 改动 round 引擎核心，风险大。

## Decision / 方案决策

- Selected / 选定方案：Option A（WaitingUser / WrappingUp 均独立枚举成员）
- Why / 选择原因：用户 Q1 最终确认；语义清晰、实现直接、无 flag 歧义。
- Decision Owner / 决策人：user（Q1 已确认；本方案为对应技术实现）
- Decision Time / 决策时间：2026-08-16
- Open Questions 状态：全部关闭

## API Design / API 设计

### Contract Scope / 契约范围

- 变更类型：扩展（非破坏性）
- 消费方：TopicStore 调用方（assistant_session hooks）、前端 Topic 面板、`assistant.complete_scope` 裁决模型
- 真相源文件：`models.rs`（Rust）、`types.ts`（前端）、`inserts/assistant.complete_scope.md`（裁决契约）

### `ScopeInItem.status`

- 值域：`"pending" | "completed" | "blocked"`
- `blocked`：等待用户介入，不可被模型自行解除；只由模型裁决写入、用户接入后由 before hook 解除。

### `TopicStatus`

- 值域：`todo | in_progress | paused | done | cancelled | waiting_user | wrapping_up`
- `WaitingUser`：全部非 completed 项均 `blocked`，等待用户介入；由 scope 推导产出；PollAll 过滤显式跳过；用户接入后解除。
- `WrappingUp`：scope 100% 完成但最后轮以工具调用结束，等待收尾总结；仅轮询期过渡态，非工具轮后转 `done`。

### `assistant.complete_scope` 裁决 JSON

- `completed_item_ids: string[]`：done_contract 已被本轮证据满足的项（现状语义）。
- `blocked_item_ids: string[]`（新增）：需用户提供信息 / 确认 / 批准才能继续的项。
- 兼容：存量模型不返回 `blocked_item_ids` 时按 `[]` 处理。

### Compatibility Notes / 兼容说明

- `status_to_string` / `row_to_topic` 走 serde，`waiting_user` / `wrapping_up` 自动读写，无需迁移。
- `list_unfinished` SQL 不变（天然包含新状态）；跳过语义由 PollAll 过滤承接。
- 存量 scope 数据（pending/completed）与课题状态行为不变。

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：`requirements.md` 已确认；本方案已批准。
- 若执行前需求、API、范围或交互规则变化：先回写文档，再动代码。

### Step 1. 数据模型：`models.rs` + `inserts/assistant.complete_scope.md`

#### 文件：`packages/pulsar-app/src-tauri/src/core/models.rs`

- 改动类型：修改
- 改动内容：
  - `TopicStatus` 枚举增加 `WaitingUser` 与 `WrappingUp` 两个成员（serde snake_case 自动输出 `waiting_user` / `wrapping_up`）。
  - `ScopeInItem.status` 注释与默认值不变；`blocked` 通过 `normalize_scope_items` 保留（见 Step 2）。
- 验收点：`cargo test` 通过；两个新成员序列化/反序列化往返正确。

#### 文件：`packages/pulsar-app/src-tauri/inserts/assistant.complete_scope.md`

- 改动类型：修改
- 改动内容：契约示例与说明扩展为双字段：
  - 示例：`{"completed_item_ids":["scope_1"],"blocked_item_ids":["scope_2"]}`
  - 说明 `blocked_item_ids` 判定依据（需用户提供信息/确认/批准才能继续）与"未满足勿勾选/勿阻塞"的忌用约束。
  - 注明存量 neuron content 不强制迁移，未返回新字段按空处理。
- 验收点：文档契约与代码解析字段一致。

### Step 2. 存储层：`topic_store.rs`

#### 文件：`packages/pulsar-app/src-tauri/src/core/topic_store.rs`

- 改动类型：修改
- 改动内容：
  1. `normalize_scope_items`：增加 `"blocked"` 分支保留（`"completed"|"done" → completed`，`"blocked" → blocked`，其余 → `pending`）。
  2. `derive_topic_state` 扩展（保持返回 `(u8, TopicStatus)` 或引入本地 `DerivedTopicState`，二选一，倾向后者以区分"推导态"与"存储态"）：
     - `items.is_empty()` → `(0, Todo)`
     - 全部 `completed` → `(100, Done)`
     - `pending == 0 && blocked > 0`（无未完成非 blocked 项）→ `(progress, WaitingUser)`
     - `completed == 0` → `(0, Todo)`；否则 → `(progress, InProgress)`
     - `progress = completed * 100 / len`（blocked 不计完成，与现状口径一致）
  3. 新增 `mark_scope_item_blocked(topic_id, item_id)`：复用 `mutate_scope` 置 `status = "blocked"`。
  4. 新增 `unblock_scope_items(topic_id)`：将所有 `blocked` 项恢复 `pending`，重 derive 并写回（`WaitingUser` → `Todo/InProgress/Done`）。写入路径独立于 `mutate_scope`（不触发其 Paused 检查）。
  5. `mutate_scope` 的 Paused 拒绝逻辑保持不变；`WaitingUser` 课题的写入一律走 `unblock_scope_items`（User 轮 before hook 先行释放，afterhook 再写）。
  6. `resume` / `create` / `migrate_scope_in` 复用扩展后的 derive（无需改结构，仅函数内部逻辑变化）。
- 设计约束：
  - 状态推导单一真相源：`derive_topic_state`；`WaitingUser` 由推导产出，`WrappingUp` 由 hook 层 `set_status` 显式设置（round 层面过渡态，不来自 scope）。
- 验收点：新增单测覆盖推导矩阵（见 Step 6）；`complete_scope_item` 后全部 completed 仍置 Done；`blocked` 归一化在 `create/migrate` 后保留。

### Step 3. Hook 层：`assistant_session.rs`

#### 文件：`packages/pulsar-app/src-tauri/src/core/assistant_session.rs`

- 改动类型：修改
- 改动内容：
  1. PollAll 过滤：跳过条件扩展为 `TopicStatus::Paused | TopicStatus::Cancelled | TopicStatus::WaitingUser`（`list_unfinished` 天然列出 waiting_user，此处必须排除，否则等待用户课题仍被轮询）。
  2. `before_round` User 分支：在 `score_feedback` 前调用 `unblock_scope_items(ctx)`——若课题存在 blocked 项（含 `WaitingUser`）则解除并重 derive 恢复轮询；手动 `Paused` 课题不受影响。
  3. `complete_scope` afterhook 扩展：
     - 开头保护：`topic.status` 为 `Paused` 或 `WaitingUser` 时直接 `Ok(())` 跳过（不再触发 mutate 报错；Poller 轮现状仅记录，User 轮从报错改为跳过）。
     - 收尾关闭判断（前置）：若 `topic.status == WrappingUp` 且本轮**非**工具结束 → `set_status(Done)` 并返回。
     - 裁决调用后解析双字段：`completed_item_ids` → `complete_scope_item`；`blocked_item_ids` → `mark_scope_item_blocked`（均容错 `unwrap_or_default`）。
     - 延迟关闭判断（后置）：重读 topic；若 scope 已 100% completed 且 `last_message_is_tool_result(session_id)`（本轮以工具调用结束）→ `set_status(WrappingUp)`；非工具轮则 store 已推导为 `Done`。
  4. `build_topic_brief` 细化：
     - blocked 项渲染为 `[⏳] {goal}` + `等待用户：{done_contract}`（勿选语义）。
     - `topic.status == WrappingUp` 时末尾指令改为"所有事项已完成，请输出最终总结并复核，本轮无需调用工具"。
     - 其余结构不变（非 blocked 项、进度、课题信息）。
  5. 常量/注释同步（`SYSTEM_TYPE_COMPLETE_SCOPE` 语义、hook 文档注释）。
- 设计约束：
  - 延迟关闭判断依赖 `last_message_is_tool_result`（afterhook 时反映本轮最后一条已 persist 的消息）。
  - 不新增 Tauri 命令、不改 round 引擎。
- 验收点：工具轮结束时课题为 `WrappingUp` 而非 `Done`；非工具轮收尾后 `Done`；`WaitingUser` 课题不被 PollAll 推进；User 轮解除 blocked 后课题恢复轮询；Paused/WaitingUser 课题 complete_scope 跳过不报错。

### Step 4. 前端：`types.ts` / `translations.ts` / `TopicPanel.svelte`

#### 文件：`packages/pulsar-app/src/lib/types.ts`

- 改动类型：修改
- 改动内容：
  - `TopicStatus` 增加 `"waiting_user"` 与 `"wrapping_up"`。
  - `ScopeInItem.status` 注释更新为 `"pending" | "completed" | "blocked"`。
- 验收点：类型与 Rust 契约一致；svelte-check 通过。

#### 文件：`packages/pulsar-app/src/lib/i18n/translations.ts`

- 改动类型：修改
- 改动内容：
  - `topicStatus` 映射增加 `waiting_user`（中：等待用户 / en：Waiting for user）与 `wrapping_up`（中：收尾中 / en：Wrapping up）。
  - 新增 `scopeStatusBlocked`（中：等待用户 / en：Waiting user）。
  - 中英文两份同步。
- 验收点：文案齐全，无缺失 key。

#### 文件：`packages/pulsar-app/src/lib/components/TopicPanel.svelte`

- 改动类型：修改
- 改动内容：
  - 状态徽标 `tMap` 自动覆盖 `waiting_user` / `wrapping_up`（靠 translations；确认新状态样式 class 可读，必要时补 CSS）。
  - scope item：`item.status === "blocked"` 时展示"等待用户"徽标且隐藏"完成"按钮。
  - 暂停/恢复按钮逻辑保持现状（`waiting_user` / `wrapping_up` 不显示暂停按钮，天然满足）。
- 验收点：新状态展示正确；前端 lint 通过；手动验证 TopicPanel 刷新后状态一致。

### Step 5. 裁决契约回归

- 确认 `call_judgement(SYSTEM_TYPE_COMPLETE_SCOPE)` 的 JSON 解析对缺失 `blocked_item_ids` 容错（`unwrap_or_default`），存量 neuron content 不迁移即可工作。
- 验收点：存量数据下 complete_scope 行为与改动前一致。

### Step 6. 测试与检查

#### 命令

- 运行：`cargo test -p pulsar-app`（或工作区对应命令，确认后执行）。
- 前端检查：`pnpm` workspace 内 svelte-check / lint（按仓库现有脚本）。
- 修复：按失败用例回改，遵守 Reverse Sync。

#### 单测覆盖（`topic_store.rs` / `assistant_session.rs` 现有测试模块补充）

- `derive_topic_state` 推导矩阵：全 pending / 全 completed / 部分 completed / 全 blocked / blocked+completed（无 pending）/ blocked+pending（有 pending）。
- `normalize_scope_items` 保留 `blocked`；迁移路径不丢 blocked。
- `unblock_scope_items`：`WaitingUser` → 恢复可轮询状态；手动 `Paused` 课题不受影响。
- `build_topic_brief`：blocked 项标记、WrappingUp 收尾指令文案。
- `complete_scope` 延迟关闭（工具轮 → WrappingUp；非工具轮 → Done）与 PollAll 过滤（WaitingUser 跳过）——按现有 hook 测试模式用 fake 裁决/runner 校验。

#### 文件：`docs/sdd-lab/2026-08-16_16-33_topic-mgmt-edge-cases/lifecycle.md`

- 回写执行记录、实际改动摘要、验证结果、下一步状态。

## Risk And Mitigation / 风险与缓解

- 存量 neuron content 未含 `blocked_item_ids`：
  - 缓解：解析 `unwrap_or_default`，行为与现状一致；insert 文件已更新，下次 reset-system 自动生效。
- `normalize_scope_items` 漏保 blocked 导致重启丢失语义：
  - 缓解：Step 2 显式增加 `"blocked"` 分支 + 单测覆盖。
- PollAll 未显式排除 `WaitingUser`（`list_unfinished` 天然包含）：
  - 缓解：Step 3 显式扩展跳过清单 + 单测覆盖；需求约束已固化。
- `WaitingUser` 课题写入路径与 `mutate_scope` Paused 检查冲突：
  - 缓解：解除走独立 `unblock_scope_items`；complete_scope 对 Paused/WaitingUser 课题跳过。
- WrappingUp 滞留（模型持续工具调用）：
  - 缓解：需求验收标准 4 已接受有限滞留；收尾轮简报明确"无需调用工具"，且非工具轮后置判断保证收敛路径存在。

## Execute Checkpoint / 执行检查点

- 当前理解：边界 1 用 blocked + WaitingUser（独立状态 + PollAll 显式排除）解决无限轮询；边界 2 用 WrappingUp 延迟关闭解决无法收尾。
- 核心目标：数据层（models/topic_store）、hook 层（assistant_session：complete_scope 双通道 + 延迟关闭 + PollAll 过滤 + User 轮释放）、契约（complete_scope.md）、前端（types/i18n/TopicPanel）四处改动落地，全量回归通过。
- 下一步动作：等待用户批准本技术方案后进入执行（executing）。
- 风险：改动横跨 Rust 后端与前端；单测与 svelte-check 是主要验证手段。
