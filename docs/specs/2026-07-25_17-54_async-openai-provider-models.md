# Spec: async-openai 服务商与模型调用能力

## Goal

- 要解决什么问题：
  基于 `async-openai` 为 `agent-app` 增加服务商配置、模型列表查询/展示、指定模型调用的能力，替换当前本地确定性回复的 runtime 基础能力。
- 验收结果：
  core 能维护服务商与模型信息，入口层能查询服务商/模型列表，并能通过选定服务商与模型发起一次真实 LLM 调用。

## Done Contract

- 什么算完成：
  文档、core API、配置模型、provider runtime、Tauri/CLI/TUI 查询与调用链路均接通。
- 由什么证明：
  Rust 测试覆盖 provider/model 配置与调用参数构造；CLI/Tauri 至少完成一次服务商/模型查询 smoke；有可手动配置 API key/base URL 的说明。
- 哪些情况仍算未完成：
  只能固定单一 OpenAI 模型调用、入口层无法选择模型、错误未结构化返回、或服务商配置散落在入口层。

## Scope

- In:
  - 引入 `async-openai` 作为第一版 OpenAI-compatible provider client。
  - 定义服务商、模型、调用请求、调用结果的数据结构。
  - 支持内置服务商列表：OpenAI、DeepSeek、Ollama-compatible、Custom OpenAI-compatible。
  - 支持模型列表：先使用本地配置/内置清单，不依赖远端 `models` API 作为唯一来源。
  - 让 `Gateway::send_message` 走 provider-backed runtime。
  - 让 Tauri/CLI/TUI 暴露服务商列表、模型列表、指定模型调用。
- Out:
  - 完整工具调用循环。
  - Streaming UI。
  - 多轮 tool calling。
  - 远端模型列表自动同步。
  - 多 provider 并发调度/路由。
  - 凭据加密存储。
  - 本地 session 聊天工作流改造。
  - 将模型调用结果自动写入会话历史。

## Facts / Constraints

- 已确认事实：
  - 当前 `AgentRuntime::respond` 是本地确定性回复。
  - 当前 storage spec 预留了 `.agent-app/config.json`，但第一版尚未实现配置持久化。
  - 当前架构要求 core 拥有业务行为，Tauri/CLI/TUI 是薄入口。
  - `async-openai` 支持 `OpenAIConfig::with_api_base`，可接 OpenAI-compatible 服务。
- 技术/业务约束：
  - API key 不能硬编码进入代码或文档示例。
  - provider/model 选择必须进入 core API，不能只在某一个入口实现。
  - 第一版优先非 streaming 调用，降低 GUI/TUI/CLI 同步复杂度。
  - 模型列表先采用静态注册表，避免每个服务商远端 models API 行为不一致。
  - 第一版模型调用使用 Chat Completions API。
  - 第一版不设置默认 provider/model，用户必须显式选择。
  - 第一版配置来源为环境变量 + `.agent-app/config.json`。
- 已知风险：
  - OpenAI Responses API 与部分兼容服务的 Chat Completions 兼容性不完全一致。
  - Ollama/DeepSeek/OpenRouter 等 OpenAI-compatible 服务可能支持不同字段。
  - Tauri command 持有同步 `Mutex<Gateway>`，真实 async 调用前需要调整状态管理，避免长时间持锁。

## Open Questions

- [x] 第一版调用接口优先用 `responses` API 还是 `chat-completions` API？
  使用 Chat Completions API。
- [x] 默认模型是否采用 `openai:gpt-4o-mini`，还是先不设默认、要求用户选择？
  不设默认，要求用户显式选择 provider/model。
- [x] API key 第一版从环境变量读取，还是同时支持 `.agent-app/config.json`？
  支持环境变量 + `.agent-app/config.json`。

## Restated Understanding

- 我理解当前任务是：
  按 `sdd-light` 先产出方案，设计如何用 `async-openai` 增加“服务商 + 模型列表 + 模型调用”能力。
- 当前核心目标是：
  先固化 provider/model/call 的 core 边界，避免后续把模型调用逻辑散落到 Tauri、CLI、TUI。
- 当前边界是：
  只出方案与执行 checkpoint，不进入代码实现。
- 暂不处理：
  工具调用、streaming、远端模型同步、凭据加密、多 provider 自动路由。

## 接口契约设计

```rust
pub struct ProviderInfo {
    pub id: String,
    pub display_name: String,
    pub api_base: Option<String>,
    pub auth_env: String,
    pub kind: ProviderKind,
}

pub enum ProviderKind {
    OpenAi,
    OpenAiCompatible,
}

pub struct ModelInfo {
    pub id: String,
    pub provider_id: String,
    pub display_name: String,
    pub capabilities: ModelCapabilities,
}

pub struct ModelCapabilities {
    pub chat: bool,
    pub tools: bool,
    pub streaming: bool,
}

pub struct ModelCallRequest {
    pub provider_id: String,
    pub model_id: String,
    pub messages: Vec<ModelMessage>,
}

pub struct ModelMessage {
    pub role: ModelMessageRole,
    pub content: String,
}

pub enum ModelMessageRole {
    System,
    User,
    Assistant,
}

pub struct ModelCallResponse {
    pub provider_id: String,
    pub model_id: String,
    pub output: String,
}

pub trait LlmProvider {
    async fn call(&self, request: ModelCallRequest) -> AppResult<ModelCallResponse>;
}
```

建议 core API：

```rust
impl Gateway {
    pub fn list_providers(&self) -> AppResult<Vec<ProviderInfo>>;
    pub fn list_models(&self, provider_id: Option<String>) -> AppResult<Vec<ModelInfo>>;
    pub async fn call_model(
        &self,
        request: ModelCallRequest,
    ) -> AppResult<ModelCallResponse>;
}
```

边界说明：

- `call_model` 是无会话的底层 provider 调用，只关心 provider、model、messages 与模型输出。
- 会话型聊天工作流属于后续上层能力，负责读取/写入本地 session，并把 session history 转成 `ModelMessage`。
- provider 模块不得依赖本地 `conversation_id` / `session_id`，避免把存储语义泄露进模型调用层。

## Implementation Plan

1. 文档更新
   - 更新 `docs/agent-app/architecture.md`：明确 provider-backed runtime。
   - 更新 `docs/agent-app/commands.md`：增加 `providers`、`models`、`call-model --provider --model`。
   - 更新 `docs/agent-app/storage.md`：补充 `.agent-app/config.json` 第一版配置格式，并说明环境变量优先级。
   - 更新 `docs/agent-app/errors.md`：增加 `provider_not_found`、`model_not_found`、`provider_auth_missing`、`llm_request_failed`。

2. core 数据模型
   - 在 `models.rs` 增加 `ProviderInfo`、`ModelInfo`、`ModelCapabilities`、`ModelCallRequest`、`ModelCallResponse`。
   - 在 `error.rs` 增加 provider/model/LLM 调用错误。
   - 增加 provider registry，先使用静态服务商与模型清单。

3. async-openai provider
   - 引入 `async-openai` 依赖。
   - 新增 `providers` 或 `llm` 模块，封装 `OpenAIConfig::with_api_key` 与 `with_api_base`。
   - 第一版使用 Chat Completions，以兼容 DeepSeek/Ollama/OpenAI-compatible。

4. runtime 改造
   - 新增无会话的 provider-backed `call_model` 路径。
   - 当前本地 deterministic chat runtime 暂不替换。
   - 会话读写与模型调用结果写回不进入本轮实现。

5. 入口层接入
   - Tauri command 增加 `list_providers`、`list_models`、`call_model`。
   - CLI 增加 `providers`、`models [provider]`、`call-model --provider <id> --model <id> <message>`。
   - TUI 增加 `/providers`、`/models [provider]`、`/call <provider> <model> <message>`。

6. 验证
   - 单元测试 provider registry 和模型过滤。
   - 单元测试缺失 API key、未知 provider、未知 model 的错误。
   - provider 调用用 mock/test double，避免 CI 依赖真实外网。
   - 手动 smoke 使用环境变量验证真实调用。

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：
  是。当前只固化 provider/model/call 的方案，不提前进入实现。
- 若否，偏差在哪里：
  无。
- 是否需要调整本轮目标或范围：
  Open Questions 已确认；仍需等待执行批准后再进入实现。

## Checkpoint Summary

- 当前任务理解：
  需要基于 `async-openai` 设计服务商、模型列表和模型调用能力。
- 当前核心目标：
  让 core 拥有 provider/model/call 边界，所有入口复用同一套能力。
- 当前进度：
  已完成只读上下文确认与方案落盘。
- 下一步 1:
  等待用户明确批准进入实现。
- 下一步 2:
  获批后先更新 `docs/agent-app/*`，再改 Rust core。
- 涉及文件 / 模块：
  `docs/agent-app/*`、`packages/agent-app/src-tauri/src/core/*`、Tauri/CLI/TUI 入口。
- 风险：
  OpenAI-compatible 服务字段兼容性不一致；Tauri async command 与状态锁需要谨慎处理。
- 验证方式：
  Rust 单元测试 + CLI/Tauri smoke + 可选真实 provider 手动验证。
- Execution Approval: `Pending`

## Change Log

- 2026-07-25: 创建方案 spec，明确 provider/model/call 的 core 边界与实现顺序。
- 2026-07-25: 确认第一版使用 Chat Completions、不设默认模型、配置来源为环境变量 + `.agent-app/config.json`。
- 2026-07-25: 修正模型调用边界：本轮只提供无会话 `call_model(request: ModelCallRequest)`，会话聊天工作流后置。
- 2026-07-25: 实现 provider/model registry、config 读取、`async-openai` Chat Completions 调用、Tauri/CLI/TUI 入口。
- 2026-07-25: 修复快速 create/clear/create 下本地会话 ID 可能复用的问题。

## Validation

- Self-check:
  已确认 `call_model` 为无会话模型调用；会话型聊天工作流未纳入本轮。
- Static checks:
  `cargo fmt && cargo test` 通过；`pnpm check` 通过。
- Runtime / Test:
  CLI `providers` / `models deepseek` smoke 通过；TUI `/providers` / `/models deepseek` smoke 通过。
- Human confirmation:
  已确认 Open Questions，并已收到“开始执行”。
- 结果汇总：
  最小 provider/model/call 能力已完成；真实外部模型调用需用户提供 API key/base URL 后手动验证。
- 核心目标是否已由证据证明完成：
  已由静态检查、Rust 测试和 CLI/TUI smoke 证明完成最小实现目标。
- 若未完成，当前剩余差距：
  未做真实 provider 网络调用验证，未实现 streaming/tool calling/session chat workflow。
- 剩余风险：
  OpenAI-compatible 服务字段兼容性仍需在真实 provider 下验证。

## Resume / Handoff

- 当前状态：
  最小实现完成，等待后续真实 provider 配置与手动调用验证。
- 当前卡点：
  需要 API key/base URL 才能验证真实模型调用。
- 下一步唯一动作：
  使用环境变量或 `.agent-app/config.json` 配置一个 provider 后执行 `call-model` 真实调用。
- 下一轮核心目标：
  验证真实 provider 调用，随后再评估 streaming、tool calling 或 session chat workflow。
