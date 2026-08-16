# assistant.complete_scope

## 工具

根据本轮模型输出 / 工具结果 / 用户输入，判断课题中哪些 ScopeIn 项已经满足其 Done Contract 应标记完成，哪些需要用户介入而标记为等待用户。

## 对模型的期待

只返回 JSON 对象：

```json
{"completed_item_ids":["scope_item_id_1"],"blocked_item_ids":["scope_item_id_2"]}
```

- `completed_item_ids`：字符串数组；元素必须是输入 `scope_in` 里已有项的 `id`。表示该项的 `done_contract` 已被本轮证据满足。
- `blocked_item_ids`：字符串数组；元素必须是输入 `scope_in` 里已有项的 `id`。表示该项当前无法由 AI 继续推进，必须等待用户提供信息 / 确认 / 批准才能继续（如缺少必要资料、方案待用户拍板）。
- 没有任何项完成时返回 `{"completed_item_ids":[]}`；没有任何项阻塞时返回 `{"blocked_item_ids":[]}`；两者可同时为空。
- 判定依据：`done_contract` 是否已被本轮证据满足；未满足勿勾选，也不要以"进度慢"为由阻塞。

## 忌用

- 不要编造 id。
- 不要因为「聊到相关」就提前勾选。
- 不要修改 goal/done_contract 文本（本工具只勾选 / 阻塞）。
- 不要把尚未实际卡住的项标为 `blocked_item_ids`——只有确实需要用户介入、AI 无法单方推进的项才阻塞。
- `blocked_item_ids` 与 `completed_item_ids` 不要重叠。

## 注意

系统只会对返回的 id 调用对应接口；错勾会推进错误进度，错阻塞会让课题进入等待用户状态并暂停轮询。
