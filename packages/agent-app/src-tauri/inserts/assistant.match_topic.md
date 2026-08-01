# assistant.match_topic

## 工具

判断当前用户输入应**切换到已有课题**还是**新建课题**。新建时必须完成课题任务拆解。

## 对模型的期待

只返回 JSON 对象，且**必须包含顶层 `action` 字段**（`switch` / `create`）。若未给出 `action`，按 `create` 处理。

### 切换已有课题

```json
{"action":"switch","topic_id":"topic_…"}
```

- `topic_id` 必须来自输入列表中的未完成课题。
- 若给出的 `topic_id` 不在输入列表中，本步骤会回退为新建（带兜底 scope），但**不要依赖该回退**，应优先匹配真实存在的课题。

### 新建课题（任务拆解）

```json
{
  "action": "create",
  "name": "短标题",
  "description": "一句话课题说明",
  "scope_in": [
    {
      "goal": "可执行的子目标",
      "done_contract": "如何验收算完成（Done Contract）"
    }
  ]
}
```

- `scope_in`：**核心产出，禁止为空**。至少 1 项；每项 `goal` 与 `done_contract` 均非空。
- 即使用户输入是开放式提问、闲聊或意图模糊（如「推荐十本书」「帮我想想」），也必须**基于输入自行推断**出合理的 `goal` 与 `done_contract`，**不得省略 `scope_in` 或留空**。
- `goal`：可执行的子目标，不空泛。
- `done_contract`：可判定的完成标准，不是口号；例如「列出 10 本书并附一句话理由」比「推荐好书」更可验收。
- 拆解粒度：服务后续推进与勾选完成，避免空泛单项「做完整个需求」。
- `name` / `description` 缺省时回退为用户输入原文，但 `scope_in` 无回退，务必给出。

## 忌用

- `create` 时不要省略 `scope_in` 或只写 name；`scope_in` 为空将导致本轮对话直接失败、无返回。
- 不要发明不存在的 `topic_id`。
- 不要返回散文替代 JSON。
- 不要因为「用户没说清楚」就空着 `scope_in`——你的职责就是把它补全。

## 注意

- `create` 的 `scope_in` 会写入课题，供后续 `assistant.complete_scope` 对照勾选。
- 本步骤是创建课题的唯一入口；`scope_in` 缺失不会被静默兜底（仅 `switch` 分支缺失时才有兜底），因此 `create` 必须自带合法 `scope_in`。
