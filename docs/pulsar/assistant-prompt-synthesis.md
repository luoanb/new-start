# Assistant 模式：模型调度与提示词合成报告

> 状态：实现态快照（代码为准）  
> 日期：2026-08-03
> 范围：`packages/pulsar-app` 助手一轮流程中会触发的全部模型调用，及其 system / user 提示词如何拼装  
> 相关：[`architecture.md`](./architecture.md) · [`docs/specs/2026-08-01_20-46_self-describing-inserts.md`](../specs/2026-08-01_20-46_self-describing-inserts.md) · [`docs/specs/2026-08-02_model-call-input.md`](../specs/2026-08-02_model-call-input.md) · [`docs/specs/2026-08-02_19-29_model-call-input-call-sites.md`](../specs/2026-08-02_19-29_model-call-input-call-sites.md) · `docs/sdd-lab/2026-07-26_21-30_assistant-mode/`

---

## 1. 总览

助手一轮统一为：**beforehook → run_core → afterhook**。全部模型调用的 **messages** 经 `ModelCallInput::assemble`（或等价 `append`）产出，再填入 `ModelCallRequest`。

| 类型 | 入口 | 是否拼会话历史 `msgs` | 落库（add_message） | `role_system` | append 模板 |
|------|------|----------------------|---------------------|---------------|-------------|
| A. Hook / 裁决 | `call_system_prompt_json` / `try_llm_select` / `generate_drafts` | **是**（只读） | **否** | 系统/选型 neuron.content | `Manual`（说明书进 `content`） |
| B. 主对话核心 | `run_core` | **是** | **是**（user/assistant/tool） | 选中业务 neuron.content | `Neuron` |

装配规则（`ModelCallInput::assemble`）：

```text
body = with_user_input_for_append(content, user_input, template)  // template ∈ {Neuron, Manual}

if history.is_empty():
  system = join_nonempty(role_system, body)   # body 并入 System，不产 User
  messages = replace_system([], system)
else:
  messages = replace_system(history, role_system)
  messages = append(messages, User(body))     # body 非空时
```

实现：`packages/pulsar-app/src-tauri/src/core/model_call_input.rs`。

Insert 文件目录：`packages/pulsar-app/src-tauri/inserts/<id>.md`（`rust-embed` 编译进二进制）。调用方用 `InsertCatalog::require` 取说明书正文，传入 `assemble` 的 `content`（`Manual`），**不再**用 `system_with_insert` 把 insert 焊进 `role_system`（该 helper 仍保留供其它用途）。

---

## 2. 一轮调度顺序（按入口）

### 2.1 `converse`（用户输入）

```text
1. ScoreFeedbackBeforeHook      → call_system_prompt_json (条件触发；history=ctx.messages)
2. MatchTopicBeforeHook         → call_system_prompt_json (history=ctx.messages)
3. SelectNeuronBeforeHook
      ├─ 第一步 select_assistant_candidates(scope)
      │    ├─ 首轮（无 last_selected）→ Global（默认 7，可覆盖）
      │    └─ 后续轮 → Neighborhood（默认下游 6 + self/兄弟最多 3 + 三层上游最多 3 + 全局权重 top5，可覆盖）
      └─ 第二步 select_one_from_with_history(candidates, ctx.messages) → 选型模型
4. authorize_tools
5. run_core                     → 主对话模型（可 tools；Neuron 模板）
6. CompleteScopeAfterHook       → call_system_prompt_json (history=ctx.messages)
7. mark_user_intervention       （无模型）
```

### 2.2 `step` / Poller

```text
1. SelectNeuronBeforeHook（有 last_selected 时统一按 self 邻域池）
2. run_core
3. CompleteScopeAfterHook
```

不跑 `ScoreFeedback` / `MatchTopic`。

### 2.3 侧路：系统提示词神经元尚未存在时

`call_system_prompt_json` / `SelectNeuronBeforeHook` 会 `ensure_system_neuron(system_type)`：

- 已存在：直接返回，并可能 `ensure_own_candidate_pool`（内部再走 `select_candidates` → 可能 `generate_drafts`）。
- 不存在：`generate_draft(creator.content, 写 system_type 的 user_prompt)` 落库后再填候选池（装配 history=`[]`）。

Bootstrap 启动时已保证 `create_neuron` + `assistant_select_neuron`；其余 hook 的 `system_type` 多为懒创建。

---

## 3. 共用拼装原语

### 3.1 `call_system_prompt_json(system_type, user_payload, model, history)`

位置：`assistant_mode.rs`

```text
prompt_neuron = ensure_system_neuron(system_type)
insert        = InsertCatalog::require(insert_id_for_system_type)
messages      = ModelCallInput::assemble(
                  history,                 // ctx.messages；只读
                  prompt_neuron.content,   // role_system
                  insert,                  // content = 说明书
                  user_payload.to_string(),
                  Manual,
                )
tools = None
→ providers.call_model → extract_json_object(output)
```

**不**为本次 Hook 调用 `add_message`。业务字段仍由调用方抽成 `user_payload` JSON。

| `system_type` | insert id |
|---------------|-----------|
| `assistant_score_feedback` | `assistant.score_feedback` |
| `assistant_match_topic` | `assistant.match_topic` |
| `assistant_complete_scope` | `assistant.complete_scope` |

未映射的 `system_type` 直接报错（无 insert）。

### 3.2 `try_llm_select(candidates, history)`

位置：`neuron_manager.rs`

```text
selector = get_neuron_by_system_type("assistant_select_neuron")
insert   = InsertCatalog::require("neuron.select_one")
messages = assemble(history, selector.content, insert, JSON{candidates}, Manual)
→ model_caller.call_model(messages) → {"neuron_id": "..."}
失败则 weight fallback（无第二次模型调用）
```

助手路径先由 `select_assistant_candidates(scope)` 构造候选，再经
`select_one_from_with_history(candidates, ctx.messages)` 传入历史；管理/bootstrap 的其它选型路径无会话时 `history=[]`。

### 3.3 `generate_drafts(system_prompt, user_prompt, expected, history)`

位置：`neuron_manager.rs`

```text
insert   = InsertCatalog::require("neuron.draft_from_model")
messages = assemble(history, system_prompt, insert, user_prompt, Manual)
→ model_caller.call_model(messages) → 解析草稿 JSON 列表
```

常见调用方：`fill_candidates_batch` / `ensure_system_neuron` / `create_neuron`（当前多传 `history=[]`）。

### 3.4 `run_core`（主对话）

位置：`assistant_mode.rs`

```text
# 若有本轮 user_input：先 add_message(User)，再装配
messages = assemble(
  ctx.messages,                          # 不含本轮尚未写入 history 的重复；user 进 user_input
  selected_neuron.content,               # role_system；无 insert
  "",
  user_input | poller nudge | "",
  Neuron,
)
tools = ToolRegistry.definitions_for(authorized_tool_ids) 或 None
→ providers.call_model(...); 至多执行第一个授权 tool_call，结果落盘
```

`ctx.system_prompt` 由 `SelectNeuronBeforeHook` 设为**选中普通神经元**的 `content`。

---

## 4. 各调度点明细

### 4.1 ScoreFeedbackBeforeHook（权重打分）

| 项 | 内容 |
|----|------|
| 时机 | `converse` 第一个 beforehook |
| 跳过 | 无 topic；无 `last_intervention_at`；`intervention_neuron_ids` 为空 |
| 通道 | `call_system_prompt_json` + `Manual` |
| role_system | `assistant_score_feedback` neuron.content |
| content | `assistant.score_feedback` insert 全文 |
| user_input | JSON：`{ user_input, topic_id, neuron_ids }` |
| 期望输出 | `{"score": <int -5..=5, ≠0>}` |
| 聊天历史 | **只读拼入** `ctx.messages`；不 add_message |
| 失败策略 | JSON 解析失败 → warn 后跳过打分；非法 score 范围 → 报错 |

### 4.2 MatchTopicBeforeHook（课题匹配 / 创建）

| 项 | 内容 |
|----|------|
| 时机 | `converse` 第二个 beforehook |
| 通道 | `call_system_prompt_json` + `Manual` |
| role_system / content | match_topic neuron + insert |
| user_input | JSON：`{ user_input, current_session_id, topics }` |
| 聊天历史 | 只读 `ctx.messages`；不 add_message |

### 4.3 SelectNeuronBeforeHook → 选型（及可能的补齐草稿）

| 项 | 内容 |
|----|------|
| 时机 | converse / step / poller |
| 选型源 | 仅无 `last_selected_neuron_id` 时取全局 7；否则三种入口均以 last selected 为 self |
| 第一步：候选池 | `select_assistant_candidates(scope)`；Global/Neighborhood 强类型作用域；Policy 可控制既有下游、新建下游、缺口补齐、兄弟数和上游深度 |
| 默认邻域池 | 下游 6（既有最多 4，固定新建 2，既有缺口也新建补齐）+ self/兄弟最多 3 + 父/爷/爷的父最多 3 + 全局权重 top5（去重并入池尾）；按 id 去重 |
| 多父规则 | 每层选节点 weight 最高的直接父节点；最高权重并列随机；兄弟取第一层父节点的其他直接子节点 |
| 第二步：选 1 | `select_one_from_with_history(candidates, ctx.messages)`；`Manual` + `neuron.select_one`；失败按 weight 回退 |
| 写入 ctx | `selected_neuron`；`system_prompt = selected.content` |
| 补齐草稿 | 非首轮至少生成 2 个 self 直接下游；既有下游不足 4 时一并补缺口；`history=[]` |

### 4.4 run_core（助手主对话）

| 项 | 内容 |
|----|------|
| 时机 | 全部入口的核心 |
| 装配 | `assemble(..., Neuron)` |
| Tools | 选中神经元 `tool_ids` ∩ 注册表 |
| 轮次约束 | 1 次 LLM + 至多 1 次工具 |
| 落库 | user（若有）/ assistant / tool 仍 `add_message` |

这是助手模式中**唯一**会把本轮对话写入会话存储的模型调度（Hook 只读历史）。

### 4.5 CompleteScopeAfterHook（勾选 ScopeIn）

| 项 | 内容 |
|----|------|
| 时机 | 核心之后 |
| 通道 | `call_system_prompt_json` + `Manual` |
| user_input | JSON：`{ topic_id, scope_in, model_output, tool_result, user_input }` |
| 聊天历史 | 只读 `ctx.messages`；不 add_message |

### 4.6 ensure_system_neuron（懒创建系统根，侧路）

| 项 | 内容 |
|----|------|
| 装配 | `generate_drafts` → `Manual` + draft insert；history=`[]` |
| 期望输出 | 单条草稿 `{desc, content, tool_ids}` |

---

## 5. Insert 清单与职责

| insert id | 读者 | 消费位置 | 核心输出契约 |
|-----------|------|----------|--------------|
| `assistant.score_feedback` | 打分模型 | `call_system_prompt_json` → assemble `content` | `{"score": N}`，N∈[-5,5]≠0 |
| `assistant.match_topic` | 课题裁决 | 同上 | `action`；create 时强制 `scope_in` |
| `assistant.complete_scope` | 勾选模型 | 同上 | `completed_item_ids` |
| `neuron.select_one` | 选型模型 | `try_llm_select` → assemble `content` | `neuron_id` ∈ candidates |
| `neuron.draft_from_model` | 草稿模型 | `generate_drafts` → assemble `content` | desc/content/tool_ids 列表 |

约定：

- **神经元** = 角色/能力载体（是谁、怎么做）→ 通常在 `role_system`；`Neuron` 模板给 body 加「本轮输入」分节（及可选能力附文）。
- **操作说明书** = 工具与输出契约 → 作为 `Manual` 的 `content`；body 分节为「说明书 + 待处理输入」。
- 角色人设不进 insert；输出格式契约不进 neuron 长文。

---

## 6. 「提示词合成」对照一览

```text
┌─ Hook / 裁决类（Manual）────────────────────────────────────┐
│  role_system = 系统 neuron.content                          │
│  body = with_user_input_for_append(insert, JSON/payload, Manual) │
│  history = ctx.messages（只读）；不 add_message               │
│  空历史 → body 并入 System；非空 → User append body          │
└──────────────────────────────────────────────────────────────┘

┌─ 主对话 run_core（Neuron）──────────────────────────────────┐
│  role_system = 选中业务 neuron.content                      │
│  body = with_user_input_for_append("", user/nudge, Neuron)  │
│  history = ctx.messages；本轮 user 先落库再进 user_input     │
└──────────────────────────────────────────────────────────────┘
```

| # | 调度名 | role_system | assemble content | template | user_input | msgs | 落库 |
|---|--------|-------------|------------------|----------|------------|------|------|
| 1 | score_feedback | score neuron | score insert | Manual | JSON | ✓ 只读 | ✗ |
| 2 | match_topic | match neuron | match insert | Manual | JSON | ✓ 只读 | ✗ |
| 3a | fill 候选（条件） | creator 子/自身 | draft insert | Manual | Purpose 契约 | [] | ✗ |
| 3b | select_one LLM | select neuron | select insert | Manual | JSON candidates | ✓ 只读 | ✗ |
| 4 | run_core | **业务 neuron** | `""` | Neuron | user/nudge | ✓ | ✓ |
| 5 | complete_scope | complete neuron | complete insert | Manual | JSON | ✓ 只读 | ✗ |
| ※ | ensure 系统根 | creator content | draft insert | Manual | system_type 契约 | [] | ✗ |

---

## 7. 与协议的对齐备忘

需求（assistant-mode Hook Contract）：hook 与对话**同源上下文**。

1. **轮次对象同源**：共享 `AssistantRoundContext`。  
2. **模型输入同源历史**：Hook / 选型经 `assemble(history=ctx.messages, …)` **只读拼入**会话消息；**不**为 Hook 合成轮次 `add_message`。  
3. **主对话**：`Neuron` 模板 + 选中 neuron content；负责落库。  
4. **说明书位置**：insert 作为 `Manual` 的 `content` 进入 body，不再焊进 `role_system`（空历史时 body 仍会并入 System）。

---

## 8. 代码索引

| 能力 | 路径 |
|------|------|
| 消息装配 / 模板 | `packages/pulsar-app/src-tauri/src/core/model_call_input.rs` |
| 编排 / hooks / `call_system_prompt_json` / `run_core` | `packages/pulsar-app/src-tauri/src/core/assistant_mode.rs` |
| `select_assistant_candidates` / `select_one_from_with_history` / `try_llm_select` / `generate_drafts` | `packages/pulsar-app/src-tauri/src/core/neuron_manager.rs` |
| `NeuronModelCaller`（收 messages） | `packages/pulsar-app/src-tauri/src/core/neuron_model.rs` |
| Insert 正文 / `require` | `packages/pulsar-app/src-tauri/src/core/insert_catalog.rs` · `inserts/*.md` |

---

## 9. Change Log

| 日期 | 说明 |
|------|------|
| 2026-08-02 | 初版：按当时实现梳理助手模式全部模型调度与提示词合成路径 |
| 2026-08-02 | 接入 `ModelCallInput`：Hook 只读拼历史；`Neuron`/`Manual` 模板；insert 进 assemble `content`；反写本报告 |
| 2026-08-03 | 助手选型改为首轮全局、后续统一使用 last selected 邻域池；记录 6+3+3 配额与三层上游规则 |
| 2026-08-03 | 恢复候选池构造与 LLM 选 1 两个显式阶段；新增强类型 Scope/Policy 控制配额，默认值暂不进入 config |
| 2026-08-09 | 邻域池在既有装配后并入全局权重 top5（按 id 去重），保证高分节点在任意轮次可被选中；`list_global_candidates` 口径收紧为排除系统提示词与 observing 变体 |
