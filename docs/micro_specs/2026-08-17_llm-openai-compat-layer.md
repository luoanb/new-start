# Spec: 独立 OpenAI 兼容调用层（去 async-openai）+ providers 仅做整合

## Goal

1. **完整支持 OpenAI Chat Completions 契约**（请求/响应全字段，含流式、思考模式），不受 async-openai 版本/能力限制。
2. **架构分层**：`providers.rs` 只做「服务商整合 + 参数抹平」，LLM 调用协议封装（消息序列化、请求装配、HTTP 发送、响应解析、SSE 流式）**独立成文件**。
3. 去掉 `async-openai` 依赖（当前 `call_with_thinking` 已绕过其客户端，主路径 `call_model` 仍用它）。

## 现状（async-openai 在 providers.rs 的全部使用面）

| 位置 | 用途 | async-openai 类型 |
|---|---|---|
| `call_model`（315-377） | 客户端发送 + 请求装配 | `Client`、`OpenAIConfig`、`CreateChatCompletionRequestArgs`、`ChatCompletionTool`/`FunctionObject`/`ChatCompletionTools` |
| `to_chat_message`（942-994） | 消息转换（System/User/Assistant+tool_calls/Tool+tool_call_id） | `ChatCompletionRequest*MessageArgs`、`*MessageContent`、`ChatCompletionMessageToolCall`/`ToolCalls`/`FunctionCall` |
| `parse_chat_response`（438-564） | 响应解析（content/tool_calls/finish_reason/usage/refusal/reasoning_tokens） | `CreateChatCompletionResponse`、`FinishReason`、`ChatCompletionMessageToolCalls` |
| `build_openai_config`（996-1001） | api_key/api_base | `OpenAIConfig` |
| `call_with_thinking`（379+） | 思考模式 raw JSON 注入 + reqwest 直发 | 复用上面类型做反序列化 |

**当前缺口**：主路径（非思考模式）仍走 async-openai builder+client，无法设置 `reasoning_effort`/`extra_body`/流式；`streaming` 能力仅是展示字段，无流式实现。

## 目标架构

```
providers.rs（整合层：保留）
├─ ProviderRegistry：config 读写、服务商/模型管理、default_model_selection
├─ 参数抹平：resolve_sampling / resolve_thinking（模型定义默认 → 调用覆盖）
└─ call_model()：调用层封装 → 组装统一规范 → 交给 openai_compat 发送

openai_compat.rs（新：调用协议封装层，不依赖 async-openai）
├─ 请求类型：OpenAiChatRequest（完整契约，serde）
├─ 消息类型：OpenAiMessage（system/user/assistant/tool，含 tool_calls、tool_call_id、reasoning）
├─ 工具类型：OpenAiTool / FunctionObject / ToolChoice
├─ 响应类型：OpenAiChatResponse / Choice / Usage（含 reasoning_tokens、refusal、system_fingerprint）
├─ 发送：send_non_stream()  → reqwest POST /chat/completions（Bearer）
├─ 流式：send_stream()      → SSE 逐 chunk 解析（content、reasoning_content、tool_calls delta、finish_reason）
└─ 解析：到 core::models 的映射（ModelCallResponse / ToolCall / ThinkingChunk）
```

**边界**：`openai_compat.rs` 只做「OpenAI 兼容协议」的序列化/反序列化/HTTP，**不含**服务商策略；`providers.rs` 决定用哪个服务商、怎么抹平、走流式与否。两个 kind（`OpenAi`/`OpenAiCompatible`）都走此层（同协议）。

## 完整 OpenAI 契约字段清单（openai_compat 需支持）

### 请求 `POST /chat/completions`
- 必填：`model`、`messages`
- 消息 role：`system`/`user`/`assistant`/`tool`；content 支持 `string` 与多模态 `[{type,text,image_url,input_audio}]`；`assistant` 可带 `tool_calls` + 可选 `reasoning_content`（DeepSeek 多轮工具调用需回传）；`tool` 带 `tool_call_id`
- 采样：`temperature`、`top_p`、`max_tokens`（旧）/`max_completion_tokens`（推理模型）、`stop`、`presence_penalty`、`frequency_penalty`、`seed`、`logit_bias`
- 工具：`tools`（function，含 `parameters` JSON Schema、`strict`）、`tool_choice`、`parallel_tool_calls`
- 推理/思考：`reasoning_effort`（low/medium/high）、DeepSeek `thinking`（extra_body）
- 结构化：`response_format`（json_object/json_schema）
- 其他：`stream`（true/false）、`stream_options`（include_usage）、`n`、`logprobs`/`top_logprobs`、`user`、`service_tier`、`store`
- 请求可保留**未知字段透传**能力（`extra_body`），支持各家扩展

### 响应 `200`
- 顶层：`id`、`object`、`created`、`model`、`system_fingerprint`、`usage`（prompt/completion/total/completion_tokens_details.reasoning_tokens）
- `choices[]`：`index`、`finish_reason`（stop/length/tool_calls/content_filter/function_call）、`message`（`role`、`content`、`reasoning_content`、`tool_calls`、`refusal`、`annotations`）、`logprobs`
- 流式 chunk：`choices[].delta`（content/reasoning_content/tool_calls/role）、`finish_reason`、`usage`（最后 chunk / stream_options）
- tool_calls：`id`、`type`（function）、`function{name,arguments}`

### 错误响应
- 非 2xx：结构化错误（`error{message,type,code,param}`）+ HTTP 状态；429/5xx 幂等/重试提示

## 改造步骤

1. **新建 `src/core/openai_compat.rs`**：实现上述请求/响应/消息/工具类型 + `send_non_stream` + `send_stream`（SSE）+ 解析函数。纯 `serde` + `reqwest`，零 async-openai。
2. **`providers.rs` 瘦身**：
   - 删 async-openai import；`to_chat_message`/`parse_chat_response`/`build_openai_config` 改调 openai_compat。
   - `call_model`：组装 `OpenAiChatRequest` → `openai_compat::send_non_stream`（思考模式注入 `reasoning_effort`/`thinking` 走 extra_body）。
   - 新增流式入口 `call_model_stream`（若需）供 round_executor 使用；`ModelCapabilities.streaming` 从此被消费。
3. **`Cargo.toml`**：移除 `async-openai`。
4. **测试**：`cargo test`（providers/round_executor 现有用例）；新增 openai_compat 消息序列化/响应解析单测。
5. **边界**：删除后 `call_with_thinking` 合并进主路径（思考模式成为 `extra_body` 的一个注入分支，不再有独立函数）。

## Done Contract

- 无 async-openai 依赖；`providers.rs` 编译通过且只做整合。
- 非流式 + 流式（SSE）路径可用；思考模式（reasoning_effort/thinking/reasoning_content）在两种模式都支持。
- 现有 `cargo test --lib` 全绿（排除既有 flaky），`pnpm check` 0 error。

## 风险

- 中：消息/响应序列化边界（多模态、复杂 tool_calls、流式 delta 拼接）需单测覆盖。
- 中：流式为新增能力，需 SSE 解析正确性测试。
- 低：`max_tokens` vs `max_completion_tokens` 双字段语义需按模型/时代处理。

## 范围

- In：`openai_compat.rs`（新）、`providers.rs`（瘦身）、`Cargo.toml`、`core/mod.rs`（模块注册）。
- Out：多模态 `image_url`/`input_audio` 的上传层（仅类型支持，不上传媒体）；logit_bias 的 UI；response_format 的 UI（仅协议层支持）。

## 实现完成

- 新建 `core/openai_compat.rs`：完整 OpenAI Chat Completions 契约类型（消息/工具/请求/非流式响应/流式 chunk）+ `Client::chat`（非流式）+ `chat_stream`（SSE 逐 chunk）+ 错误归一（结构化 error 解析）。纯 `serde`+`reqwest`，零 async-openai。
- `providers.rs` 瘦身：删 async-openai；`call_model` 改用 `openai_compat::Client`；`to_chat_messages`/`apply_sampling`/`apply_tools`/`apply_thinking` 落到 openai_compat 类型；`parse_chat_response` 适配 `ChatResponse`。新增 `call_model_stream`（流式入口，供后续 round_executor 接入）。保留 `resolve_sampling`/`resolve_thinking`/`thinking_effort_wire`/`model_runtime_spec` 等抹平逻辑。
- `ModelMessage` 增加 `reasoning_content`（推理思维链，多轮工具调用回传）。
- `Cargo.toml` 移除 `async-openai`；`reqwest` 增加 `stream` feature。
- `core/mod.rs` 注册 `openai_compat` 模块。
- 验证：`cargo check --all-targets` 通过；`cargo test --lib` 253 passed（含 openai_compat 4 测试 + providers 9 测试；唯一失败为既有 Windows flaky `execute_timeout_kills_process`，与本次无关）。因删除依赖需 `cargo clean` 后全量重编以消除 rustc 缓存 ICE。

## Change Log

- 2026-08-17: 初版方案（独立 OpenAI 兼容调用层 + providers 仅整合 + 完整契约）
- 2026-08-17: 完整落地（openai_compat.rs + providers 瘦身 + 移除 async-openai + reasoning_content 回传 + 流式 API）
