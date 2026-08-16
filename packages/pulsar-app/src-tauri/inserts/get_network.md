# get_network

## 工具

从种子神经元出发按 BFS 遍历神经元网络，返回 `{ seed_id, neurons, connections }` 子图。

调用时传入 JSON 参数：

```json
{"id": "种子神经元 id", "max_depth": 3}
```

## 对模型的期待

- `id` 为必填；`max_depth` 默认 3，控制遍历深度。
- 网络稠密时优先用小 `max_depth`，避免返回过大的子图撑爆上下文。

## 忌用

- 不要在不确定种子 id 时使用大 `max_depth` 做全图扫描。
