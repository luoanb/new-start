# Micro Spec: response_format 严格对齐 OpenAI 官方 json_schema 契约

- 日期：2026-08-23
- 状态：已执行
- 深度：fast

## 执行结论（Reverse Sync）

- `providers.rs` `apply_response_format`：`JsonSchema` 分支改为官方三层包装
  `{"type":"json_schema","json_schema":{"name","strict","schema":{...}}}`；`name` 由调用方提供，
  **不硬编码**（修正：`ResponseFormatSpec::JsonSchema` 由元组改为 `{name, schema}` 结构体变体，
  各 hook 定义处提供 `complete_scope` / `match_topic` / `revise_topic` / `score_feedback`）。
  `JsonObject` 分支不变。补 3 个单测（wire 形态 / json_object / none noop）。
- `judgement.rs` 4 个 schema 补 `additionalProperties: false`；
  `MATCH_TOPIC_SCHEMA` 额外升级 strict 兼容（全字段 required、可选字段 `["T","null"]` 联合、
  `scope_in` 补 `items`）。补 schema 校验单测（可解析 + 顶层 `additionalProperties: false`）。
- `assistant_session.rs` `parse_scope_in_from_decision`：`scope_in` 为 `null` 时返回空 `Vec`。
- `openai_compat.rs` / `providers.rs` 注释同步为官方 wire 描述。
- 验证：`cargo test --lib` 全量通过（407 passed）。

## 目标

provider 层（`apply_response_format`）生成的 `response_format` wire 形态严格对齐 OpenAI
Structured Outputs 官方契约，修复 `provider returned 400 Bad Request: ... response_format:
missing field 'schema'`。

## 决策（三条原则）

1. **provider 对外提供的 API 严格对齐 OpenAI 官方规范**：`apply_response_format` 生成的
   `response_format` wire 形态 = OpenAI 官方 `json_schema` 三层结构（`name`/`strict`/`schema`）。
2. **内部允许基于服务商的特殊翻译**：wire 层可按具体服务商（openai / deepseek / ollama /
   custom）做特殊化处理；本次统一官方形态，翻译点预留（按 provider 能力差异后续可加）。
3. **外部调用策略根据 provider 对外提供的 API 决定**：调用方（hook 侧 / round_executor）基于
   provider 暴露的契约（能力探测 `model_capabilities`）决定是否携带 `response_format`、
   失败后如何降级（`neutral_fallback` 兜底），不在这层强制降级。

## 涉及文件

1. `packages/pulsar-app/src-tauri/src/core/providers.rs` — `apply_response_format` wire 包装
2. `packages/pulsar-app/src-tauri/src/core/openai_compat.rs` — `ResponseFormatSpec` 文档注释
3. `packages/pulsar-app/src-tauri/src/core/hook/judgement.rs` — 4 个 hook schema 补 `additionalProperties: false`

## 改动要点

### 1. wire 形态（providers.rs `apply_response_format`）

`JsonSchema` 分支由裸 schema 直接作为 `json_schema` 值，改为官方三层包装：

```json
{
  "type": "json_schema",
  "json_schema": {
    "name": "hook_judgement",
    "strict": true,
    "schema": { ...原 schema 对象... }
  }
}
```

- `name` 固定 `"hook_judgement"`（hook 裁决共用；如需细分再演进）。
- `strict: true`（官方结构化输出默认要求）。
- `JsonObject` 分支不变：`{"type":"json_object"}`。

### 2. hook schema 补 `additionalProperties: false` + strict 兼容

OpenAI strict 模式要求每个 object 节点声明 `additionalProperties: false`；且
**所有属性必须在 `required`**（可选用 `["T","null"]` 联合表达），**数组必须声明 `items`**。

- `COMPLETE_SCOPE_SCHEMA`：顶层 object 加 `additionalProperties: false`（已全 required）。
- `MATCH_TOPIC_SCHEMA`：顶层加；且升级 strict 兼容：
  - `topic_id` / `name` / `description` → `["string","null"]`；`scope_in` → `["array","null"]`（带 `items`）
  - 全部属性进 `required`
- `REVISE_TOPIC_SCHEMA`：顶层 + `add_items` items + `update_items` items 三个 object 节点加。
- `SCORE_FEEDBACK_SCHEMA`：顶层 object 加。

### 2.1 消费端 null 容错（assistant_session.rs）

`parse_scope_in_from_decision`：`scope_in` 为 `null` 时返回空 `Vec`（与缺失同语义），
避免 strict 模式的 `["array","null"]` 联合导致反序列化失败。

### 3. 注释修正

- `openai_compat.rs` `ResponseFormatSpec` 文档（L28-30）：wire 形态描述改为官方三层结构。
- `providers.rs` `apply_response_format` 文档（L1121-1124）：同步。

## 验证方式

- `cargo test`（pulsar-app src-tauri）：现有 4 个 hook schema 相关测试 +
  新增 `apply_response_format` wire 形态单测（JsonSchema 分支断言三层结构）。
- 校验 4 个 schema JSON 均可反序列化为对象且含 `additionalProperties: false`。

## 风险

- 老服务商若不支持 `json_schema` / `strict` 会报错；按决策，该层不兼容，由调用方降级
  （hook 已有 `neutral_fallback` 兜底；能力探测链可后续评估）。
- schema 补 `additionalProperties: false` 不影响本地解析（fallback 均只读已知键）。

## Done Contract

- [ ] `cargo test` 通过
- [ ] wire 形态单测断言 `json_schema.name` / `strict` / `schema` 存在
- [ ] 4 个 schema 均含 `additionalProperties: false`
