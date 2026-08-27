# Spec: 内置系统提示词种子优先入库

## Goal

- 要解决什么问题：`ensure_system_neuron` 创建内建系统神经元时一律走 `generate_draft`（LLM 生成 content），提示词质量不可控（可能为空 / 跑题 / 格式漂移），直接影响 Assistant 选型与 4 个裁决 hook 的运行质量。`create_neuron` 已有内置种子 + config 覆盖范式，其余内建 system_type 却没有。
- 本次目标：内建 `system_type`（selector + 4 裁决）初始化时**优先使用项目内置提示词种子直落库**（确定性、零模型调用）；LLM 生成仅作为无内置种子的自定义 `system_type` 兜底。
- 验收结果：`cargo test` 全绿；空库首次 bootstrap / 任意 `ensure_system_neuron`（内建 type）不再触发 generate_draft；`/neuron rebootstrap` 用内置种子稳定重建；自定义 system_type 行为不变（仍 LLM 生成）。

## Done Contract

- 完成：`neuron/config.rs` 新增内置种子表（5 个内建 type）+ config 覆盖键；`ensure_system_neuron` 创建分支优先种子直落库；测试证明种子命中零模型调用。
- 由什么证明：新增/更新单元测试（fake model caller 断言 generate_draft 未被调用）；`cargo test` 全绿；`cargo check` 通过。
- 哪些情况仍算未完成：内建 type 仍走 LLM 生成；config 覆盖未生效；自定义 type 行为被改动；`assistant_select_neuron` 未覆盖。

## Scope

- In：`neuron/config.rs`（种子表 + `system_prompt_for(system_type)` 读取）、`neuron/creation.rs`（`ensure_system_neuron` 创建分支）、`neuron/manager.rs`（如常量归位）、`config.json` 文档（`neurons.bootstrap.system_prompts`）、相关测试。
- Out：不改 `select_candidates` / 候选池补齐（仍 LLM 补齐）；不改普通神经元 `create_neuron` 流程；不改前端；不改 `inserts/*.md` 契约段；不删 LLM 生成路径（自定义 type 兜底保留）。

## Facts / Constraints

- 已确认事实：
  - `ensure_system_neuron` 创建路径：`ensure_creator` → `generate_draft(system=creator.content)` → `persist_system_root` → `ensure_own_candidate_pool`（[creation.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/creation.rs#L171-L230)）。
  - 内建 system_type 常量：`assistant_select_neuron`、`assistant_match_topic`、`assistant_complete_scope`、`assistant_score_feedback`、`assistant_revise_topic`（[assistant_session.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L50-L54)）；`REBOOTSTRAP_SYSTEM_TYPES` = 后 5 个（[manager.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/manager.rs#L39-L45)）。
  - 既有内置种子范式：`DEFAULT_CREATE_NEURON_PROMPT` 常量 + config `neurons.bootstrap.create_neuron_prompt` 非空覆盖、空回落默认（[config.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/config.rs#L70-L85)）。
  - `inserts/assistant.*.md` 是 behavior.insert_id 契约段（wire 附加），非 content 本体；content 与 insert 互补不重复。
- 技术/业务约束：
  - `persist_system_root` 落库后裁决类仍需补默认 behavior（`default_behavior_for_system_type`，不动）。
  - 创建后 `ensure_own_candidate_pool` 补池仍走 LLM（本 spec 不改）。
  - config 覆盖粒度：`neurons.bootstrap.system_prompts: {<system_type>: <prompt>}`（新增可选键），缺失/空回落内置种子。
- 已知风险：内置种子是静态文案，可能随需求演进过时——`rebootstrap` 可重建 + config 可覆盖，可接受。

## Open Questions

- [x] 内置种子文案由本方案起草（高质量、与 DEFAULT_CREATE_NEURON_PROMPT 同风格）后供人工修订，是否认可？**已确认：由我先起草**。
- [x] `assistant_select_neuron` 是否纳入内置种子（建议纳入：bootstrap 必保根，直接影响每次会话选型质量）？**已确认：纳入**。
- [x] 种子文案的质量标准？**已确认优先级：①格式正确（可解析、结构完整、不跑偏、符合入库契约）②高质量（200–800 字、角色/判定准则/步骤/输出契约/硬约束齐备）**。

## Restated Understanding

- 我理解当前任务是：为内建系统提示词建立"内置种子优先、LLM 生成兜底"的初始化策略，提升运行质量与确定性。
- 当前核心目标是：`ensure_system_neuron` 创建分支优先用内置种子直落库，内建 5 个 system_type 全覆盖，自定义 type 行为不变。
- 当前边界是：只改初始化/创建路径的 content 来源；候选池补齐、普通创建、前端、契约 insert 均不动。
- 暂不处理：内置种子文案的长期维护机制；`create_neuron` 种子（已有内置范式，不动）。

## 接口契约设计

```rust
// neuron/config.rs
/// 内建系统提示词种子（content 本体；与 DEFAULT_CREATE_NEURON_PROMPT 同域）。
pub const SYSTEM_PROMPT_SEEDS: &[(&str, &str)] = &[
    ("assistant_select_neuron", "..."),
    ("assistant_match_topic", "..."),
    ("assistant_complete_scope", "..."),
    ("assistant_score_feedback", "..."),
    ("assistant_revise_topic", "..."),
];

impl NeuronConfigReader {
    /// 内置种子 + config 覆盖（`neurons.bootstrap.system_prompts.<type>` 非空覆盖）。
    /// 有内置种子 → Some；无内置种子（自定义 type）→ None。
    pub fn system_prompt_for(&self, system_type: &str) -> AppResult<Option<String>>;
}

// neuron/creation.rs（ensure_system_neuron 创建分支伪代码）
if let Some(seed) = self.config.system_prompt_for(system_type)? {
    // 种子直落库：不调 generate_draft
    let created = self.selection.persist_system_root(NeuronCreate {
        desc: system_type.into(), content: seed, weight: 0.0,
        system_type: Some(system_type.into()), tool_ids: vec![], ...
    })?;
} else {
    // 自定义 type：现状 LLM 生成路径保持不变
    let draft = self.selection.generate_draft(&creator.content, &user_prompt).await?;
    ...
}
// 裁决类仍补默认 behavior；最后 ensure_own_candidate_pool 不变
```

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是（先出方案，未进入实现）。
- 若否，偏差在哪里：无。
- 是否需要调整本轮目标或范围：待用户确认 Open Questions。

## Checkpoint Summary

- 当前任务理解：内建系统提示词初始化改为内置种子优先直落库，LLM 兜底。
- 当前核心目标：`ensure_system_neuron` 内建 type 零模型调用；自定义 type 行为不变。
- 当前进度：现状分析完成，方案已落盘；未进入代码实现。
- 下一步 1: 确认 Open Questions（种子文案来源 / selector 是否纳入）。
- 下一步 2: 起草 5 个内置种子文案（content 本体，200–800 字，与 creator 种子同风格）。
- 下一步 3: 实现 `config.rs` 种子表 + `system_prompt_for` → `creation.rs` 创建分支 → 测试。
- 涉及文件 / 模块：`src/core/neuron/config.rs`、`src/core/neuron/creation.rs`、（常量归位看情况）`manager.rs`、测试。
- 风险：静态文案过时（rebootstrap + config 覆盖兜底）；config 键命名需与现有 `create_neuron_prompt` 风格一致。
- 验证方式：`cargo test` 全绿；fake model caller 断言种子命中时不调用 generate_draft。
- Execution Approval: `Approved`（用户 2026-08-27 明确"开始执行"）

## Change Log

- 2026-08-27: 初始 micro-spec。现状分析：内建 5 个 system_type 的 content 全部依赖 LLM 生成；`create_neuron` 已有内置种子 + config 覆盖范式可同构扩展。方案：新增内置种子表 + config 覆盖键，`ensure_system_neuron` 创建分支优先种子直落库，LLM 仅兜底自定义 type。
- 2026-08-27: 实现完成。`config.rs` 新增 `SYSTEM_PROMPT_SEEDS`（5 条种子）+ `system_prompt_for()`（config `neurons.bootstrap.system_prompts.<type>` 非空覆盖 > 内置种子）；`NeuronCreation` 注入 `NeuronConfigReader`；`ensure_system_neuron` 创建分支：有种子 → `persist_system_root` 直落库（desc=system_type、tool_ids=[]），无种子 → 原 LLM 生成路径。裁决类 behavior 补写与 `ensure_own_candidate_pool` 补池不变。新增 4 个测试。文档同步：`neuron-init.md`、`storage.md`。
- 2026-08-27: 追加优化 `DEFAULT_CREATE_NEURON_PROMPT`（生成器种子，用户批准）：统一中文、新增职责边界约束（不生成 `assistant_*` / `create_neuron` 系统级节点）、新增安全约束（content 将作为系统文本执行，禁提示注入/越权）、强化质量标准。行为不变，仅影响未配置 `create_neuron_prompt` 的用户。`cargo test --lib` 411 全绿。

## Validation

- Self-check: 种子分支与 LLM 分支语义分离；config 覆盖空值回落种子；裁决类 behavior 补写路径未触碰。
- Static checks: `cargo check -p pulsar-app` 通过（仅既有 providers.rs 2 个 unused import 警告）。
- Runtime / Test: `cargo test -p pulsar-app --lib` 全绿（411 passed，含新增 4 测试：种子命中/behavior 保留/config 覆盖/自定义 type 回落 LLM）。
- Human confirmation: 方案与决策已确认；种子文案由本实现起草，建议人工审校后可再调。
- 结果汇总：实现与验证完成。
- 核心目标是否已由证据证明完成：是——内建 system_type 初始化 content 零模型调用（种子直落库），自定义 type 行为不变，测试证明。
- 若未完成，当前剩余差距：无。
- 剩余风险：内置种子文案为静态文本，需长期维护（rebootstrap 重建 + config 覆盖兜底）。

## Resume / Handoff

- 当前状态：实现完成，测试全绿，文档已回写。
- 当前卡点：无。
- 下一步唯一动作：可选——人工审校 5 条种子文案质量（`config.rs` `SYSTEM_PROMPT_SEEDS`）。
- 下一轮核心目标：种子文案定稿（如需修订只需改常量并重跑测试）。
