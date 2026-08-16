# update_neuron

## 工具

更新普通神经元的描述（desc）或内容（content），返回更新后的神经元。

调用时传入 JSON 参数：

```json
{"id": "神经元 id", "desc": "新的短描述", "content": "新的内容正文"}
```

## 对模型的期待

- `id` 为必填；`desc` / `content` 至少提供一个。
- `content` 必须可直接作为角色 / 系统提示使用；`desc` 保持简短。

## 忌用

- 不要尝试更新系统神经元（system_type 非空）——会被拒绝。
- 不要清空 `content` 或填入占位句。
