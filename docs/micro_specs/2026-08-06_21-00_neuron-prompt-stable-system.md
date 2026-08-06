# Spec: 主对话提示词装配——首轮角色定格 System + 神经元变化才拼末尾

## Goal

- 要解决什么问题：主对话 `run_core` 每轮用**本轮选中神经元 content** 经 `ModelCallInput::assemble` 的 `replace_system` 覆盖第一条 System（[assistant_mode.rs:473](file:///home/lab/Documents/trae_projects/new-start-wt/packages/agent-app/src-tauri/src/core/assistant_mode.rs#L473)）。多轮邻域重选不同神经元时：① 首轮角色设定被覆盖（角色漂移）；② System 前缀每轮变化，provider 侧 prompt cache（按前缀）命中率下降。用户期望：**前缀稳定（缓存命中）+ 动态角色放末尾（模型对最后一条消息注意力最高）**。
- 验收结果：主对话非首轮不再覆盖第一条 System；首轮选中的神经元角色定格 System；仅当本轮选中神经元与上轮不同时，末尾 User body 才追加该神经元 content；`cargo test`（含 `assemble_stable` 新增用例）通过；`cargo check` 0 error；hook 裁决类调用行为不变。

## Done Contract

- 什么算完成：
  1. `ModelCallInput` 新增 `assemble_stable(history, role_system, content, user_input, template)`：空历史 → System = `role_system ∥ body`（与现有 `assemble` 一致）；非空历史 → **不替换**已存在的 System（仅当历史无 System 时兜底头部插入 `role_system`），body 非空则 append 为末尾 User。
  2. `assistant_mode::run_core` 改用 `assemble_stable`；`content` 取值规则：`(last_selected_neuron_id 为 Some(prev) 且 != 本轮 neuron.id)` 时取 `neuron.content`，否则空串。
  3. 现有 `assemble`、`call_system_prompt_json`、`try_llm_select`、`generate_drafts` **不动**。
  4. `model_call_input.rs` 测试补 `assemble_stable` 3 用例：空历史 fold / 非空 System 定格 / 非空但无 System 兜底插入。
- 由什么证明：`cargo test --lib`（新增用例通过 + 既有全绿）；`cargo check` 0 error。
- 哪些情况仍算未完成：hook 裁决类同样改造（不做，用户已定仅主对话）；末尾每轮都拼完整 content（不做，用户已定变化才拼）；System 每轮仍跟随最新神经元（不做，已改为首轮定格）。

## Scope

- In：`model_call_input.rs`（新增 `assemble_stable` + 单测）、`assistant_mode.rs`（`run_core` 装配）。
- Out：`call_system_prompt_json` / hook 裁决；`try_llm_select` / `generate_drafts`；前端；存储层。

## Facts / Constraints

- `assemble` 现语义（[model_call_input.rs:105-125](file:///home/lab/Documents/trae_projects/new-start-wt/packages/agent-app/src-tauri/src/core/model_call_input.rs#L105-L125)）：空历史 fold body 进 System；非空历史 `replace_system` 后 append User(body)。本 spec 只新增 `assemble_stable`，不改 `assemble`。
- `run_core` 时 `ctx.last_selected_neuron_id` 仍是**上一轮**持久化值（`persist_selected_neuron` 在 run_core 之后执行，[assistant_mode.rs:619](file:///home/lab/Documents/trae_projects/new-start-wt/packages/agent-app/src-tauri/src/core/assistant_mode.rs#L619)），因此 `prev != 本轮 id` 即"相对上轮换神经元"。
- 首轮（`last_selected_neuron_id = None`）：content 取空，角色已定格在 System。
- 非首轮且神经元未变：content 取空，末尾仅本轮输入，避免 token 累积。
- Neuron 模板 `render_neuron_template`（[model_call_input.rs:188-204](file:///home/lab/Documents/trae_projects/new-start-wt/packages/agent-app/src-tauri/src/core/model_call_input.rs#L188-L204)）已支持 `content` 非空时渲染「## 角色与能力」分节，无需改模板。

## 接口契约设计

### model_call_input.rs 新增

```rust
/// 主对话专用：首轮角色定格 System；非首轮 System 不再被替换。
/// - 空 `history`：System = role_system ∥ body（同 `assemble`）。
/// - 非空 `history`：不替换已有 System；仅当历史中无 System 时头部兜底插入 `role_system`；
///   body 非空则 append 为末尾 User。
pub fn assemble_stable(
    history: &[ModelMessage],
    role_system: &str,
    content: &str,
    user_input: &str,
    template: ModelAppendTemplate,
) -> Vec<ModelMessage> {
    let body = Self::with_user_input_for_append(content, user_input, template);
    if history.is_empty() {
        let system = join_nonempty(role_system, &body);
        return Self::replace_system(&[], &system);
    }
    let mut out = history.to_vec();
    if !out.iter().any(|m| m.role == ModelMessageRole::System) {
        out.insert(0, Self::message(ModelMessageRole::System, role_system));
    }
    if !body.is_empty() {
        out.push(Self::message(ModelMessageRole::User, &body));
    }
    out
}
```

行为表：

| 入参历史 | System | 末尾 |
|---|---|---|
| 空 | `role_system ∥ body` | 无独立 User（fold） |
| 非空、含 System | **保持原样**（不替换） | body 非空则 append User |
| 非空、无 System（legacy 导入） | 头部插入 `role_system` | body 非空则 append User |
| body 为空 | — | 无 append |

### assistant_mode.rs run_core 装配替换

```rust
let role_system = ctx.system_prompt.clone().unwrap_or_default();
// 神经元相对上轮变化才在末尾追加 content；首轮（None）不拼，角色已在 System 定格。
let content = match (&ctx.last_selected_neuron_id, &ctx.selected_neuron) {
    (Some(prev), Some(neuron)) if prev != &neuron.id => neuron.content.clone(),
    _ => String::new(),
};
let messages = ModelCallInput::assemble_stable(
    &ctx.messages,
    &role_system,
    &content,
    &user_input,
    ModelAppendTemplate::Neuron,
);
```

## Open Questions

- [ ] 首轮 System 定格后，后续轮次选中神经元若长期不变，是否需要在末尾周期性"重申"角色防遗忘？——本轮不做，观察实际效果后定。
- [ ] 主对话历史中 System 若因 compaction 被重写，`assemble_stable` 兜底逻辑是否覆盖？——compaction 只动 storage，装配层每次基于 `ctx.messages` 快照，语义自洽。

## Restated Understanding

- 我理解当前任务是：主对话 `run_core` 提示词装配从"每轮替换 System"改为"首轮角色定格 System + 神经元变化才拼末尾"，目标是前缀稳定（缓存命中）与末尾动态角色（注意力）。
- 当前核心目标是：角色不漂移、前缀稳定、避免 token 重复累积。
- 当前边界是：仅主对话；hook / select / drafts 不动。
- 暂不处理：hook 类统一改造、每轮完整 content 追加。

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是，落盘 micro-spec 是执行前必要 spec。
- 若否：N/A。

## Checkpoint Summary

- 当前任务理解：主对话提示词装配策略改造（`assemble_stable` + `run_core` 传参）。
- 当前核心目标：前缀稳定 + 角色不漂移 + token 不重复累积。
- 当前进度：✅ 已实现并验证（`cargo test --lib` 全绿、`cargo check` 0 error）。
- 下一步 1：App 内多轮对话观察 System 前缀稳定性与末尾追加行为。
- 下一步 2：依据观察结果决定是否做"末尾周期性重申角色"（Open Questions 首项）。
- 验证方式：`cargo test --lib`（118 通过）+ `cargo check` 0 error；App 内多轮对话观察 System 前缀与末尾追加。
- Execution Approval: ✅ 已批准并执行完成。

## Change Log

- 2026-08-06: 初始 micro-spec。决策：首轮角色定格 System；神经元变化才拼末尾；仅主对话；非首轮无 System 时兜底插入；补 3 个单测。
- 2026-08-06: 执行完成。实现 `assemble_stable` + `run_core` 装配替换 + 3 单测；`cargo test --lib` 118 通过，`cargo check` 0 error。

## Validation

- Self-check：✅ 已实现 `assemble_stable`（[model_call_input.rs](../..//packages/agent-app/src-tauri/src/core/model_call_input.rs)）与 `run_core` 装配替换；`assemble`、`call_system_prompt_json`、`try_llm_select`、`generate_drafts` 未改动；首轮 System 定格、非首轮不替换、仅换神经元时末尾追加 content，与行为表一致。
- Static checks：`cargo check` 0 error（仅 3 个既有 warning：assistant_mode.rs AtomicUsize、compactor.rs ConversationMode、agent-app-cli.rs mut，均非本次改动引入）。
- Runtime / Test：`cargo test --lib` 118 通过 / 0 失败，含新增 3 用例——`assemble_stable_folds_body_into_system_on_empty_history`（空历史 fold）、`assemble_stable_keeps_existing_system_untouched`（非空 System 定格）、`assemble_stable_inserts_system_when_missing`（非空无 System 兜底插入）。
- Human confirmation：代码执行已批准；结果待用户在 App 内多轮对话观察确认。
- 结果汇总：✅ 已完成代码与单测，测试与编译全绿。
- 核心目标是否已由证据证明完成：✅ `cargo test` 全绿 + `cargo check` 0 error 证明装配行为符合 Done Contract。
- 若未完成，当前剩余差距：N/A。
- 剩余风险：末尾追加仅覆盖"换神经元"轮；长对话中角色在 System 定格后不再更新，若模型遗忘需靠末尾重申（本轮未做，观察后定）。

## Resume / Handoff

- 当前状态：micro-spec 已提交，待批准。
- 当前卡点：无。
- 下一步唯一动作：用户批准后实现代码与单测。
- 下一轮核心目标：主对话前缀稳定、角色不漂移。
