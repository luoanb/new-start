# get_neuron

## 工具

按 id 获取单个神经元的详情及其连接，返回神经元本体与连接列表。

调用时传入 JSON 参数：

```json
{"id": "神经元 id"}
```

## 对模型的期待

- `id` 为必填，通常来自 `list_neurons` / `get_network` 的结果。
- 返回内容包括神经元字段（desc / content / weight / 系统类型等）与其连接，据此判断是否引用该神经元。

## 忌用

- 不要凭记忆编造 id——不存在的 id 会返回错误。
