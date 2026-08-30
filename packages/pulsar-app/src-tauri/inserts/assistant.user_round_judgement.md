# assistant.user_round_judgement

## 工具

用户轮合并裁决：一次输出同时完成①上一轮介入效果打分与②课题路由（切换已有课题 / 新建课题 / 维持现状）。

## 对模型的期待

只返回一个 JSON 对象，**必须包含全部顶层字段** `score`、`action`、`topic_id`、`name`、`description`、`scope_in`（不适用字段置 null）。`action` 缺失或非法时按 `none` 处理（不创建、不切换）。

### 打分（score）

```json
{"score": 2, "action": "none", "topic_id": null, "name": null, "description": null, "scope_in": null}
```

- `neuron_ids` 非空才评分：正分 = 介入有帮助，负分 = 有害 / 跑偏；信息不足输出最小正分 1，而不是 0。
- `neuron_ids` 为空（无可评区间）时 `score` 置 0。
- 分数直接加到神经元及其相关边的权重上，判定须谨慎。

### 切换已有课题（switch）

```json
{"score": 0, "action": "switch", "topic_id": "topic_…", "name": null, "description": null, "scope_in": null}
```

- `topic_id` 必须来自输入列表中的未完成课题；给出的 `topic_id` 不在列表中时，系统会回退为新建（带兜底 scope），但**不要依赖该回退**，应优先匹配真实存在的课题。

### 新建课题（create，任务拆解）

```json
{
  "score": 0,
  "action": "create",
  "topic_id": null,
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
- `done_contract`：可判定的完成标准，不是口号；例如「列出 10 本书并附一句话理由」比「推荐好书」更可验收。
- 拆解粒度：服务后续推进与勾选完成，避免空泛单项「做完整个需求」。
- `name` / `description` 缺省时回退为用户输入原文，但 `scope_in` 无回退，务必给出。

### 维持现状（none）

```json
{"score": 0, "action": "none", "topic_id": null, "name": null, "description": null, "scope_in": null}
```

- 当前已绑定课题且输入仍在课题范围内（无跨课题信号）时选择 none。

## 忌用

- `create` 时不要省略 `scope_in` 或只写 name。
- 不要发明不存在的 `topic_id`。
- 不要返回散文替代 JSON，不要省略任何顶层字段。
- 不要因为「用户没说清楚」就空着 `scope_in`——你的职责就是把它补全。

## 注意

- `create` 的 `scope_in` 会写入课题，供后续 `assistant.round_review` 对照勾选。
- 本步骤是创建课题的唯一入口；`scope_in` 缺失不会被静默兜底（仅 `switch` 分支缺失时才有兜底），因此 `create` 必须自带合法 `scope_in`。
