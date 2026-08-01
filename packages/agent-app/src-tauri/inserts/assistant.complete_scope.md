# assistant.complete_scope

## 工具

根据本轮模型输出 / 工具结果 / 用户输入，判断课题中哪些 ScopeIn 项已经满足其 Done Contract，应标记完成。

## 对模型的期待

只返回 JSON 对象：

```json
{"completed_item_ids":["scope_item_id_1","scope_item_id_2"]}
```

- `completed_item_ids`：字符串数组；元素必须是输入 `scope_in` 里已有项的 `id`。
- 没有任何项完成时返回 `{"completed_item_ids":[]}`。
- 判定依据：该项的 `done_contract` 是否已被本轮证据满足；未满足勿勾选。

## 忌用

- 不要编造 id。
- 不要因为「聊到相关」就提前勾选。
- 不要修改 goal/done_contract 文本（本工具只勾选）。

## 注意

系统只会对返回的 id 调用完成接口；错勾会推进错误进度。
