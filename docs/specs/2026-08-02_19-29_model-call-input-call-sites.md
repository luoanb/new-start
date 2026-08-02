# Spec: ModelCallInput 调用点接入与装配策略

## Goal

- 要解决什么问题：上期已交付消息列表手术工具，但现网各调用点仍手拼 `Vec<ModelMessage>`；Hook 不带会话历史，与「同源上下文」目标不一致；各场景 user 侧文案缺少统一模板入口。
- 验收结果：所有模型调用的 **messages 装配** 以上游 `ModelCallInput` 完成；首轮/续轮装配规则统一；Hook 可读会话历史但不落库；场景模板预定义、调用方点名选用。

## Done Contract

- 什么算完成：
  1. 本 spec 契约无歧义并获批；
  2. `with_user_input_for_append` 签名演进为三参（保留 `content`/`user_input`，新增内置模板选择），并具备统一 `assemble` 入口；
  3. 下表「必改调用点」全部改为经 `ModelCallInput` 产出 `messages` 再 `call_model`；
  4. `assistant-prompt-synthesis.md` 反写与实现对齐（尤其 Hook 带历史）。
- 由什么证明：相关单测 + `rg`/代码审阅确认无旁路手拼 messages（测试替身除外）；合成报告与本 spec 一致。
- 仍算未完成：自行查 insert、完整 `ModelCallRequest` 字段托管（provider/model/tools 仍可由调用方填）；不强制消灭 `NeuronModelCaller` trait 本身，但其内部必须走 `ModelCallInput`。

## Scope

### In

- 扩展 `ModelCallInput`：内置 append 模板枚举；`with_user_input_for_append` 内部按模板拼装；`assemble`（或等价命名）统一消息列表装配。
- 改写全部业务模型调用点的 messages 构建（见调用点清单）。
- Hook：装配时传入 `ctx.messages`（或等价会话 `ModelMessage` 历史）；**不**为 Hook 的合成 User/System 轮次调用 `add_message`。
- 主对话 `run_core`：仍按现逻辑 `add_message`（用户输入 / 助手输出 / tool）；仅 messages 装配改走工具。
- 反写：`docs/agent-app/assistant-prompt-synthesis.md` §7 及调度表。

### Out

- Insert 正文改写 / 新 insert 文件（输出契约仍由现有 insert 负责）。
- `ModelCallInput` 托管 `provider_id` / `model_id` / `tools`（可另开）。
- 前端 / TUI 展示层（除非其直接拼 messages）。

## Restated Understanding

- 策略四条落地为：
  1. **唯一上游**：业务侧禁止手搓最终 `messages`；一律 `ModelCallInput` → `ModelCallRequest.messages`。
  2. **首轮 vs 续轮**：`history` 为空时，本轮要送达模型的「模板+用户侧正文」**并入 System**；非空时 **一律 append 到末尾**（User）。
  3. **Hook 带历史、不落库**：Hook 与主对话共享只读会话历史做装配；Hook 调用本身不 `add_message`。
  4. **`with_user_input_for_append` 内置模板**：保留原参数 `content` / `user_input`，**新增**模板选择参数；当前仅两种——**神经元** / **操作说明书**；骨架在函数内部，调用方不先 `render` 再塞进 `content`。场景差异（打分 / 选型 / 主对话等）靠传入的 `content`·`user_input`·`role_system` 区分，**不**再为每个 hook 拆模板枚举。
- 当前核心目标：固化装配契约与调用点改造清单，供批准后实现。
- 相对现状的**产品行为变更**：Hook 从「不读 msgs」改为「读 msgs」——需同步合成报告与相关 insert 期待（insert 仍管输出形状）。

## Facts / Constraints

- 上期工具：`replace_system` / `append` / `insert_at` / 两参版 `with_user_input_for_append`（已测）；本期将后者演进为三参。
- 现网路径索引见 [`assistant-prompt-synthesis.md`](../agent-app/assistant-prompt-synthesis.md)。
- 角色 System（neuron.content ± insert）仍由**调用方**先算好再传入装配；`ModelCallInput` 不查 `InsertCatalog`。
- `add_message` 与「模型入参装配」解耦：落库只发生在主对话（及 Engine/Agent 既有路径），不发生在 Hook 裁决调用。

## 装配契约（目标态）

### `with_user_input_for_append`（签名演进）

```rust
/// 将 content 与 user_input 按内置模板拼成一段字符串，供随后并入 System 或 append 成 User。
/// - `content` / `user_input`：语义与上期相同（调用方提供的两侧正文；可空）。
/// - `template`：选择内置模板；模板条文与如何嵌入两侧正文均在本函数内部，不对外暴露裸模板全文 API。
pub fn with_user_input_for_append(
    content: &str,
    user_input: &str,
    template: ModelAppendTemplate,
) -> String;
```

内部行为（契约要点）：

1. 按 `template`（仅 `Neuron` | `Manual`）渲染**结构化**骨架（见下节「设计初衷与骨架」）。
2. 将 `content` / `user_input` 填入对应分节；某侧为空时**省略该节**（无空标题）。
3. 不提供 `Passthrough`；场景差异靠两侧正文，不靠膨胀枚举。

**禁止**：调用方先拼好「模板正文」再当作 `content` 传入；模板种类只表达 **神经元 vs 操作说明书** 两种拼装口径。

### `assemble` 输入

| 参数 | 含义 |
|------|------|
| `history` | 会话 `ModelMessage` 列表；可空；**不修改** |
| `role_system` | 调用方已准备好的角色 System（如 `neuron.content`，或 `system_with_insert(...)`） |
| `content` | 传给 `with_user_input_for_append` 的 content 侧（业务附文；可空） |
| `user_input` | 传给 `with_user_input_for_append` 的 user 侧（用户原文 / JSON payload / nudge 等） |
| `template` | 内置模板选择 |

### 算法

```text
body = with_user_input_for_append(content, user_input, template)

if history.is_empty():
  # 首轮：body 并入 System；不单独产 User
  system = join_nonempty(role_system, body)   # 两边都非空才插 "\n\n"
  messages = replace_system([], system)
else:
  # 续轮：System 只承载角色；body 追加为末尾 User
  messages = replace_system(history, role_system)
  messages = append(messages, User { content: body })
```

### 不变量

- 不可变：不改调用方 `history`。
- Hook / 选型 / 草稿：可传同一份 `ctx.messages`，但**不得**因该次模型调用 `add_message`。
- `run_core`：装配用的 history **不含**本轮尚未落库的 user（与今一致：先装配再按需 `add_message` + 把本轮 user 纳入 `user_input` / body）；具体顺序见调用点表。
- 多条 System：`replace_system` 只替换第一条（沿用上期）。

## 预定义模板（仅两种）

```rust
pub enum ModelAppendTemplate {
    /// 神经元：角色/能力载体
    Neuron,
    /// 操作说明书：工具与输出契约
    Manual,
}
```

### 设计初衷与骨架

| | 神经元 `Neuron` | 操作说明书 `Manual` |
|--|----------------|---------------------|
| 初衷 | 定义**是谁、擅长什么、怎么做**（角色/能力） | 定义**工具职责 + 对模型的标准输出**（契约） |
| 不该写什么 | 不写死某次调用的 JSON 输出格式（那是说明书的事） | 不写角色人设长文（那是神经元的事） |
| `content` | 能力/角色附文（可空；主角色常在 `role_system`） | insert 全文（工具说明书） |
| `user_input` | 本轮任务（用户话 / nudge / purpose） | 待处理输入（JSON payload 等） |

**Neuron 骨架（结构化）**

```text
【神经元】角色与能力载体：按身份边界完成本轮任务；勿编造未提供的工具结果或事实。

## 角色与能力          ← content 非空时
{content}

## 本轮输入            ← user_input 非空时
{user_input}
```

**Manual 骨架（结构化）**

```text
【操作说明书】输出契约优先：严格按说明书规定的结构作答；待处理输入只提供事实与上下文，不得用散文替代规定格式。

## 操作说明书（工具与输出契约）  ← content 非空时
{content}

## 待处理输入                      ← user_input 非空时
{user_input}
```

说明：

- 场景（打分/选型/主对话）不拆枚举；靠两侧正文区分。
- `role_system` 与 `template` 正交：主对话常见 `role_system=neuron.content` + `Neuron` 且 `content=""`（角色在 System，body 只带「本轮输入」）。
- 骨架住在 `ModelCallInput` 内部；改文案不改枚举。

## 调用点清单（必改）

| # | 位置 | 今日拼法 | 目标装配 | 落库 |
|---|------|----------|----------|------|
| 1 | `assistant_mode::run_core` | 手推 System + extend msgs + User | `assemble(..., template=Neuron)`（user / nudge 进 `user_input`） | 是（保持现语义） |
| 2 | `assistant_mode::call_system_prompt_json` | System+User 两段，无历史 | `assemble(history=msgs, …, template=Manual)` 为主（说明书进 `content` 或由调用方决定两侧）；签名增 history + template | **否** |
| 3 | Score / Match / Complete hooks | 仅 JSON payload | 传 `ctx.messages`；模板多为 `Manual` | 否 |
| 4 | `neuron_manager::try_llm_select` | `NeuronModelCaller(system,user)` | 经 `ModelCallInput`；history 上层传入；模板 `Neuron` 或 `Manual`（选型 insert 在 `role_system` 时 body 常用 `Manual`） | 否 |
| 5 | `neuron_manager::generate_drafts` | 同上 | 同上；草稿契约偏 `Manual`（draft insert）或 `Neuron`（以 creator content 为 content 时）——实现时按「append 主体是谁」二选一，写入调用点注释 | 否 |
| 6 | `neuron_model::DefaultNeuronModelCaller` | 内部手拼两段 | 内部走 `assemble` / 三参 append；过渡期可选 `Neuron` | 否 |
| 7 | `engine` chat/agent | 手推 context + user | append 走工具；模板按主体选 `Neuron`/`Manual` | 是 |
| 8 | `compactor::call_summary_llm` | 单条 System | `assemble`；无说明书时可用 `Neuron` 薄骨架或等价朴素拼 | 否 |

CLI / TUI 若手拼 `ModelCallRequest.messages`：同期改为 `ModelCallInput`（烟测级，列入同一 Done）。

## 与上期工具的关系

- `replace_system` / `append` / `insert_at` 保持。
- `with_user_input_for_append`：**破坏性签名演进**——第三参 `template: ModelAppendTemplate`（仅 `Neuron` | `Manual`）。上期 spec / 单测需同期修订。
- 本期另增 `assemble`（推荐），内部调用三参 `with_user_input_for_append`，避免各调用点重复「空历史并入 System / 非空 append」分支。
- **不**新增对外 `render_template` 业务 API；**不**按业务场景膨胀模板枚举。
- `insert_at` 本期调用点可不强制使用；保留能力。
- 不查 insert；`role_system` 仍由调用方 `InsertCatalog::system_with_insert` 等准备。

## 反写义务

实现后必须更新：

1. [`assistant-prompt-synthesis.md`](../agent-app/assistant-prompt-synthesis.md)：Hook「不消费 msgs」→「消费只读 msgs，不 add_message」；总览表与 §7。
2. 本文件 Validation / Change Log。
3. 上期 [`2026-08-02_model-call-input.md`](./2026-08-02_model-call-input.md) Resume：指向本迭代已承接。

## 风险

| 风险 | 缓解 |
|------|------|
| Hook 带全文历史 → token / 费用上升 | 先全量接入对齐产品；若过大再开「窗口裁剪」迭代（本期不做） |
| 首轮无 User、仅 System | 部分供应商对「无 User」敏感；单测 + 实网冒烟；若失败则修订为「首轮仍 append User，但 system 合并模板」并改本 spec |
| `NeuronModelCaller` 双字符串 API 与 history 传入冲突 | 扩展 trait 或改为传 `messages`；助手路径优先改 |
| 模板与 insert 职责重叠 | `Manual` 只包输入侧拼装口径；**输出契约正文**仍来自 insert 文件，由调用方放入 `content` 或 `role_system` |
| 调用点误用 Neuron/Manual | 约定：append 主体是神经元文案 → `Neuron`；主体是操作说明书 → `Manual`；`role_system` 仍可同时含两者 |

## 待确认（阻塞实现前拍板）

1. **首轮无 User**：确认采用上文算法（空 history → body 并入 System，不产 User）。若供应商不适配，备选：空 history 也 `append User(body)`，仅把「角色」放 System。
2. **Hook history 范围**：默认 = 完整 `ctx.messages`（已滤成 ModelMessage 的会话历史）。是否需要「介入窗口」裁剪？默认否。
3. **`try_llm_select` / `generate_drafts` 的 history**：助手路径传 `ctx.messages`；bootstrap / 管理 API 无会话时传 `[]`。是否同意？
4. **本期是否包含 Engine + Compactor**：默认 **包含**（满足「所有模型调用」）；若要拆 PR，可先 Assistant+Neuron，但 Done Contract 仍算未完。

## Checkpoint Summary

- 当前理解：调用点统一经 `ModelCallInput`；空历史并入 System、非空末尾 append；Hook 带历史不落库；`template` ∈ {`Neuron`, `Manual`}。
- 核心目标：调用点接入已落地。
- 待确认四点：按 **默认值** 执行（用户 2026-08-02「开始执行」）。
- Execution Approval: `Approved`

## Change Log

- 2026-08-02: 初版方案。承接上期工具；定义 assemble 规则、模板枚举、调用点清单与反写义务。
- 2026-08-02: 纠正模板位置——`with_user_input_for_append` 保留原两参并新增 `template`，骨架在函数内。
- 2026-08-02: `ModelAppendTemplate` 收敛为仅 `Neuron` / `Manual`。
- 2026-08-02: 实现落地——三参 append + `assemble`；改写 assistant / neuron / engine / compactor / CLI / TUI；`NeuronModelCaller` 改收 messages；反写合成报告。
- 2026-08-02: 按设计初衷加厚结构化骨架——Neuron=角色/能力+本轮输入；Manual=输出契约+待处理输入；空侧省略分节。

## Validation

- Self-check: 与方案默认项一致。
- Human confirmation: 用户批准执行。
- 证据: `cargo test --manifest-path packages/agent-app/src-tauri/Cargo.toml --lib` → **82 passed**。
- 核心目标是否已由证据证明完成: **是**（本期调用点接入）。

## Resume / Handoff

- 状态: 本期 Done。
- 下一步（可选）: `ModelCallInput` 托管 provider/model/tools；Hook 历史窗口裁剪；加厚 Neuron/Manual 骨架文案。
