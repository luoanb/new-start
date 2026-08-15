# Spec: 神经元稳定系统提示词 + 独立角色消息（B2）

## Goal

- 要解决什么问题：当前每轮将选中 neuron.content 替换 System 消息，导致首条 System 提示词频繁变化，模型 KV 缓存失效、角色不一致。
- 验收结果：首轮选中的 neuron.content 冻结为稳定 System 提示词，后续轮次选中的神经元作为独立 Context 消息插入（与真实用户输入区分），System 消息不再变化。

## Done Contract

- 什么算完成：
  1. `SessionState` 新增 `stable_system_prompt` / `stable_system_frozen` 字段，首轮选中后冻结
  2. `resolve_role` 在已冻结时返回 `stable_system_prompt` 而非新选中 neuron.content
  3. `assemble` 新增 `assemble_with_context` 变体，在历史之后、用户输入之前插入 Context 消息（`User` 角色 + `[当前角色]` 前缀）
  4. 现有 `assemble` 调用点（裁决类）不受影响
  5. 现有测试全部通过，新增测试覆盖首轮冻结、后续轮 Context 插入、直连无影响场景
- 由什么证明：cargo test 通过 + 人工确认改造前后消息结构符合预期
- 哪些情况仍算未完成：任一模块改动未完成、测试失败、现有 assemble 调用点被误改

## Scope

- In:
  - `call_service.rs` — `SessionState` 新增字段、`resolve_role` 冻结逻辑、`converse` 传 context
  - `model_call_input.rs` — 新增 `assemble_with_context`
  - 测试用例更新
- Out:
  - 裁决类调用（`try_llm_select`、`generate_drafts`、`call_judgement`）的行为不变
  - 前端 UI 改动
  - 管理面关于系统提示词的配置入口
  - `config.json` 默认值变更

## Facts / Constraints

- 已确认事实：
  - 当前 `resolve_role` 返回 `role_system = selected_neuron.content`，每轮不同
  - 当前 `assemble` 在非空历史时用 `replace_system` 替换首条 System 消息
  - `session.assistant_dialogue`（2026-08-15 已移除）的 content 为空字符串，仅作 behavior 载体
  - `SessionState` 通过 `conversation.extra.session.state` 持久化
- 技术/业务约束：
  - Context 消息直接使用 `ModelMessageRole::User` + `[当前角色]` 前缀，不新增角色变体
  - `stable_system_prompt` 持久化字段需 `#[serde(default)]` 保证向后兼容（旧数据无此字段）
  - 首轮 `context` 传 `None`（因为已进入 stable_system_prompt），不产生重复
- 已知风险：
  - 新增 `Context` 消息会增加总 token 消耗（每轮多一条 neuron content）
  - 模型需理解 `[当前角色]` 前缀语义（可通过模板引导缓解）

## Open Questions

- [x] `Context` 消息用什么角色？→ 直接用 `User` + `[当前角色]` 前缀，不新增角色变体
- [x] 首轮冻结时机？→ 选完就冻（`resolve_role` 内），不依赖模型调用成功
- [x] 持久化兼容？→ `#[serde(default)]` 保证旧数据反序列化正常

## Restated Understanding

- 我理解当前任务是：改造核心提示词拼接机制，将首轮选中 neuron 冻结为稳定系统提示词，后续轮选中 neuron 拆为独立 Context 消息。
- 当前核心目标是：完成 B2 方案的代码实现，通过测试验证。
- 当前边界是：只改主对话路径（`converse`），不改裁决类路径；不改前端和管理面。
- 暂不处理：`session.assistant_dialogue` 的 content 默认值填充（2026-08-15 该内建神经元已移除，此项作废）、UI 展示 Context 消息、config 配置化。

## 接口契约设计

### `SessionState` 新增字段

```rust
pub struct SessionState {
    pub last_selected_neuron_id: Option<String>,
    pub stable_system_prompt: Option<String>,   // 新增
    pub stable_system_frozen: bool,             // 新增
}
```

### `ModelCallInput::assemble_with_context` 签名

```rust
pub fn assemble_with_context(
    history: &[ModelMessage],
    role_system: &str,
    content: &str,          // insert 正文（Manual）或 ""（Neuron）
    context: Option<&str>,  // 新增：选中 neuron.content（非首轮），作为 User 消息插入
    user_input: &str,
    template: ModelAppendTemplate,
) -> Vec<ModelMessage>
```

### `resolve_role` 冻结逻辑

```
所有分支（Global / Neuron(普通) / Neuron(系统)）统一在确定 role_system 后追加：

if !state.stable_system_frozen {
    state.stable_system_prompt = Some(role_system.clone());
    state.stable_system_frozen = true;
} else if let Some(stable) = &state.stable_system_prompt {
    role_system = stable.clone();  // 用冻结值覆盖
}
```

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是
- 若否，偏差在哪里：—
- 是否需要调整本轮目标或范围：—

## Checkpoint Summary

- 当前任务理解：B2 方案落地，首轮冻结 + 独立 Context 消息
- 当前核心目标：完成 spec 获准后进入实现
- 当前进度：spec 起草中
- 下一步 1: 用户审批 spec
- 下一步 2: 按 `call_service.rs` → `model_call_input.rs` 顺序实现
- 下一步 3: 更新测试、cargo test 验证
- 涉及文件 / 模块：
  - `packages/pulsar-app/src-tauri/src/core/call_service.rs`
  - `packages/pulsar-app/src-tauri/src/core/model_call_input.rs`
- 风险：Context 消息序列化兼容性、持久化向后兼容
- 验证方式：cargo test + 人工审查消息结构
- Execution Approval: `Approved`

## Change Log

- 2026-08-14: 初版 spec
- 2026-08-14: 实现完成
  - `SessionState` 新增 `stable_system_prompt` / `stable_system_frozen`（`#[serde(default)]` 向后兼容）
  - `resolve_role` 重构为统一返回值 + `freeze_or_replace` 后处理
  - `model_call_input.rs` 新增 `assemble_with_context`，`assemble` 委托调用
  - `converse` 已冻结时传入选中的 neuron.content 作为 context 消息
  - 不涉及 `models.rs` / `providers.rs` 改动

## Validation

- Self-check: 代码结构审查通过，新增字段均 `#[serde(default)]` 兼容旧数据
- Static checks: cargo build 通过
- Runtime / Test: cargo test 209 passed, 0 failed
- Human confirmation:
- 结果汇总：全部 Done Contract 5 项完成
- 核心目标是否已由证据证明完成：是，209 测试通过
- 若未完成，当前剩余差距：—
- 剩余风险：Context 消息增加 token 消耗（设计预期内）

## Resume / Handoff

- 当前状态：spec 草稿，待审批
- 当前卡点：—
- 下一步唯一动作：用户审批后进入实现
- 下一轮核心目标：完成 B2 方案代码实现