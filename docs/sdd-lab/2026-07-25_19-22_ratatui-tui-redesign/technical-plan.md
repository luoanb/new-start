# Technical Plan / 技术方案: ratatui-tui-redesign

## Requirement Baseline / 需求基线

- 对应需求文档：`docs/sdd-lab/2026-07-25_19-22_ratatui-tui-redesign/requirements.md`
- 需求确认状态：用户已指定核心交互目标与 `ratatui` 技术栈；方案仍待用户审阅确认，确认前不进入执行。
- 本方案覆盖范围：`agent-app` 的 `agent-app-tui` 从轻量行式 shell 升级为基于 `ratatui` 的全屏 TUI 会话聊天工作台。

## Current Project Facts / 当前项目事实

- 已读取文件/模块：
  - `packages/agent-app/package.json` — `tui` 脚本通过 Cargo 启动 `agent-app-tui`。
  - `packages/agent-app/src-tauri/Cargo.toml` — 当前无 `ratatui` / `crossterm` / `ratatui-textarea` 依赖。
  - `packages/agent-app/src-tauri/src/bin/agent-app-tui.rs` — 当前 TUI 使用 `std::io::{stdin, stdout}` 逐行读取和打印。
  - `docs/agent-app/architecture.md` — `core` 拥有业务行为，TUI 只拥有终端渲染、按键处理和状态刷新。
  - `docs/agent-app/commands.md` — 当前 TUI 是 compact interactive shell，全屏布局被记录为 later。
  - `docs/agent-app/storage.md` — session 和 config 由 core/storage 管理，入口层不直接改文件。
  - `docs/specs/2026-07-25_18-44_tui-session-chat-v2.md` — 已规定 provider-backed session chat、可恢复错误、模型配置化。
- 当前实现事实：
  - `agent-app-tui.rs` 的主循环是 `read_line` + `handle_input` + `println!`。
  - 当前已有 `/help`、`/skills`、`/providers`、`/models`、`/use`、`/call`、`/sessions`、`/history`、`/clear`、`/status`、`/exit`。
  - 普通输入已通过 `Gateway::send_model_message` 进入 provider-backed session chat。
  - 错误已经能在 loop 内打印 hint，但展示仍是线性文本。
  - 当前没有全屏布局、输入框组件、滚动聊天区、折叠工具块、长任务 UI 状态。
- 相关接口/数据结构：
  - `Gateway::default()` 初始化 core。
  - `Gateway::status()` 提供当前 conversation、skill 数、conversation 数。
  - `Gateway::list_conversations()` / `Gateway::history()` 提供会话列表和历史。
  - `Gateway::list_providers()` / `Gateway::list_models()` / `Gateway::require_model()` 提供模型选择能力。
  - `Gateway::send_model_message()` 提供会话型模型聊天。
  - `Gateway::call_model()` 提供无会话调试调用。
- 约束与风险：
  - TUI 不能直接读写 `.agent-app/sessions` 或 `.agent-app/config.json`。
  - 当前 core 是否已有工具调用事件流不明确；若没有，TUI 可以先定义渲染模型和占位事件契约，实际工具调用接入需跟随 core 能力扩展。
  - token streaming 不在现有 TUI 二期 spec 范围内；若本轮要做 streaming，需要先调整需求。

## Open Questions / 开放问题

- [ ] Q1 第一版是否需要 token streaming？
  - 触发来源：需求拟定 / 现有 spec
  - 无法确定的内容：用户要求“长任务运行感”，但未明确要求 token-by-token 输出。
  - 影响范围：core API、TUI event loop、message rendering、取消语义。
  - 候选处理：方案 A — 第一版不做 token streaming，只做任务状态、耗时和最终回复；方案 B — 同步引入 streaming event API。
  - 用户回答/确认：待用户确认。
  - 状态：待用户确认。
- [ ] Q2 `/model` 和 `/provider` 是否替代 `/use <provider> <model>`？
  - 触发来源：需求拟定 / 现有命令契约
  - 无法确定的内容：是否需要保持现有 `/use` 用户习惯。
  - 影响范围：命令解析、帮助文案、测试。
  - 候选处理：方案 A — 保留 `/use`，新增 `/model` / `/provider`；方案 B — 用新命令替代 `/use`。
  - 用户回答/确认：待用户确认。
  - 状态：待用户确认。

## Solution Options / 方案候选

### Option A / 方案 A（推荐）

- 推荐：是
- 方案摘要：使用 `ratatui + crossterm + ratatui-textarea` 重写 TUI adapter，保留现有 `Gateway` 作为业务边界；第一版不做 token streaming，只做全屏布局、会话恢复、多行输入、结构化错误、任务状态块和最终回复展示。
- 涉及模块：
  - `packages/agent-app/src-tauri/Cargo.toml`
  - `packages/agent-app/src-tauri/src/bin/agent-app-tui.rs`
  - 可新增 `packages/agent-app/src-tauri/src/tui/` 或 `src/bin/agent_app_tui/` 子模块承载 app state、render、event、commands。
  - 可能扩展 `core` 的只读查询或错误展示辅助，但不移动业务逻辑到 TUI。
- 优点：
  - 改造边界清晰，TUI 只负责 terminal app。
  - 能快速解决当前“刷屏式 REPL”体验问题。
  - 不强行引入 streaming，降低 core API 变更面。
  - `ratatui-textarea` 直接覆盖多行输入、大段粘贴和基础编辑体验。
- 缺点：
  - 第一版助手回复仍是完整返回后展示，不是实时 token 输出。
  - 工具状态块若缺少 core 事件源，只能先覆盖 TUI 内已知任务状态，真实 tool calling 需要后续接入。
- 风险：中。主要风险是 TUI event loop 与 async model call 协调、终端恢复、布局复杂度。

### Option B / 方案 B

- 推荐：否
- 方案摘要：在引入 `ratatui` 的同时改造 core 为 streaming / event-driven API，TUI 直接消费 `RunEvent`，一次完成 token streaming、工具执行事件、取消和进度。
- 涉及模块：
  - `Cargo.toml`
  - `agent-app-tui.rs`
  - core chat API
  - provider registry
  - session persistence
- 优点：
  - 最终体验最好，天然支持实时输出、工具进度和取消。
  - 为后续 GUI/CLI 共享事件流打基础。
- 缺点：
  - 变更范围明显扩大，容易把 TUI 重构和 core runtime 重构耦合在一起。
  - 需要重新定义跨入口事件契约，测试成本高。
  - 与现有 TUI 二期 spec 中 “Out: streaming 输出 / tool calling 循环” 冲突，需要先改旧 spec。
- 风险：高。

### Option C / 方案 C

- 推荐：否
- 方案摘要：不做全屏 TUI，只用 `reedline` 或 `rustyline` 替换 `std::io::read_line`，增强历史输入、多行编辑和补全。
- 涉及模块：
  - `Cargo.toml`
  - `agent-app-tui.rs`
- 优点：
  - 实现成本最低。
  - 能快速改善输入体验。
- 缺点：
  - 无法满足上方上下文区、主聊天流、折叠工具块、长任务状态等核心需求。
  - 本质仍是 CLI REPL，不是用户要求的 TUI。
- 风险：低，但方向不满足需求。

## Decision / 方案决策

- Selected / 选定方案：（建议 Option A，等待用户确认）
- Why / 选择原因：Option A 最贴合用户指定的 `ratatui` 技术栈和交互目标，同时控制变更范围，不把 TUI 重构扩散成 core streaming/runtime 大改。
- Decision Owner / 决策人：用户
- Decision Time / 决策时间：（等待用户确认）
- Open Questions 状态：Q1、Q2 待用户确认

## API Design / API 设计

### Contract Scope / 契约范围

- 变更类型：新增 TUI 内部状态与事件契约；core API 第一版尽量不做破坏性变更。
- 消费方：`agent-app-tui` terminal app。
- 真相源文件：
  - TUI 内部契约：建议新增 `packages/agent-app/src-tauri/src/tui/` 下的模块。
  - core 真相源仍为 `agent_app_lib::core`。

### `TuiApp`

- `gateway: Gateway`：业务入口，只通过 core API 操作会话、模型、状态。
- `active_session_id: String`：当前会话 id，从 `Gateway::status()` 或会话选择结果同步。
- `active_model: Option<ChatModelSelection>`：当前 provider/model。
- `messages: Vec<TuiMessage>`：当前会话渲染用消息列表，由 core history 投影而来。
- `input: TextArea<'static>`：底部多行输入状态。
- `input_history: Vec<String>`：本次 TUI 会话内的输入历史。
- `focus: FocusPane`：当前焦点区域，例如聊天流、输入框、命令面板、会话列表。
- `tasks: Vec<TuiTaskBlock>`：运行中或已完成的模型调用/工具调用状态块。
- `error_banner: Option<TuiErrorView>`：最近错误的结构化展示。

### `TuiMessage`

- `id: String`：渲染层稳定 id，可由 session id + index 派生。
- `role: TuiMessageRole`：`User` / `Assistant` / `Tool` / `Error` / `Status`。
- `content: String`：展示内容。
- `timestamp: Option<i64>`：消息时间。
- `collapsed: bool`：长内容或工具详情是否折叠。

### `TuiTaskBlock`

- `id: String`：任务 id。
- `kind: TuiTaskKind`：`ModelCall` / `ToolCall` / `ConfigLoad` / `SessionLoad`。
- `label: String`：短标题，例如 `calling deepseek/deepseek-v4-flash`。
- `status: TuiTaskStatus`：`Running` / `Done` / `Failed` / `Cancelled`。
- `started_at: Instant`：开始时间，用于展示耗时。
- `finished_at: Option<Instant>`：结束时间。
- `summary: Option<String>`：完成或失败摘要。
- `details: Vec<String>`：展开后展示的细节。
- `expanded: bool`：是否展开。
- `cancellable: bool`：是否支持取消。

### `TuiErrorView`

- `code: String`：对应 `AppError::code()`。
- `what_happened: String`：发生了什么。
- `possible_causes: Vec<String>`：可能原因。
- `next_actions: Vec<String>`：下一步可选操作。
- `raw_summary: Option<String>`：保留 provider 原始错误摘要，但不直接 dump stack trace。

### Compatibility Notes / 兼容说明

- 现有 `/use <provider> <model>` 不应直接删除，建议保留兼容，并在帮助中引导 `/model` / `/provider`。
- 现有 `Gateway` 能力足够支撑 session list、history、status、provider/model list、send message 的第一版全屏 TUI。
- 若后续加入 token streaming 或真实 tool calling event，需要先扩展 core 事件契约，再让 TUI 消费，不应让 TUI 自己模拟业务事件。

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：
  - 用户确认本技术方案。
  - 用户回答或接受 Q1、Q2 的推荐处理。
  - 用户明确批准进入执行阶段。
- 若执行前需求、API、范围或交互规则变化：
  - 先更新 `requirements.md` 与 `technical-plan.md`。
  - 再重新请求用户批准执行。

### Step 1. 引入 Ratatui 依赖与模块骨架

#### 文件：`packages/agent-app/src-tauri/Cargo.toml`

- 改动类型：修改
- 改动内容：
  - 新增 `ratatui`。
  - 新增 `crossterm`，或使用 `ratatui` 默认 crossterm feature 并按需要显式依赖事件 API。
  - 新增 `ratatui-textarea`。
- 设计约束：
  - API：不影响 Tauri GUI / CLI 入口。
  - 状态：不改变 storage 格式。
  - 交互：只为 TUI binary 增强终端 UI。
- 验收点：
  - `cargo check --manifest-path packages/agent-app/src-tauri/Cargo.toml --bin agent-app-tui` 通过。

#### 文件：`packages/agent-app/src-tauri/src/bin/agent-app-tui.rs`

- 改动类型：修改
- 改动内容：
  - 将当前行式 main loop 替换为 terminal app 启动入口。
  - 初始化 raw mode、alternate screen、panic/退出恢复逻辑。
  - 调用新增 TUI app runner。
- 设计约束：
  - 终端异常退出时必须 restore terminal。
  - 启动阶段 Gateway 初始化失败可以退出；运行期可恢复错误不退出。
- 验收点：
  - 正常启动进入全屏界面。
  - `/exit` 或 `Ctrl+C` 后终端恢复正常。

### Step 2. 建立 TUI App State 与事件循环

#### 文件：`packages/agent-app/src-tauri/src/tui/app.rs`

- 改动类型：新增
- 改动内容：
  - 定义 `TuiApp`、`FocusPane`、`TuiAction`。
  - 封装启动加载：status、默认 model、会话列表、当前会话历史。
  - 维护输入、消息列表、任务块、错误 banner、当前焦点。
- 设计约束：
  - API：只能通过 `Gateway` 获取业务数据。
  - 状态：TUI state 只用于渲染和交互，不直接持久化。
  - 交互：启动后必须能看到当前上下文或最近会话。
- 验收点：
  - 初始状态能正确反映当前 session、provider/model、conversation count。

#### 文件：`packages/agent-app/src-tauri/src/tui/event.rs`

- 改动类型：新增
- 改动内容：
  - 用 `crossterm::event` 读取 key、paste、resize。
  - 将终端事件转换为 TUI 内部动作。
  - 支持输入框编辑、滚动聊天流、切换焦点、提交消息、打开命令面板、退出。
- 设计约束：
  - 大段粘贴进入 textarea，不触发多次提交。
  - `Enter` 与多行输入快捷键需定义清楚，例如 `Enter` 发送、`Alt+Enter` 或 `Shift+Enter` 换行；若终端无法稳定区分 Shift+Enter，则采用 `Ctrl+J` 换行。
- 验收点：
  - 多行输入、粘贴、提交、退出、resize 都可用。

### Step 3. 建立 Ratatui 渲染层

#### 文件：`packages/agent-app/src-tauri/src/tui/render.rs`

- 改动类型：新增
- 改动内容：
  - 定义整体 layout：顶部上下文区、主聊天流、底部输入区。
  - 顶部显示 project、session、provider/model、config 状态、任务摘要。
  - 主区域渲染 `TuiMessage` 和 `TuiTaskBlock`。
  - 底部渲染 `ratatui-textarea`。
  - 错误 banner 或命令面板以 popup/overlay 展示。
- 设计约束：
  - 样式：user、assistant、tool、error、status 使用不同颜色/边框/标题。
  - 交互：聊天流支持滚动，长内容和工具详情支持折叠。
  - 可读性：错误信息优先显示结构化摘要，不直接输出完整 stack trace。
- 验收点：
  - 窄终端下布局不崩溃。
  - 长消息可滚动查看。
  - 工具状态块可折叠/展开。

### Step 4. 命令解析与轻量切换

#### 文件：`packages/agent-app/src-tauri/src/tui/commands.rs`

- 改动类型：新增
- 改动内容：
  - 将现有 `/help`、`/skills`、`/providers`、`/models`、`/use`、`/call`、`/sessions`、`/history`、`/clear`、`/status`、`/exit` 迁移为结构化命令。
  - 新增 `/model`、`/provider`、`/config`。
  - `/sessions` 打开会话列表或命令面板，而不是只打印文本。
- 设计约束：
  - API：命令只调用 `Gateway` 或更新 TUI state。
  - 兼容：保留 `/use <provider> <model>`。
  - 错误：解析失败返回 `TuiErrorView`，不退出。
- 验收点：
  - 旧命令仍可用。
  - 新命令能完成 provider/model/session/config 查看或切换。

### Step 5. 会话恢复与聊天发送

#### 文件：`packages/agent-app/src-tauri/src/tui/app.rs`

- 改动类型：修改
- 改动内容：
  - 启动时加载最近会话和当前会话历史。
  - 支持选择 session 后刷新聊天流。
  - 普通输入创建 `ModelCall` 任务块，调用 `Gateway::send_model_message`，成功后刷新 history 或追加 user/assistant 投影。
  - 失败时更新任务块为 `Failed` 并展示结构化错误。
- 设计约束：
  - 发送期间 UI 不应空白无反馈。
  - 如果底层调用不可取消，取消按钮/快捷键必须显示为不可用或仅停止等待 UI，不能误导为已取消远端请求。
  - provider/model 未选时不发送请求，直接显示 next actions。
- 验收点：
  - 启动后可看到历史会话。
  - 切换会话后聊天流正确刷新。
  - 普通消息发送成功后写入会话并展示助手回复。
  - 失败不退出 TUI。

### Step 6. 错误映射与任务状态块

#### 文件：`packages/agent-app/src-tauri/src/tui/error_view.rs`

- 改动类型：新增
- 改动内容：
  - 将 `AppError` 映射为 `TuiErrorView`。
  - 为 `model_not_selected`、`provider_auth_missing`、`model_not_found`、`llm_request_failed`、`invalid_input` 等错误提供 what/causes/actions。
- 设计约束：
  - 不直接 dump stack trace。
  - provider 原始错误只作为摘要或展开详情。
- 验收点：
  - 常见错误都有明确“发生了什么 / 可能原因 / 下一步”。

#### 文件：`packages/agent-app/src-tauri/src/tui/task.rs`

- 改动类型：新增
- 改动内容：
  - 定义 `TuiTaskBlock`、状态转换、耗时格式化、折叠逻辑。
  - 第一版覆盖 model call、session load、config/model list load。
  - 为未来 tool call event 预留 `ToolCall` 类型。
- 设计约束：
  - 工具执行块 UI 契约先定义，真实 tool calling 事件需等 core 能力接入。
- 验收点：
  - running/done/failed 状态展示明确。
  - 用户可展开查看详情。

### Step 7. 验证与文档回写

#### 命令

- 运行：
  - `cargo check --manifest-path packages/agent-app/src-tauri/Cargo.toml --bin agent-app-tui`
  - `pnpm --filter agent-app check`
  - `pnpm --filter agent-app tui`
- 手动 smoke：
  - 启动 TUI，确认进入全屏并显示上下文。
  - 粘贴大段文本，确认只进入输入框。
  - 输入多行消息并发送。
  - 使用 `/sessions` 切换会话。
  - 使用 `/provider`、`/model`、`/use` 切换模型。
  - 制造缺 key / 错 model / provider 失败，确认错误结构化且不退出。
  - 发送模型请求，确认 running/done/failed 状态和耗时。
  - `/exit` 或 `Ctrl+C` 后终端恢复正常。

#### 文件：`docs/sdd-lab/2026-07-25_19-22_ratatui-tui-redesign/lifecycle.md`

- 回写执行记录：记录每个执行阶段、偏差、验证结果。
- 记录实际改动摘要：按模块记录。
- 记录验证结果：命令输出摘要和手动 smoke 结果。
- 记录下一步状态：执行完成后更新为 `done`，或若方案需调整回退到 `planned`。

## Risk And Mitigation / 风险与缓解

- 风险：`ratatui` 全屏模式异常退出后终端状态损坏。
  - 缓解方式：统一 terminal guard，确保 drop/错误路径执行 restore；手动 smoke 覆盖 `Ctrl+C`、panic-like error、正常退出。
- 风险：async model call 阻塞 UI 刷新。
  - 缓解方式：事件循环与异步任务解耦，发送时创建任务状态，完成后通过 channel 或 join handle 回写 app state。
- 风险：终端无法稳定区分 `Shift+Enter`。
  - 缓解方式：默认 `Enter` 发送，`Ctrl+J` 或 `Alt+Enter` 换行，并在 `/help` 中说明。
- 风险：真实 tool calling 事件源尚未存在，工具状态块可能只能覆盖模型调用状态。
  - 缓解方式：第一版明确 `TuiTaskBlock` 渲染契约，真实 tool call event 另随 core tool calling 能力接入。
- 风险：TUI state 与 core session state 不一致。
  - 缓解方式：发送成功后以 core history/status 为准刷新；TUI 不直接写 storage。
- 风险：第一版不做 streaming，用户可能仍觉得长回复等待时间长。
  - 缓解方式：展示 running、耗时、可取消状态；streaming 作为后续独立迭代。

## Execute Checkpoint / 执行检查点

- 当前理解：用户要求使用 `ratatui` 重新梳理 TUI 方案，目标是全屏会话聊天工作台，不再接受当前行式 REPL 体验。
- 核心目标：在不破坏 core 业务边界的前提下，引入 `ratatui + crossterm + ratatui-textarea`，实现上下文区、聊天流、多行输入、会话恢复、结构化错误和任务状态块。
- 下一步动作：
  1. 用户审阅 `requirements.md` 与 `technical-plan.md`。
  2. 用户确认 Q1：第一版是否不做 token streaming。
  3. 用户确认 Q2：是否保留 `/use` 并新增 `/model` / `/provider`。
  4. 用户明确说“按方案执行”后，才进入代码实现。
- 风险：主要在 async event loop、终端恢复、工具事件源与 streaming 边界。
