# 模型调用入口

> 所有对外发起 LLM 请求的入口（call sites）清单与调用链说明。
> 配套审计：对话审计报告（2026-08-12）。输入组装规范见
> `docs/specs/2026-08-02_model-call-input.md` 与
> `docs/specs/2026-08-02_19-29_model-call-input-call-sites.md`。

## 1. 总览

pulsar-app（Rust / Tauri）的模型调用采用**单一下游出口** + 多层入口的设计：
所有请求最终汇聚到 [`ProviderRegistry::call_model`](../../packages/pulsar-app/src-tauri/src/core/providers.rs) 出网。

---

## 2. 唯一下游出口

[`ProviderRegistry::call_model`](../../packages/pulsar-app/src-tauri/src/core/providers.rs#L140-L235)

- 所有模型请求最终都汇聚到这里出网。
- 非流式 `client.chat().create()`，返回 `ModelCallResponse`。
- 内置 provider：`openai` / `deepseek` / `ollama` / `custom`（全部 OpenAI 兼容协议）。
- 本地校验：请求非空、消息非空、provider/model 存在性。
- API key 解析：环境变量 → `.pulsar/config.json`。

---

## 3. 调用入口清单

### 3.1 Tauri 命令（前端可调）

| 入口 | 位置 | 说明 |
|---|---|---|
| `call_model` | [`lib.rs`](../../packages/pulsar-app/src-tauri/src/lib.rs#L181-L190) | 裸模型调用命令；当前前端未使用，仅 CLI/TUI 使用 |
| `send_chat_message` | [`lib.rs`](../../packages/pulsar-app/src-tauri/src/lib.rs#L48-L72) | 聊天主入口 → `Gateway::send_model_message` |

### 3.2 聊天主链路（Gateway 路由）

[`Gateway::send_model_message`](../../packages/pulsar-app/src-tauri/src/core/gateway.rs#L496-L579) 按会话模式路由：

- **Assistant** → `AssistantMode::converse`（课题 hooks 编排，最重路径）
- **Chat** → `call_service.execute_round`（退化形态，无规格/无工具）
- **Agent** → `gateway.agent_loop`（多轮工具循环，护栏 `AGENT_MAX_ITERATIONS = 20`）

### 3.3 业务服务层

| 入口 | 位置 | 说明 |
|---|---|---|
| `NeuronCallService::call_system_prompt` | [`call_service.rs`](../../packages/pulsar-app/src-tauri/src/core/call_service.rs#L348-L420) | 统一系统提示词入口；被 3 个 assistant hook 复用 |
| `NeuronCallService::execute_round` | [`call_service.rs`](../../packages/pulsar-app/src-tauri/src/core/call_service.rs#L424-L607) | 会话执行轮：模型调用 + 单次工具执行 + 消息落库 |
| `Compactor::call_summary_llm` | [`compactor.rs`](../../packages/pulsar-app/src-tauri/src/core/compactor.rs#L193-L217) | 长会话压缩摘要 |

### 3.4 神经元管理（NeuronManager）

走 `NeuronModelCaller` trait（接口定义于 [`neuron_model.rs`](../../packages/pulsar-app/src-tauri/src/core/neuron_model.rs#L11-L14)；
生产实现为 [`DefaultNeuronModelCaller`](../../packages/pulsar-app/src-tauri/src/core/neuron_model.rs#L40-L61)，
内部经 `default_model_selection()` 取模型后调 `ProviderRegistry::call_model`；
测试注入 `MockModelCaller`）。三个内部调用点：

| 调用点 | 位置 | 说明 |
|---|---|---|
| `try_llm_select` | [`neuron_manager.rs`](../../packages/pulsar-app/src-tauri/src/core/neuron_manager.rs#L1119-L1192) | 候选池选型（neuron.select_one 契约） |
| `generate_drafts` | [`neuron_manager.rs`](../../packages/pulsar-app/src-tauri/src/core/neuron_manager.rs#L1207-L1256) | 草稿生成（bootstrap / 池扩充） |
| `rewrite_variant` | [`neuron_manager.rs`](../../packages/pulsar-app/src-tauri/src/core/neuron_manager.rs#L1058-L1107) | creator 变体自我迭代（variant_evolve 契约） |

### 3.5 轮询推进（Poller）

- `AssistantMode::process_step_request` → `step_poller`：每课题独立会话，Semaphore 限并发。
- Poller tick 通过 `default_model_selection()` 取模型。

### 3.6 CLI / TUI

| 入口 | 位置 | 说明 |
|---|---|---|
| `call-model` 命令 | [`pulsar-cli.rs`](../../packages/pulsar-app/src-tauri/src/bin/pulsar-cli.rs#L52-L71) | 直接调 `Gateway::call_model` |
| `Command::Call` | [`tui/app.rs`](../../packages/pulsar-app/src-tauri/src/tui/app.rs#L540-L567) | TUI 手动调用 |

---

## 4. 输入组装（统一规范）

所有入口经 [`ModelCallInput::assemble`](../../packages/pulsar-app/src-tauri/src/core/model_call_input.rs#L114-L131) 组装消息：

- **Neuron 模板**：角色/能力载体（谁、怎么做）。
- **Manual 模板**：操作说明书/输出契约（产出什么、什么格式）。
- `sanitize_tool_pairs`：自愈孤儿 tool_calls（OpenAI 兼容接口强校验）。
- insert 契约注入：`inserts/*.md`（match_topic、select_one、draft_from_model、variant_evolve、complete_scope、score_feedback、execute_command）。

---

## 5. 单次用户输入的模型调用次数（Assistant 模式）

```
score_feedback (0~1) → match_topic (1) → select_role (0~1) → execute_round (1) → complete_scope (1)
```

最多 **4~5 次串行 LLM 调用**，成本与延迟叠加。

---

## 6. 已知风险（摘要）

详见审计报告；此处仅列入口相关：

1. LLM 调用无超时（唯一出网点 `.await` 无 `tokio::time::timeout`）。
2. 无重试机制。
3. 非流式，长响应整块等待。
4. 采样参数硬编码（`ModelCallRequest` 无 temperature / max_tokens / top_p）。
5. `execute_round` 仅执行第一个 tool_call，其余静默丢弃。
6. `call_model` Tauri 命令绕过会话/授权层（当前前端未暴露使用）。
