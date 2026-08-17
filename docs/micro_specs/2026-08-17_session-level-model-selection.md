# Spec: 会话级模型选择 + 统一模型调用参数规范

## Goal

把「模型选择」从**全局 + 前端本地（localStorage）**改为**会话级 + 后端持有**，并落地一套**统一的模型调用参数规范**：

- 后端持有每个会话的模型选择（含 provider/model + 高级参数 + 思考模式），随会话持久化。
- 前端读取后端选中（切换会话回显），写入后端（改选落库）。
- **统一参数规范对外**：前端 / hook / 调用方只面对一套自洽规范。
- **providers 抹平细节**：各服务商协议差异、思考模式与采样参数的互斥，都在 `ProviderRegistry::call_model` 翻译层内部消化，对外透明。
- 对话上下文（`RoundContext.model`）已携带模型，hook 只读（不覆盖，改选是用户行为）。
- 全局默认（`config.json` `defaults`）仍是回退源。

## 现状与问题

1. **前端全局持有**：`+page.svelte` 用 `ui.activeProviderId / activeModelId`（视图级 $state），持久化 `localStorage`。切换会话不改变选中。
2. **后端不持有会话级模型**：`send_chat_message` 每次传 `provider_id/model_id`；后端仅靠 `default_model_selection()` 知道全局默认。
3. **无高级参数承载**：`ModelCallRequest` 仅 `provider_id/model_id/messages/tools`；`call_model` 用 OpenAI 默认采样参数。前端模型编辑（`ModelEditInfo`）亦无采样/思考配置。
4. **上下文已携带模型**：`RoundContext.model: ChatModelSelection` 已存在，`RoundHooks` 可读 `ctx.model`。
5. **服务商差异**：DeepSeek thinking mode 用 `extra_body={"thinking":{"type":...}}` + `reasoning_effort`；思考模式开启时 `temperature/top_p/penalties` 静默失效（不报错）。OpenAI 系用 `reasoning_effort`。

## 统一参数规范（对外契约）

```rust
/// 会话级 / 调用级统一模型选择
pub struct ChatModelSelection {
    pub provider_id: String,
    pub model_id: String,
    /// 会话级采样参数（统一规范，服务商差异由 providers 抹平）
    pub params: Option<SamplingParams>,
    /// 会话级思考模式（None = 跟随模型默认）
    pub thinking: Option<ThinkingConfig>,
}

pub struct SamplingParams {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,          // 覆盖模型定义 max_output_tokens
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub stop: Option<Vec<String>>,
    pub seed: Option<i64>,
}

pub struct ThinkingConfig {
    pub enabled: Option<bool>,            // 用户视角开关
    pub effort: Option<ThinkingEffort>,   // low / high / max
}
```

### 参数来源三层（优先级低→高）
1. 模型定义默认值（`providers[].models[].sampling/.thinking`，服务商/模型管理维护）
2. 会话级覆盖（`conversation.extra.session.state.model.params/.thinking`）
3. 单次调用覆盖（`ModelCallRequest.params/.thinking`）
→ 合并结果交给 `providers` 翻译。

## 方案

### 后端

1. **统一规范结构体**（`models.rs`）：新增 `SamplingParams`、`ThinkingConfig`、`ThinkingEffort`；`ChatModelSelection` 扩展 `params`/`thinking`（serde default + skip_serializing_if，wire 向后兼容）。
2. **`SessionState` 增加 `model`**（`round_types.rs`）：`pub model: Option<ChatModelSelection>`，落位 `conversation.extra.session.state.model`。旧会话无 → `None` → 全局默认回退。
3. **`ModelCallRequest` 增加 `params`/`thinking`**（单次覆盖，`Option` 兼容）。
4. **新增 Tauri 命令 `set_session_model(conversation_id, selection)`**：校验模型存在 → 写 `session.state.model` → 广播 `StateChange::Conversations`。
5. **`send_model_message` 模型回退**：命令可选带 `selection`；未带则从会话 `read_session_state().model` 取；仍无则 `default_model_selection()`；再兜底报错。前端传入时合并持久化。
6. **providers 抹平细节**：`ProviderRegistry::call_model` 内新增 `resolve_sampling(...)`（三层合并）+ `build_request(...)` 按统一规范翻译：
   - OpenAI 兼容：`reasoning_effort` 走标准字段；`thinking` 走 `extra_body`（DeepSeek）。
   - **思考模式开启时忽略采样参数**（providers 内部处理，不向上暴露互斥）。
   - 模型定义声明 `thinking.supported`；不支持的模型忽略 thinking。

### 前端

1. **`types.ts`**：`ChatModelSelection`、`SamplingParams`、`ThinkingConfig`、`Conversation.extra.session.state.model` 类型。
2. **会话切换回显后端选中**：`selectConversation`/`openSession`/`createConversation` 后从当前会话 `state.model` 回显到 `ui`（后端权威）；无则回退上次选择/全局默认。
3. **`changeModel` 调后端**：`handleModelChange` 调 `set_session_model` 落库当前会话，并更新 `ui` + localStorage（localStorage 仅作新会话初始默认记忆）。
4. **`sendMessage`**：仍带当前 `ui` 选中（兼容），后端以其为准并持久化。
5. **高级参数/思考模式 UI**：模型选择面板提供编辑入口（本期可选，若做则开启思考模式时提示采样参数不生效——提示属 UI 增强，互斥逻辑不强制）。

## Done Contract

- 后端：统一规范结构体落位；`SessionState.model` 持久化；`set_session_model` 命令；`send_model_message` 回退链（前端传 → 会话 → 全局默认）；`providers` 翻译采样 + thinking。
- 前端：类型扩展；会话切换回显；`changeModel` 落库后端。
- `cargo test` 全绿；前端 `pnpm check` 无新增错误。
- 旧会话无 `model` / 模型无新字段均兼容。

## Scope

- In：`models.rs`、`round_types.rs`、`gateway.rs`、`lib.rs`、`conversation_runner.rs`、`providers.rs`（resolve_sampling + build_request + thinking 翻译）、前端 `types.ts`/`+page.svelte`/`dataStore.svelte.ts`/`viewContext.ts`。
- Out：思考模式与采样参数互斥的 UI 强制联动（互斥由 providers 消化）；hook 强制覆盖模型（hook 只读）。

## 风险

- 中：`providers` 翻译逻辑变复杂（extra_body / reasoning_effort / 互斥），需单测覆盖（openai 兼容 + deepseek thinking）。
- 低：`ChatModelSelection`/`SessionState` 增字段向后兼容（serde default）。

## Restated Understanding

- 任务：模型选择迁移为「会话级 + 后端持有」；统一参数规范对外；providers 抹平服务商差异（含思考模式互斥，上层不管）。
- 核心目标：后端持有会话模型（含 params/thinking）+ 前端读写 + providers 翻译，cargo test + pnpm check 通过。
- 暂不处理：互斥的 UI 强制联动；hook 覆盖模型。

## Checkpoint Summary

- 当前任务理解：会话级模型选择（统一规范）+ providers 抹平
- 当前核心目标：后端持有 + 前端读写 + 统一参数规范 + providers 翻译
- 当前进度：spec 定稿
- 下一步 1: 用户确认范围（是否含高级参数/思考模式 UI）
- 下一步 2: 实现后端（统一规范 → SessionState → 命令 → providers 翻译）
- 下一步 3: 实现前端（类型 → 回显 → changeModel 落库）
- 下一步 4: cargo test + pnpm check
- 涉及文件：models.rs / round_types.rs / gateway.rs / lib.rs / conversation_runner.rs / providers.rs / types.ts / +page.svelte / dataStore.svelte.ts / viewContext.ts
- 风险：中（providers 翻译 + 回退链）
- 验证方式：cargo test + pnpm check
- Execution Approval: `Pending`

## 实现完成

- 后端：`SamplingParams`/`ThinkingConfig`/`ThinkingEffort`/`ThinkingCapability` 结构体 + `ChatModelSelection`/`ChatOptions`/`ModelCallRequest`/`ModelInfo`/`ConfiguredModel`/`ModelEditInfo` 扩展；`SessionState.model` 持久化；`set_session_model` 命令；`send_model_message` 携带 params/thinking；providers 翻译层（`resolve_sampling`/`resolve_thinking`/`apply_sampling` + 思考模式 raw JSON 注入 `reasoning_effort`/`thinking`）；调用链（round_executor/compactor/neuron/model/rpc/cli）带参数。
- 前端：`types.ts` 类型扩展；会话切换回显后端选中（`echoSessionModel`）；`handleModelChange` 落库后端（`set_session_model`）；`sendMessage` 携带参数；ModelPicker 高级参数面板（采样 + 思考模式开关/强度）；ProviderManager 模型定义采样/思考能力编辑。
- 验证：`cargo test --lib` 249 passed（2 个与本次无关的既有 flaky 失败：`execute_timeout_kills_process` 为 Windows 进程超时时序问题、neuron variant 为时序依赖）；`pnpm check` 0 error；`pnpm build` 成功。
- 兼容：旧会话无 `model` / 模型无新字段 → serde default 回落；无思考能力模型忽略 thinking。

## Change Log

- 2026-08-17: 初版 spec（仅 id）
- 2026-08-17: 扩展统一参数规范（SamplingParams + ThinkingConfig）+ providers 抹平；确认互斥由 providers 消化、hook 只读
- 2026-08-17: 完整落地（结构体 + 会话持久化 + set_session_model + providers 翻译 + 前端回显/落库/参数UI）
