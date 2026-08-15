# Spec: 移除内建会话规格 + 术语统一（assistant_dialogue / 规格）

## Goal

- 要解决什么问题：
  1. `session.assistant_dialogue` 是 2026-08-09 旧设计的「内建会话规格」，设计意图是作为 `Global` 种子（无规格的助手主对话）的默认 behavior 载体；但代码中 `Global` 由 `resolve_role` 内联构造 behavior（`DEFAULT_ASSISTANT_GLOBAL_LIMIT`），**从不读取** `session.assistant_dialogue`。它只在 bootstrap 被注册、被 `list_session_specs` 列出——僵尸占位，文档与代码偏差。
  2. 「规格」术语沿用旧命名（规格 = 带 behavior 的 `session.%` 系统神经元），2026-08-12 重构已把发起语义泛化为 `SessionSeed::Neuron(id)`（任意普通/系统神经元），术语残留导致误解。
- 验收结果：`session.assistant_dialogue` 全部移除（代码无残留）；代码注释 / 前端注释 / 活跃文档中不再出现「规格神经元 / 会话规格」描述词；`session.%` 系统神经元在描述中统称「系统神经元」，`spec_neuron_id` 锚定的神经元在描述中统称「发起神经元」；`cargo test --lib` / `cargo check` 通过。

## Done Contract

- 完成定义：
  1. **移除内建规格**（代码）：
     - `manager.rs`：删 `SESSION_ASSISTANT_DIALOGUE` 常量、`default_assistant_dialogue_behavior` 函数、相关 import。
     - `creation.rs`：删 bootstrap 中 `ensure_session_neuron(SESSION_ASSISTANT_DIALOGUE, ...)` 懒注册块与相关日志、import。
     - `config.rs` / `neuron/config.rs`：删 `SessionDefaultsSection`（含 `assistant_dialogue` 字段与 `fallback_assistant_dialogue`）、`NeuronSection.session_defaults` 字段、`session_defaults()` 读取方法、相关 import 与注释。
  2. **术语统一**（描述性文字，代码标识符不改）：
     - 全仓注释 / 前端 ts 注释 / 活跃文档中所有「规格 / 规格神经元 / 会话规格 / 规格管理 / 规格化 / 规格会话」按语义替换：指向 `session.%` 系统神经元 → 「系统神经元」；指向 `spec_neuron_id` 锚点 → 「发起神经元」。
     - **不重命名**代码标识符：`spec_neuron_id`、`SessionSpecManager`、`list_session_specs`、`SessionBehavior`、`SystemPromptStatus`、`ensure_session_neuron`、`session.*` system_type 前缀等。
  3. **文档反写**：本次变更记入 `neuron-call-service-refactor.md` 与本 spec 的 Change Log；历史 spec 快照（2026-08-09 的 call-service / phase2 文档）不全文改写（保留历史决策可追溯），仅以头部警示块/Change Log 标注「内建会话规格已移除」。
- 由什么证明：`cargo test --lib` / `cargo check` 通过；Grep 验证（排除 `spec` 标识符与历史 spec 快照）无「规格神经元 / 会话规格」残留；`session.assistant_dialogue` 无代码残留。
- 哪些情况仍算未完成：`Global` 种子的运行时行为改为读取某个内建神经元（不改，`resolve_role` 硬编码 `DEFAULT_ASSISTANT_GLOBAL_LIMIT` 即为最终行为）；`SessionSpecManager` / `list_session_specs` 机制整体移除（不改，仍管理用户自建 `session.%` 系统神经元）；`spec` 代码标识符重命名（不做）。

## 背景与根因

- 旧设计（[neuron-call-service.md](file:///home/lab/Documents/trae_projects/new-start/docs/specs/2026-08-09_16-10_neuron-call-service.md) L68）：「助手主对话的本质是『无固定业务神经元的动态选型会话』→ 映射为内建规格 `session.assistant_dialogue`」。
- 现实代码（[call_service.rs resolve_role Global 分支](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src-tauri/src/core/call_service.rs#L354-L377)）：内联构造 `SessionBehavior { Global{ DEFAULT_ASSISTANT_GLOBAL_LIMIT }, ToolPolicy::None, None }`，不读 `session.assistant_dialogue`。全仓 `SESSION_ASSISTANT_DIALOGUE` 仅出现在常量定义、bootstrap 注册与日志——无运行时消费点。
- 术语演进：2026-08-09「规格 = 带 behavior 的 session.% 系统神经元」→ 2026-08-12「种子 = 任意神经元（普通推导默认领域 / 系统用 behavior）」。旧术语残留造成「规格神经元」「发起神经元」混用。
- 用户决策（2026-08-15）：①移除 `session.assistant_dialogue`；②`session.%` 系统神经元就叫「系统神经元」；③`spec_neuron_id` 锚定的就叫「发起神经元」；④任何地方不出现「规格神经元」一词。

## 改动点

| 文件 | 改动 |
|---|---|
| `src-tauri/src/core/neuron/manager.rs` | 删 `SESSION_ASSISTANT_DIALOGUE`（L50）、`default_assistant_dialogue_behavior`（L68-76）、`SessionDefaultsSection` import；`specs` 字段注释改「系统神经元 behavior 管理」 |
| `src-tauri/src/core/neuron/creation.rs` | 删 bootstrap 懒注册块（L310-338）、import（L26）、相关日志 |
| `src-tauri/src/core/config.rs` | 删 `SessionDefaultsSection`（L72-90）、`NeuronSection.session_defaults`（L66-68）与注释 |
| `src-tauri/src/core/neuron/config.rs` | 删 `session_defaults()`（L104）与 import、注释 |
| `src-tauri/src/core/models.rs` / `call_service.rs` / `gateway.rs` / `spec.rs` / `store.rs` 等 | 术语注释替换（详见 grep 清单） |
| `src/lib/types.ts` / `dataStore.svelte.ts` / `layoutStorage.ts` / `systemTypeColor.ts` | 前端注释术语替换 |
| `docs/` 活跃文档 | 术语替换 + Change Log 反写 |

## 兼容性

- **运行时行为不变**：`assistant_dialogue` 从未被消费，删除不影响任何选型/会话路径；`Global` 种子行为保持（硬编码 `DEFAULT_ASSISTANT_GLOBAL_LIMIT`）。
- **config 兼容**：`neuron.session_defaults.assistant_dialogue` 配置读取路径删除（该配置从未生效，无行为迁移）。
- **管理面**：bootstrap 后 `list_session_specs` 不再返回内建条目（无内建 `session.%` 生成）；前端未调用 `list_session_specs`（仅类型定义残留），UI 无影响。
- **标识符**：`spec_neuron_id`、`SessionSpecManager`、`session.*` 等代码标识符不重命名，API 协议不变。
- **历史文档**：2026-08-09 的 call-service / phase2 历史 spec 不全文改写，仅反写标注。

## Validation

- `cargo check`：0 error。
- `cargo test --lib`：全绿；移除仅影响 bootstrap 注册相关断言（如有）需同步调整。
- Grep 验证：
  - `rg "assistant_dialogue"` → 仅历史 spec 文档反写标注（代码 0 处）。
  - `rg "规格神经元|会话规格"`（排除 docs/specs/2026-08-09_* 历史快照）→ 0 处。
- 手动验证：App 启动 bootstrap 正常（无 `session spec ensured` 日志、无报错）；`list_session_specs` 返回空列表不崩溃。

## Change Log

- 2026-08-15：初始 micro-spec。决策：移除 `session.assistant_dialogue` 内建规格（僵尸占位，Global 不消费）；术语统一——`session.%` 系统神经元称「系统神经元」、`spec_neuron_id` 锚点称「发起神经元」、全仓（代码注释/前端注释/活跃文档）清除「规格」描述词；代码标识符不重命名；历史 spec 快照不全文改写。
