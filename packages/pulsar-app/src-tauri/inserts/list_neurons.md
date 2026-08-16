# list_neurons

## 工具

分页列出神经元，返回 `{ items, total, has_more }`；一次最多 100 条，不会返回全量。

调用时传入 JSON 参数（全部可选）：

```json
{"page": 0, "page_size": 20, "search": "关键词", "kind": "all"}
```

## 对模型的期待

- `page` 从 0 开始；`page_size` 默认 20，上限 100。
- `search` 按 desc / id 模糊匹配，用于缩小范围；`kind` 可选 `all` / `system` / `normal`。
- 返回的 `has_more` 为 true 时说明还有更多，需要时翻页（`page + 1`）继续，**不要一次拉全量**。

## 忌用

- 不要省略 `page_size` 之外的约束直接列全部神经元——数据量大时会卡顿并撑爆上下文。
