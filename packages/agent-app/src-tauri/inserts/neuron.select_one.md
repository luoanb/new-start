# neuron.select_one

## 工具

从给定候选 neuron 列表中选出**一个**最适合本轮介入的 neuron。

## 对模型的期待

只返回 JSON 对象：

```json
{"neuron_id":"<候选中的某个 id>"}
```

- `neuron_id` 必须出现在输入 `candidates` 中。
- 依据候选的 `desc` / `content` / `weight` / `tool_ids` 与当前任务匹配度选择，不要只看 weight。

## 忌用

- 不要返回候选外的 id。
- 不要一次选多个。
- 不要改写候选 content。

## 注意

选型失败时系统可能回退为按 weight 选择；仍应尽量给出合法 `neuron_id`。
