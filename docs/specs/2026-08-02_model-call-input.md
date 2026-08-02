# Spec: ModelCallInput（模型入参 · 消息列表）

## Goal

- 要解决什么问题：各处调模型前各自拼消息列表，缺少统一、无业务依赖的入参工具；后续要收束为「装配完成后直接 `call_model`」，本期先把**消息列表**管起来。
- 验收结果：存在静态工具类 `ModelCallInput`，契约如下；不查 insert、不改原列表；方案与后续实现以此为准。

## Done Contract

- 什么算完成：本 spec 落盘且 API 契约无歧义；按本契约交付静态类 + 单测。
- 由什么证明：文档审阅通过；`cargo test model_call_input` 覆盖三种列表模式与 `with_user_input_for_append`。
- 哪些情况仍算未完成：组装完整 `ModelCallRequest`、查 `InsertCatalog`、全量替换现网调用点（属后续）。

## Scope

### In

- 静态工具类 `ModelCallInput`（模型入参管理；本期方法仅操作消息列表）。
- `history` 必传，允许空（新对话）。
- 不可变：不修改调用方传入的列表，返回新 `Vec<ModelMessage>`。
- 无外部业务：不依赖 Neuron / Topic / InsertCatalog / Provider。
- 方法：`replace_system`（firstSystemPrompt）、`append`、`insert_at`、`with_user_input_for_append`。

### Out

- 自行查询 insert / `system_type → insert` 映射。
- 本期组装 `provider_id` / `model_id` / `tools` 或完整 `ModelCallRequest`（后续扩展）。
- 本期强制改写 `NeuronModelCaller` / Assistant / Engine 全部调用点（接入另开迭代）。

## Facts / Constraints

- 现网模型调用最终形态为 `ModelCallRequest { provider_id, model_id, messages, tools? }` → `ProviderRegistry::call_model`。
- 说明书正文若需要拼进消息，由**调用方**先取好再传入本工具（本类不查 insert）。
- 项目约定：可复用工具逻辑以**类 / 静态方法**为主。

## Restated Understanding

- 我理解当前任务是：先固化 `ModelCallInput` 的消息列表 API，再实现。
- 当前核心目标是：统一、不可变的消息列表手术 + 与用户输入拼接。
- 当前边界是：只管 messages；不查 insert；不碰其它模型入参字段。
- 暂不处理：全链路强制接入、完整 Request 装配。

## 接口契约设计

```rust
/// 模型入参工具（静态）。本期只提供消息列表相关方法；后续可扩展 provider/model/tools。
pub struct ModelCallInput;

impl ModelCallInput {
    /// firstSystemPrompt：替换消息列表中的系统提示词。
    /// - 若存在 role=System 的消息：用新内容替换**第一条** System（其余消息顺序保留）。
    /// - 若列表为空或不存在 System：在新列表**头部**插入一条 System。
    /// - 不修改 `history`；返回新 Vec。
    pub fn replace_system(
        history: &[ModelMessage],
        system_prompt: &str,
    ) -> Vec<ModelMessage>;

    /// 在末尾追加一条消息。
    pub fn append(
        history: &[ModelMessage],
        message: ModelMessage,
    ) -> Vec<ModelMessage>;

    /// 在指定位置插入提示词消息。
    /// - 保留 `[0..index)`；
    /// - 在 `index` 放入 `message`；
    /// - **原 index 及之后的消息全部舍弃**。
    /// - `index > history.len()` → 视为错误（或等价：不允许越界；实现用 `AppResult`）。
    /// - `index == history.len()` → 等价于在末尾插入（无舍弃），与「该位置无可舍弃」一致。
    pub fn insert_at(
        history: &[ModelMessage],
        index: usize,
        message: ModelMessage,
    ) -> AppResult<Vec<ModelMessage>>;

    /// 将指定内容与用户输入拼接为一段字符串，供随后 append 成 User 消息。
    /// 默认分隔：`"{content}\n\n{user_input}"`。
    /// `content` 或 `user_input` 为空时的行为：仍拼接，但避免多余空行
    ///（两边都非空才插入 `\n\n`；仅一侧非空则返回非空侧）。
    pub fn with_user_input_for_append(
        content: &str,
        user_input: &str,
    ) -> String;
}
```

### 行为表

| 方法 | 入参 | 出参 | 要点 |
|------|------|------|------|
| `replace_system` | `history`（可空）, `system_prompt` | 新消息列表 | 空对话 / 无 System → 头部新建 System |
| `append` | `history`, `message` | 新消息列表 | 末尾加一条 |
| `insert_at` | `history`, `index`, `message` | 新消息列表 | 插入提示词；截断 index 及之后 |
| `with_user_input_for_append` | `content`, `user_input` | `String` | 不碰列表；给 append 用 |

### 与现网调用的关系（后续）

目标态：业务准备好文案 / 说明书正文 / 用户输入后，用本类得到 `messages`，再填入 `ModelCallRequest` 调 `call_model`。  
本期只交付工具本身；助手 / 神经元 / Engine 接入另开。

## 后续规划（非本期）

- 扩展 `ModelCallInput`：管理 `provider_id` / `model_id` / `tools`，最终直接产出可 `call_model` 的完整入参。
- 取消 `NeuronModelCaller`「两段字符串 + 内部 default model」旁路，统一走入参装配。
- 调用方仍负责：查 insert、ensure neuron、选模型、解析响应。

## Checkpoint Summary

- 当前任务理解：模型入参静态工具，先管消息列表。
- 当前核心目标：实现 `ModelCallInput` + 单测并通过。
- 当前进度：实现已落地。
- 涉及文件 / 模块：`packages/agent-app/src-tauri/src/core/model_call_input.rs`。
- 风险：`replace_system` 若存在多条 System，只替换第一条——需调用方知悉。
- 验证方式：`cargo test model_call_input`。
- Execution Approval: `Approved`（用户 2026-08-02 明确「开始落地代码」）

## Change Log

- 2026-08-02: 对齐并落盘。不查 insert；history 必传可空；类名 `ModelCallInput`；`replace_system` / `append` / `insert_at` / `with_user_input_for_append`；本期仅消息列表。
- 2026-08-02: 实现 `model_call_input.rs` + 单元测试；`core/mod.rs` 导出 `ModelCallInput`。未接入现网调用点。

## Validation

- Self-check: 与对话对齐点一致（不查 insert、不可变、insert_at 含 message、命名 ModelCallInput）。
- Human confirmation: 用户批准实现。
- 证据: `cargo test --manifest-path packages/agent-app/src-tauri/Cargo.toml model_call_input` → `9 passed; 0 failed`。
- 核心目标是否已由证据证明完成: **是**（本期消息列表工具 + 单测）。

## Resume / Handoff

- 当前状态: 上期消息列表工具 Done；签名已在调用点迭代演进为三参 + `Neuron`/`Manual`。
- 调用点接入: 见 [`2026-08-02_19-29_model-call-input-call-sites.md`](./2026-08-02_19-29_model-call-input-call-sites.md)（已落地）。
