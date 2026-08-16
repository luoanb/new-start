# assistant.revise_topic

## 工具

判断当前课题 scope 是否需要在推进过程中增删改（用户补充需求 / 取消需求 / 调整验收标准，或 AI 发现契约过时），输出结构化 diff。

## 对模型的期待

只返回 JSON 对象，包含最多四类字段：

```json
{
  "add_items": [
    {"goal": "可执行子目标", "done_contract": "可判定验收标准"}
  ],
  "remove_item_ids": ["scope_…"],
  "update_items": [
    {"id": "scope_…", "goal": "新目标（可选）", "done_contract": "新验收标准（可选）"}
  ],
  "reason": "变更理由（必填且非空）"
}
```

- `add_items`：新增为 pending 项；每项 `goal` 与 `done_contract` 均非空，缺一即整项跳过。
- `remove_item_ids` / `update_items`：`id` 必须来自输入 `scope_in` 中已有的项，不得编造。
- `update_items`：至少携带一个非空字段（`goal` 或 `done_contract`）；未携带的字段保持不变；同时为空视为非法，跳过该项。
- `reason`：必填且非空。变更必须能溯源到本轮输入（用户输入 / 模型输出 / 工具结果）——用户明确要求、或证据表明原契约过时 / 范围错误。无法溯源的空洞理由视为无效，跳过本轮全部变更。

### 变更依据

- 优先响应**用户显式要求**（如「顺便把 X 也做了」「Y 不用做了」「Z 的验收标准改成 W」）。
- AI 主动修订仅限 `pending` / `blocked` 项（如推进中发现 done_contract 不可判定、目标已偏离）；修订后应能说明理由。
- **`completed` 项**：只有在本轮为**用户对话（User 轮）且用户显式要求**修改 / 删除时才允许 edit / remove；轮询 / 手动推进轮一律不得改动 `completed` 项。

## 忌用

- 不要编造不存在的 `id`。
- 不要省略 `reason` 或写空泛理由。
- 不要把状态勾选混入（completed / blocked 状态由 `assistant.complete_scope` 管理，本步骤只增删改条目与文本）。
- 不要因「进度慢」或「想让它显得完成」而改契约文本。
- `add_items` 的 `goal` / `done_contract` 不得是占位或空串。
- 无任何变更时不要硬凑 diff；返回空对象或仅 `reason`。

## 注意

- 本步骤在 `complete_scope` 之前执行：新加的项本轮即可参与验收勾选。
- 编辑 `completed` 项会被自动重置为 `pending`（契约已变，需重新验收）；删除已完成项会改变课题进度口径，仅在用户明确要求时删除。
- 未返回任何字段时按空 diff 处理，不产生副作用。
