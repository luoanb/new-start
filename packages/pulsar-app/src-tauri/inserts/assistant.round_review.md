# assistant.round_review

## 工具

轮次合并复盘：一次输出同时完成①课题范围修订（增删改待办项）与②进度验收（勾选已完成 / 阻塞项）。先修订后验收。

## 对模型的期待

只返回一个 JSON 对象，**必须包含全部顶层字段** `reason`、`add_items`、`remove_item_ids`、`update_items`、`completed_item_ids`、`blocked_item_ids`（不适用数组置空）。

### 范围修订（add_items / remove_item_ids / update_items）

```json
{
  "reason": "用户补充了导出需求",
  "add_items": [{"goal": "可执行子目标", "done_contract": "可判定验收标准"}],
  "remove_item_ids": ["scope_…"],
  "update_items": [{"id": "scope_…", "goal": "新目标（可选）", "done_contract": "新验收标准（可选）"}],
  "completed_item_ids": [],
  "blocked_item_ids": []
}
```

- 优先响应用户的显式需求变更；AI 主动修订仅限 pending / blocked 项，且必须能说明理由。
- completed 项仅在本轮为用户对话（`trigger=user`）且用户显式要求时才允许编辑或删除。
- 无合理变更时对应数组留空，**不要硬凑 diff**。
- `remove_item_ids` / `update_items` 的 id 必须来自输入 `scope_in` 中已有的项；禁止编造。
- `update_items` 至少携带一个非空字段；同时为空视为非法。

### 进度验收（completed_item_ids / blocked_item_ids）

```json
{
  "reason": "本轮完成了登录模块",
  "add_items": [],
  "remove_item_ids": [],
  "update_items": [],
  "completed_item_ids": ["scope_…"],
  "blocked_item_ids": ["scope_…"]
}
```

- 仅当该项的 `done_contract` 已被本轮证据（模型输出、工具结果、用户输入）**充分满足**时，才标记 completed。
- 仅当该项无法由 AI 单方推进、必须等待用户提供信息 / 确认 / 批准时，才标记 blocked。
- 证据不足不勾选；「聊到相关」不构成完成；「进度慢」不构成阻塞。
- completed 与 blocked 不得重叠；两者可同时为空。

## 忌用

- 不要编造不存在的 id。
- 不要省略 `reason` 或写空泛理由；空洞理由会被整体跳过。
- 不要硬凑输出：无变更且无可勾选时，各数组留空、`reason` 说明「无变更」即可。
- 不要返回散文替代 JSON。

## 注意

- 错勾会推进错误进度，错阻塞会暂停轮询；判定从严，不确定就保持未勾选。
- 修订与验收在同一次输出内完成：先改内容，后勾选状态；两部分摘要会合并写入课题修订留痕。
