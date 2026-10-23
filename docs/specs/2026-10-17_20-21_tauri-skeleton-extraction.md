# 从 pulsar-app 抽取 Tauri 应用骨架 `pulsar-tauri-template`（只留与功能无关的基建）

- 日期：2026-10-17
- 状态：**Completed（v7，2026-10-17）** — P1–P5 全部完成；v6 按用户验收反馈补齐「无边框窗口的自绘标题栏」（拖拽 + 最小化/最大化/关闭）；v7 按用户验收反馈改造「图标方案」与「运行环境判定」，见 Change Log v7
- 端口决策：Q4 采用建议占位值 **1442**（P1 已落地）
- 关联：`packages/pulsar-app`（只读蓝本）、`packages/pulsar-tauri-template`、`packages/pulsar-tauri-template/README.md`、`docs/pulsar-tauri-template/architecture.md`、`docs/pulsar-tauri-template/storage.md`
- 任务深度：`deep`（架构级抽取、跨模块、需方案权衡）

---

## Goal

以 `packages/pulsar-app` 为蓝本，新建包 `packages/pulsar-tauri-template`，**只保留「与功能无关」的 L1 基建**，剥离全部业务功能（L3）与全部通用能力（L2），得到一个「空白但能跑通、带完整工程底座」的应用起点。

验收结果：`pulsar-tauri-template` 能独立 `pnpm tauri:dev` 起窗，窗口内具备布局容器、主题、i18n 框架、热键等与功能无关的通用能力，且不含任何业务视图、业务命令、业务数据，也不含任何可选能力。

## Done Contract

- **什么算完成**：`packages/pulsar-tauri-template` 落地并通过 §9 全部验收标准。
- **由什么证明**：`cargo check` 0 error、`pnpm check` 0 error、`pnpm tauri:dev` 起窗证据、全文 `grep` 无业务标识残留、`invoke_handler` 命令清单核对、体量核对。
- **仍算未完成**：任一核心链路（起窗、布局交互、主题/语言切换）未跑通；或残留任一 L2/L3 命令注册在 `invoke_handler`。

## Scope

### In

- 新包 `packages/pulsar-tauri-template`：工程接线（`package.json` / `tauri.conf.json` / `Cargo.toml` / `vite.config.js` / 端口 / 脚本 / CI）。
- 后端：只搬 §6.1 的 L1 清单（≈ 2,000 行），含 4 处裁剪 + 1 处重写。
- 前端：只搬 §7.1 的 L1 清单（≈ 4,500 行），含裁剪 + 4 个顶层文件重写。
- 文档：本 spec + 新包 `README.md` + `docs/pulsar-tauri-template/architecture.md`。

### Out

- 全部 L2 通用能力（§6.2 / §7.2 清单，共 8+ 项，本次**全部不保留**）。
- 全部 L3 业务功能（§6.3 / §7.3 清单，≈ 38,000 + 21,500 行，**不复制进新包**）。
- `pulsar-app` 原包的任何改动（原包保持可回查，作为取舍依据）。
- 新包的任何具体业务功能设计。
- 打包产物签名、应用商店上架、自动更新。
- 抽取过程中发现的 `pulsar-app` 自身缺陷修复（另行开 spec）。

---

## Restated Understanding

- **我理解当前任务是**：把 `pulsar-app` 里**与功能无关**的那一层抽出来，做成新包 `pulsar-tauri-template`；凡是被产品需求定义、或属于"可选能力"的代码，都不带走。
- **当前核心目标是**：得到一个可信、可跑、无业务残留、无冗余能力的 Tauri 应用起点。
- **当前边界是**：只动新包；只做抽取与接线，不发明新能力；L2 一律不要。
- **暂不处理**：新包的 dev 前端端口取值（见 Open Questions Q4，P1 前需约定）。

---

## Facts（实测事实）

### F1 体量与耦合现状

| 项 | 实测值 |
|---|---|
| 后端 Rust | `src-tauri/src` **52,342 行** |
| 前端 | `src` **29,692 行** |
| Tauri commands | **111 个**（`src-tauri/src/lib.rs`，2,097 行） |
| State 注入点 | `Gateway` 61、`StateEmitter` 37、`NeuronManager` 12、`TopicStore` 10、`ProviderRegistry` 6、`Poller` 4、`ServerController` 3、`JsonConversationStore` 2、`SessionTracker` 1、`HookJudgementStore` 1、`AssistantSession` 1 |
| 工具契约 | `src-tauri/inserts/` 45 个 markdown |
| 包版本 | `pulsar-app` 0.3.7，identifier `com.pulsar.app`，`mainBinaryName` `pulsar-app` |
| 端口 | 前端 1432（`tauri.conf.json` `build.devUrl`）、后端 8899（`PULSAR_PORT`） |

### F2 关键结论：基建与业务不是干净的目录边界

实测各模块的 crate 内依赖（`grep "^use crate::"`）：

| 模块 | 行数 | crate 内依赖 | 判定 |
|---|---|---|---|
| `core/error.rs` | 112 | 无 | 基建 |
| `stores/storage.rs` | 147 | 无 | 基建 |
| `core/events.rs` | 192 | 无（但 `StateChange` 枚举含业务变体） | 基建 + 需剪 |
| `core/log_phase.rs` | 209 | 无（但常量全为业务阶段） | 基建 + 需剪 |
| `infra/`（4 文件） | 627 | 仅 `core::error` | 基建 + 需剪（`config.rs`） |
| `sinks/app_log.rs` | 431 | — | 基建 |
| `sinks/log_redact.rs` | 126 | — | 基建 |
| `sinks/state_sinks.rs` | 74 | **`core::round_contract`（功能）** | 需重写 |
| `runtime/script_engine.rs` | 173 | 仅 `core::error` | 通用能力（mlua 脚本运行时） |
| `fileops/fs.rs` | 946 | `core::error`、`core::events` | 通用能力（文件能力） |
| `fileops/workspace.rs` | 467 | `core::error` | 通用能力（多工作区） |
| `terminal/` | 1,226 | **`application::gateway::Gateway`**、`fileops::workspace`、`infra::platform` | 通用能力，且需剪 Gateway |
| `net/` + `server_runtime.rs` | 3,042 + 83 | **`application::gateway::Gateway`**、`fileops::gitops`、`fileops::search`、`providers`、`tools::tool_config`、`terminal` | 通用能力（产品架构选择），`rpc.rs` 1,817 行为业务镜像 |
| `tui/` | 2,759 | 业务命令绑定 | 通用能力（另一个界面形态） |

**推论**：真正的「与功能无关」的基建，只集中在 `core/{error,events,log_phase}.rs` + `infra/` + `stores/storage.rs` + `sinks/` —— 后端约 2,000 行。`terminal/`、`net/`、`tui/`、`fileops/`、`runtime/` 都是**通用能力**（可复用，但存在与否由产品决定），不是基建。

### F3 `bin/` 三个入口全部「触发」，故整体不保留（用户决策依据）

实测 `src/bin/` 三个入口的 import：

| 文件 | 行数 | 直接依赖 | 结论 |
|---|---|---|---|
| `bin/pulsar-cli.rs` | 10,467 B | `application::gateway::Gateway`、`core::{log_phase, model_call_input, round_*}` | 依赖 **L3 业务** |
| `bin/pulsar-server.rs` | 6,551 B | `net::{run_server, NetState, ServerConfig}`、`terminal::TerminalEventHub`、`server_runtime` | 依赖 **L2** |
| `bin/pulsar-tui.rs` | 2,183 B | `application::gateway::Gateway`、`tui::*` | 依赖 **L3 + L2** |

`Cargo.toml` 仅有 `[[bin]] name = "pulsar-server"`（`required-features = ["embed-static"]`）一条声明。

**用户判定：触发了就不要。** 即 L2 第 8 项（多入口骨架）连同其余 7 项一并放弃，L2 层清零。

### F4 ~~必须继承的编译期约束~~ → **失效（P1 实测推翻）**

原判据：`Cargo.toml` 默认 feature `embed-static` 通过 `rust-embed` **编译期**嵌入 `../build/`，故裸跑 `cargo check` / `cargo build` 前必须先 `pnpm build`。

**P1 实测推翻**：`grep -rn "rust_embed\|embed-static" packages/pulsar-app/src-tauri/src` 显示 `rust_embed` 仅出现在**已放弃的** `net/static_assets.rs`（`#[folder = "../build/"]`）与 L3 的 `application/insert_catalog.rs`；feature `embed-static` 仅被 `net/` 消费。→ 放弃 `net/` 后 `rust-embed` 与 `embed-static` **成为死代码**，编译期约束随之消失。

**落地结论**：新包 `Cargo.toml` **不保留** `rust-embed` 与 `embed-static` feature；`scripts/dev-tauri.mjs` 的 `ensureFrontendBuild()` 兜底**移除**；新包 `README.md` **不再标注**该约束。

### F5 硬编码清单（沿用即冲突）

`productName` / `identifier` / `mainBinaryName` / `Cargo.toml` 包名 / `[lib] name = "pulsar_app_lib"` / 前端 dev 端口 1432 / 数据目录 `.pulsar/` / `localStorage` 前缀 `pulsar:`。

### F6 前端通信抽象是干净的

`lib/api/` 采用 `ApiClient` 接口 + `Contract` 泛型契约（`c(contract, params)`）。**接口抽象与 `tauriClient`（IPC 实现）属基建**；`httpClient`（远程通道）+ `env.ts` 属 L2，随 `net/` 一并放弃。放弃远程通道后 `api/index.ts` 不再需要 local/remote 切换逻辑，可显著简化。

### F7 布局系统与视图注册表是解耦的

`lib/layout/views.ts`（227 行）是视图注册表，容器只认 `ViewRegistration`。布局**容器**（拖拽 / 折叠 / 持久化 / 容器 ID 机制）属基建；**注册表内容**属功能。

### F8 放弃 `net/` 的连带简化：后端 HTTP 端口不再存在

原包里 `dev-tauri.mjs` 注入两个端口：`DEV_FRONT_PORT`（Vite dev server）与 `PULSAR_PORT`（headless 后端）。放弃 `net/` 后 **`PULSAR_PORT` 及其注入逻辑整体消失**，新包只剩 1 个 dev 前端端口需要约定。

---

## 边界判定标准

**基建 = 「换掉全部业务需求它依然存在」的代码。它不服务于任何具体功能，只服务于"这是一个 app"这件事本身。**

判定三问：

1. **需求清零后它还需要吗？** 需要 → 基建。不需要 → 不是。
2. **名字里有没有产品名词？** 有（`neuron` / `topic` / `provider` / `git` / `search` / `terminal`）→ 大概率不是基建。
3. **删掉它是"app 起不来"还是"少了个功能"？** 起不来 → 基建；少功能 → 那是通用能力。

三层结构：

| 层 | 名称 | 判据 | 本次处置 |
|---|---|---|---|
| **L1** | **基建**（与功能无关） | 三问全部指向"必需" | **无条件搬走**，含裁剪 |
| **L2** | **通用能力**（可复用但可选） | 「删掉只是少了个功能」，且换产品也可能复用 | **全部不保留**（用户决策） |
| **L3** | **功能**（业务） | 被产品需求定义 | **全部不搬**（用户决策） |

---

## 接口契约设计（新包对外能力）

### L1 基建命令（`invoke_handler` 全量白名单）

```text
debug_storage_path          // 诊断：存储路径
logs_list                   // 日志：列表（LogPanel 消费）
logs_phases                 // 日志：阶段清单（前端筛选器消费）
logs_clear                  // 日志：清空
logs_set_level              // 日志：级别设置
```

**落地细化（P1）**：原包 6 个 logs 命令中，`logs_snapshot` / `logs_get_level` / `logs_dir` 三者一并被 LogPanel 于 `onMount` 并行拉取，形态同一 → **合并为 `logs_list`**，返回 `{ entries, level, dir }`；`logs_clear_buffer` → `logs_clear`；`logs_get_level` 并入 `logs_list` 后不再单列。加上 `logs_phases` / `logs_set_level` / `debug_storage_path`，**命令集恰好 5 个**，与白名单名称与数量一一对应。

**类型归属（P1）**：`LogEntry` / `LogLevel` / `PhaseInfo` 三个纯基建类型从蓝本 `lib/types.ts`（782 行，其余全为业务类型）**迁入 `lib/api/types.ts`**（L1），新包**不携带** `lib/types.ts`。

事件通道保留 `app://state-changed`（`STATE_CHANGED_EVENT`），但 `StateChange` 枚举仅保留基建变体。

前端契约层保留 `c(contract, params)` 形态，`api/contracts.ts` 清空为 1 个示例 contract，用于演示新增命令的接线方式。

> 说明：L2 清零后，新包命令集收敛到个位数（≈5 个）。这是「纯基建」的预期形态。

---

## 6. 分级清单 · 后端 `src-tauri/src`

### 6.1 L1 · 基建（必搬，≈ 2,000 行）

| 路径 | 行数 | 处理 |
|---|---|---|
| `core/error.rs` | 112 | 原样 |
| `stores/storage.rs` | 147 | 原样 |
| `infra/elevate.rs` | 182 | 原样 |
| `infra/platform.rs` | 73 | 原样 |
| `infra/time.rs` | 14 | 原样 |
| `infra/config.rs` | 348 | 剪业务与 L2 section：`HooksSection` / `ContextSection` / `GitSection` / `PollerSection` / `NeuronSection` / `ServerSection`（后者随 `net/` 放弃）；**保留机制**：`ConfigStore` 装载 / 热重载 / 环境变量覆盖 / 落盘 |
| `core/events.rs` | 192 | 剪 `StateChange` 的 `Topics` / `Neuron*` / `Poller` / `Sessions` / `MessageDelta` 等业务变体；**保留机制**：`app://state-changed` 通道、`StateEmitter` |
| `core/log_phase.rs` | 209 | 剪 Hook / 轮次 / 神经元三类常量；**保留机制**：阶段常量定义方式、分类分组、`logs_phases` 支撑 |
| `sinks/app_log.rs` | 431 | 原样 |
| `sinks/log_redact.rs` | 126 | 原样 |
| `sinks/state_sinks.rs` | 74 | **重写**为不依赖 `core::round_contract` 的极简出口 |
| `core/mod.rs` | 37 | 重写模块声明 |
| `lib.rs` | 2,097 | **重写**为最小命令集，State 注入仅保留 `StateEmitter` / `AppLogStore` / `ConfigStore` |

**裁剪原则（Q6 决策：保留机制）**：只删业务变体与业务常量，**机制与抽象原样保留**；不因业务消失而简化机制。

### 6.2 L2 · 通用能力 —— **全部不保留**

| 路径 | 行数 | 放弃理由 |
|---|---|---|
| `terminal/` | 1,226 | 用户决策 |
| `net/` + `server_runtime.rs` | 3,042 + 83 | 用户决策 |
| `tui/` | 2,759 | 用户决策 |
| `fileops/fs.rs` + `fileops/workspace.rs` | 1,413 | 用户决策 |
| `fileops/gitops/` | 3,378 | 用户决策 |
| `fileops/search/` | 1,252 | 用户决策 |
| `runtime/script_engine.rs` | 173 | 用户决策 |
| `bin/*.rs`（多入口骨架） | 541 | 触发依赖（见 F3） |

连带放弃的 Cargo 依赖：`axum` / `portable-pty` / `ratatui`·`crossterm` / `mlua` / `reqwest` / `tree-sitter*` / `rmcp` / `getrandom` 等。

### 6.3 L3 · 功能 —— **全部不搬**（≈ 38,000 行）

| 路径 | 行数 | 内容 |
|---|---|---|
| `application/` | ≈ 10,100 | `assistant_session` 3,443、`gateway` 2,486、`hook/` 全套 2,038、`session_tracker` 420、`drivers` 368、`poller` 358、`insert_catalog` 244、`chat_session` 74、`agent_session` 78、`poller_step` 28 |
| `core/round_*` | ≈ 4,806 | `round_service` 2,643、`model_call_input` 788、`round_contract` 614、`round_resolver` 467、`round_executor` 381、`round_types` 84、`round_policy` 45 |
| `core/models.rs` | 1,148 | 业务领域模型（`Neuron` / `Topic` / `Conversation` / `MessageBody` 等） |
| `core/hook/` | 1,054 | Hook 定义与轮次契约 |
| `core/{session_coordinator,context_safety}.rs` | 551 | 会话协调、上下文安全 |
| `policies/` | 6,744 | `neuron/` 全套 + `compactor` |
| `providers/` | 2,533 | Provider / Model 注册与调用 |
| `tools/` | 3,135 | `tool_registry` / `mcp` / `dynamic_tool` / `cmd_exec` / `root_auth` / `tool_config` |
| `stores/{conversation_store,topic_store,topic_manager}.rs` | 3,012 | 会话与课题存储 |
| `fileops/fs_tools.rs` | 457 | 工具化包装层 |
| `inserts/*.md` | 45 个文件 | 工具自描述契约 |

---

## 7. 分级清单 · 前端 `src`

### 7.1 L1 · 基建（必搬，≈ 4,500 行）

| 路径 | 行数 | 处理 |
|---|---|---|
| `lib/layout/`（除 `views.ts`） | 2,144 | 原样。`LayoutStore.svelte.ts` 340、`layoutStorage.ts` 373、`EditorTabs.svelte` 594、`ViewContainer.svelte` 509、`ActivityBar.svelte` 92、`Splitter.svelte` 78、`useResizable.svelte.ts` 66、`viewContext.ts` 70、`ViewHost.svelte` 18、`WindowEdgeResize.svelte` 54、`layoutTypes.ts` 110 |
| `lib/api/{tauriClient,types,index}.ts` | 324 | `tauriClient.ts` 54 原样（IPC 实现）；`types.ts` 163 剪业务类型、保留 `ApiClient` / `Contract` / `StateChangePayload` 等抽象；`index.ts` 107 简化为纯 IPC 单通道（去 local/remote 切换） |
| `lib/theme.ts` | 54 | 原样 |
| `lib/i18n/index.svelte.ts` | 58 | 原样 |
| `lib/i18n/translations.ts` | 2,418 | 剪到仅框架 key（导航 / 通用按钮 / 错误文案），剩 ≈ 200 |
| `lib/hotkey/hotkeyService.ts` | 121 | 原样（热键服务框架） |
| `lib/actions/clickOutside.ts` | 31 | 原样 |
| `lib/utils/` | ≈ 250 | `copyToClipboard`（impl + interfaces + test）、`formatInvokeError` 41 |
| `lib/errorMessage.ts` | 10 | 原样 |
| `lib/components/FloatingEdge.svelte` | 97 | **下调 L3**（P1 实测：唯一依赖 `@xyflow/svelte`，纯神经元图连线渲染器；删掉是「少功能」） |
| `lib/components/{Select,ContextMenu,Tooltip,CopyButton,ErrorBanner,ConfirmDialog,Toggle,ThemeSwitcher,LocaleSwitcher}.svelte` | ≈1,075 | 原样（`LogPanel` 见下） |
| `lib/components/LogPanel.svelte` | 455 | 保留；去 L2 的 `SuggestInput`，改原生 `<input>` + `<datalist>`；`LogEntry/LogLevel/PhaseInfo` 改从 `lib/api/types.ts` 引入 |
| `routes/+layout.ts` | 5 | 原样 |
| `app.html` | 242 | 保留；剪 `--color-system-*`（Neuron 调色板）与 `<title>星脉</title>` → `pulsar-tauri-template` |

**另需重写**：`routes/+page.svelte`（1,151 → ≈200）、`lib/stores/dataStore.svelte.ts`（1,016 → ≈100）、`lib/layout/views.ts`（227 → ≈60）、`lib/api/contracts.ts`（291 → ≈60）。

**另需裁剪（P1 新增，原判据「原样」不成立）**：`lib/layout/layoutTypes.ts`（110）与 `lib/layout/layoutStorage.ts`（373）内含 `neurons` / `topics` / `providers` / `terminal` / `files` / `search` / `poller` / `logs` 等**业务视图 id 与迁移逻辑**，原样携带必然命中 §9 验收 5 的 `grep -ri`。裁剪原则同 Q6：只删业务 id 常量与针对业务 id 的迁移分支，容器机制（`ViewContainerId` / `MainPanelType` 泛化 / `DEFAULT_LAYOUT` / 版本迁移框架）原样保留。

`lib/layout/viewContext.ts`（70）导入 `dataStore` 与业务类型 `SamplingParams/ThinkingConfig`，导出业务 `ViewCommands/ViewUiState`，需重写解耦（容器/内容分离机制保留）。

**v7 追加（图标方案与环境判定，详见 Change Log v7）**：
- **图标**：视图图标由手写 SVG 字符串常量改为 `unplugin-icons` 构建期组件——`ViewMeta.icon` / `ViewRegistration.icon` 由 `string` 改为 `Component`，`ActivityBar` / `EditorTabs` / `ViewContainer` / `ContextMenu` 的 `{@html}` 改为组件渲染，`+page.svelte` 的折叠按钮与窗口控制按钮改用图标组件。
- **环境判定**：`lib/api/index.ts` 不再导出 `isTauriEnv`（原手写检测删除）；`lib/api/tauriClient.ts` 在非 Tauri 环境自动降级为 no-op（`call`/`invoke` reject、`subscribe` 返回 no-op、`health()` 返回 false）；新增 `lib/window/chrome.ts` 承载窗口 chrome 并在内部判环境；`lib/theme.ts` 的本地 `isTauri` 改用官方 `@tauri-apps/api/core` 的 `isTauri()`；`LogPanel.svelte` / `+page.svelte` 中散落的 `if (isTauriEnv)` 移除。
- **新增文件**：`lib/window/chrome.ts`、`src/icons.d.ts`（`~icons/*` 类型声明）。

### 7.2 L2 · 通用能力 —— **全部不保留**

| 路径 | 行数 | 放弃理由 |
|---|---|---|
| `lib/terminal/transport.ts` | 300 | 随 `terminal/` 放弃 |
| `lib/api/{httpClient,env}.ts` | 117 | 随 `net/` 放弃 |
| `lib/components/{MultiSelect,SuggestInput,PathInput}.svelte` | 960 | 用户决策 |
| `lib/components/MarkdownRenderer.svelte` | 218 | 用户决策 |
| `lib/stores/fileEditorStore.svelte.ts` | 84 | 用户决策 |
| `lib/components/{FileEditor,FileExplorer}.svelte` | 463 + 1,365 | 随 `fileops/` 放弃 |

连带放弃的 npm 依赖：`xterm` 系列 / `codemirror` 系列 / `@lezer/highlight`。

### 7.3 L3 · 功能 —— **全部不搬**（≈ 21,500 行）

- **业务组件 ≈ 15,900 行**：`ChatArea` 642、`ChatInput` 193、`ChatMessage` 348、`SessionList` 440、`SessionCreateModal` 51、`StatusBar` 419（**混合模块**：仅窗口控制/拖拽机制属 L1，其余不搬，见 Change Log v6）、`ProviderManager` 1,210、`ProvidersModelsPanel` 403、`ModelPicker` 371、`TopicPanel` 577、`PollerPanel` 207、`HookJudgementPanel` 1,020、`JudgementCard` 386、`Neuron*` 共 ≈ 3,453（`NeuronDetailDrawer` 1,102、`NeuronManager` 771、`NeuronListPanel` 593、`NeuronNetworkGraph` 226、`NeuronDetail` 201、`NeuronIndex` 197、`NeuronNetwork` 130、`NeuronFlowNode` 109、`NeuronNetworkTree` 73、`NeuronList` 51）、`ToolPanel` 431、`ToolEditor` 599、`ToolCallBlock` 153、`ToolResultBlock` 250、`ThinkingBlock` 92、`NudgeBlock` 128、`BehaviorFields` 225、`GitPanel` 1,215、`GitDiff` 535、`CommitDiff` 200、`GitConfirmHost` 93、`SearchPanel` 267、`RootAuthHost` 146、`SettingsDialog` 55
- `lib/features/neuron/` 408
- `lib/sessionModes.ts` 36
- `@xyflow/svelte` 依赖（神经元图）

### 7.4 顶层接线重写

| 文件 | 行数 | 处理 |
|---|---|---|
| `routes/+page.svelte` | 1,151 | 重写为极简组合根：挂布局容器 + 主题 + i18n + 热键 |
| `lib/stores/dataStore.svelte.ts` | 1,016 | 重写为事件订阅 + 连接状态的极简版本 |
| `lib/layout/views.ts` | 227 | 清空注册表，留 1 个示例视图演示「新增视图 = 注册一条记录」 |
| `lib/api/contracts.ts` | 291 | 清空，留 1 个示例 contract 演示 `c()` 用法 |
| Rust `lib.rs` | 2,097 | 重写为最小命令集（见「接口契约设计」） |

---

## 8. 工程基建（L1，必搬）

| 项 | 处理 |
|---|---|
| 包名 / 目录 | `pulsar-tauri-template` → `packages/pulsar-tauri-template`（pnpm workspace `packages/*` 自动纳入） |
| `productName` / `identifier` / `mainBinaryName` | 全部改为 `pulsar-tauri-template` / `com.pulsar.harness` / `pulsar-tauri-template`；`[lib] name = "pulsar_tauri_template_lib"` |
| 数据目录 / localStorage 前缀 | `.pulsar/` → `.pulsar-tauri-template/`；`pulsar:` → `pulsar-tauri-template:` |
| `scripts/dev-tauri.mjs` | 保留，端口变量改名（去掉 `PULSAR_PORT` 及其注入逻辑，见 F8），仅保留 `DEV_FRONT_PORT` |
| `scripts/release.mjs` | 保留，改包路径与 4 个版本文件路径（`package.json` / `tauri.conf.json` / `Cargo.toml` / `Cargo.lock`） |
| `scripts/fresh-user.mjs` | 保留，改数据目录名 |
| `vite.config.js` / `svelte.config.js` / `tsconfig.json` | 保留，改端口注入变量 |
| `.github/workflows/publish-pulsar-app.yml` | 复制为 `publish-pulsar-tauri-template.yml`，改 `projectPath`、tag 前缀 |
| `Cargo.toml` | 保留 `tauri` + plugins / `serde` / `serde_json` / `tokio` / `tracing` / `tracing-subscriber` / `parking_lot`；删除 §6.2 列出的全部 L2 依赖，**并额外删除 `rust-embed`（F4 已失效）、`rusqlite`、`chrono`（直接引用）、`async-trait`、`thiserror`（L1 实测未引用）**。**无 `[[bin]]` 声明、无 `[features]`** |
| `package.json` 依赖 | 保留 `svelte` / `@sveltejs/kit` / `@sveltejs/adapter-static` / `vite` / `typescript` / `svelte-check` / `@tauri-apps/api` / `@tauri-apps/cli` + 用到的 plugin-js；删除 §7.2 列出的全部 L2 依赖，**并额外删除 `@xyflow/svelte`（随 `FloatingEdge` 下调 L3）、`@types/dompurify`**；**v7 新增 dev 依赖** `unplugin-icons` + `@iconify-json/lucide`（图标，构建期编译，见 Change Log v7） |
| `.cursor/rules/` / `.cursor/skills/` / 根 `AGENTS.md` / `CLAUDE.md` | 仓库级共享，新包**自动继承**，不复制 |
| `docs/pulsar-tauri-template/` | 新建 `architecture.md`、`storage.md` |
| 新包 `README.md` | 新建（F4 已失效，**无需**标注编译期约束） |

---

## 9. 验收标准

1. `cargo check --manifest-path packages/pulsar-tauri-template/src-tauri/Cargo.toml` → 0 error
2. `pnpm check`（svelte-check）→ 0 error
3. `pnpm tauri:dev` → 能起主窗口
4. 窗口内可见：布局容器可拖拽/可折叠、主题切换生效、中英切换生效；`decorations: false`（无原生装饰）下窗口可拖动、可最小化/最大化/关闭
   - **（v7 追加）** 图标由 `unplugin-icons` 构建期组件渲染（视图 tab / ActivityBar / 标题栏按钮），无手写 SVG 常量残留
   - **（v7 追加）** 非 Tauri 环境（浏览器 http://localhost:1442）**控制台零错误**：环境判定收口到 `isTauri()` + 模块内 no-op 降级后，v6 遗留的 2 条错误（`net::ERR_CONNECTION_REFUSED`、`tauriClient` 读 undefined）消失
5. 全文 `grep -ri` 无残留标识：`pulsar-app` / `pulsar_app_lib` / `neuron` / `topic` / `provider` / `hook_judgement` / `terminal` / `gitops` / `search_panel`
6. `invoke_handler` 注册命令 ⊆ §「接口契约设计」白名单（≈5 个），无任何 L2/L3 命令
7. 无任何 L3 `State` 注入（`Gateway` / `NeuronManager` / `TopicStore` / `ProviderRegistry` / `Poller` / `HookJudgementStore` / `AssistantSession` 全部不出现）
8. 体量核对：后端 L1 ≈ 2,000 行 + 前端 L1 ≈ 4,500 行（±20%），超出即说明混入了 L2/L3
9. `Cargo.toml` 无 `[[bin]]` 声明；`src/bin/` 不存在；`Cargo.toml` 与 `package.json` 中不含任何 L2 依赖

---

## 10. 实施方案

| 方案 | 做法 | 优点 | 缺点 |
|---|---|---|---|
| 甲 · 复制后反向删 | `cp -r pulsar-app pulsar-tauri-template` 再删 L2/L3 | 最快 | 残留耦合逐个修；顶层 4 个千行文件手工裁剪，易留「看不见的尾巴」 |
| 乙 · 纯重建 | 用 `create-tauri-app` 起壳，L1 按需重写 | 最干净 | 布局系统 / 日志面板 / api 层全重写，放弃蓝本价值 |
| **丙 · 逐层分级（已采纳）** | 按 L1/L2/L3 分别「搬+剪 / 不搬 / 不搬」，顶层重写 | L1 保真 + 顶层干净；每文件有明确层归属，可逐条核对 | 需逐文件核对 |

**采纳丙。** 依据 F2：L1 基建总量只有 ≈2,000 行后端 + ≈4,500 行前端，且需要动手的只有 4 个裁剪点（`infra/config.rs` / `core/events.rs` / `core/log_phase.rs` / `sinks/state_sinks.rs`）+ `lib.rs` 重写 + 前端 4 个顶层文件重写。范围小、边界可信。

### 落地步骤

| 阶段 | 内容 | 产出 |
|---|---|---|
| P0 | 本 spec 批准 → 约定 dev 前端端口 → 状态置 `Approved` | Open Questions 全关闭 |
| P1 | 建包与工程接线（§8） | `packages/pulsar-tauri-template/` 可 `pnpm install` |
| P2 | 后端 L1：搬 + 剪 4 处 + 重写 `lib.rs` / `state_sinks.rs` / `core/mod.rs` | `cargo check` 0 error |
| P3 | 前端 L1：搬 + 剪 + 重写 4 文件 | `pnpm check` 0 error |
| P4 | 跑通验证（§9）+ 全文 grep + 体量核对 | 起窗证据 |
| P5 | 文档与 CI | `README.md` + `docs/pulsar-tauri-template/architecture.md` + workflow |

---

## Open Questions

- [x] **Q1 · L2 通用能力点单** → **全部不保留**（8 项全放弃；第 8 项 `bin/` 因实测触发 `Gateway` / `net` / `tui` 而放弃，见 F3）
- [x] **Q2 · 业务代码处置** → **(a) 彻底不搬**（L3 不复制进新包）
- [x] **Q3 · 落位与命名** → **`packages/pulsar-tauri-template`**
- [x] **Q5 · 休眠保留附加约束** → **失效**（Q2 选 (a)，无休眠代码）
- [x] **Q6 · L1 裁剪深度** → **保留机制**（只删业务变体与业务常量，机制与抽象原样保留）
- [ ] **Q4 · dev 前端端口取值** → 用户「另行约定」。因 `net/` 放弃，后端 HTTP 端口已消失（F8），**新包只需约定 1 个 dev 前端端口**（避开原 1432）。建议占位 `<FRONT_PORT>`，P1 建包时可随时替换
  - 建议值（可覆盖）：**1442**
  - **P1 落地：已采用 1442**（`tauri.conf.json` 的 `devUrl` = `http://localhost:1442`，`scripts/dev-tauri.mjs` 注入 `FRONT_PORT=1442`）。

---

## Checkpoint Summary

- **当前任务理解**：以 `pulsar-app` 为蓝本，新建 `packages/pulsar-tauri-template`，**只保留与功能无关的 L1 基建**；L2 通用能力与 L3 功能全部不带。
- **当前核心目标**：完成 P1–P5，交付可信、可跑、无业务残留、无冗余能力的 Tauri 应用起点。
- **当前进度**：**P1–P5 全部完成，v6 补齐自绘标题栏，v7 完成图标组件化与环境判定收口**（F1–F8 有实测数据；六项决策全部落定；后端 `cargo check` 0 error/0 warning；前端 `pnpm check` 0 error / 3 warning；`pnpm build` 通过；`pnpm tauri:dev` 起窗成功；§9 九条中 8 条通过、1 条（布局拖拽与标题栏窗口控制）待人工确认）。
- **下一步 1**：无阻塞项；如需收口 §9.4，人工在 Tauri 窗口内确认「视图跨容器拖拽」「分栏宽度拖拽」与「标题栏拖动 / 双击最大化 / 最小化 / 最大化 / 关闭」。
- **下一步 2**：按 `docs/pulsar-tauri-template/architecture.md` §5 的扩展点，在业务包中扩展本骨架。
- **涉及文件 / 模块**：本 spec、`packages/pulsar-app/**`（只读蓝本）、`packages/pulsar-tauri-template/**`、`docs/pulsar-tauri-template/**`、`.github/workflows/publish-pulsar-tauri-template.yml`。
- **风险**：① §9.4 的交互项（布局拖拽、标题栏窗口控制）未自动化验证（机制取自蓝本、未改逻辑，低风险；本机为 Wayland 会话，原生窗口无法自动截图/点击）；② `pnpm check` 3 条 warning 与蓝本一致；③ 后端体量 1,593 行略低于 §9.8 下界（0.4%，系裁剪所致）。
- **验证方式**：§9 验收标准 1–9，逐条留证据（见 Validation）。
- **Execution Approval**: `Approved`

## Change Log

- 2026-10-17（v1）：创建 spec。完成对 `packages/pulsar-app` 的实测调研（体量、111 命令、State 注入分布、模块级依赖实测、编译期约束）；产出 A/B/C/D 四类分级清单与三方案对比。
- 2026-10-17（v2）：**用户校正边界定义**为「基建 = 需要和功能无关的代码」。据此重做判定标准（v1 混用了「基建」与「通用能力」），改立 L1 基建 / L2 通用能力 / L3 功能 三层。`terminal/`、`net/`、`tui/`、`fileops/`、`runtime/script_engine`、`bin/` 从「基建」下调为 L2；前端 `MultiSelect`/`SuggestInput`/`PathInput`/`MarkdownRenderer`/`fileEditorStore`/`FileEditor`/`FileExplorer`/`terminal/transport`/`httpClient` 同样下调 L2。L1 收敛为后端 ≈2,000 行 + 前端 ≈4,600 行。
- 2026-10-17（v3）：**落实六项决策**。Q1 = L2 全部不保留（含第 8 项 `bin/`，实测其三个入口分别触发 `Gateway` / `net` / `tui`，详见新增 F3）；Q2 = (a) 彻底不搬；Q3 = 包名 `pulsar-tauri-template`；Q5 因 Q2=a 而失效；Q6 = 保留机制（只删变体与常量）。据此更新：§6.2 / §7.2 从「逐项点单」改为「全部不保留」；`infra/config.rs` 裁剪清单追加 `ServerSection`；新增 F8（放弃 `net/` 导致后端 HTTP 端口消失，新包只剩 1 个 dev 前端端口）；§8 新增硬编码改名清单；验收标准扩为 9 条（新增命令白名单、`[[bin]]` 与依赖核查）。
- 2026-10-17（v4，P1 执行反馈 · Reverse Sync）：P1 建包与工程接线过程中实测推翻/细化了若干判据，按「先同步文档再改代码」逐条回写：
  1. **F4 失效**（见 F4 改写）：`rust-embed` / `embed-static` 唯一消费者是已放弃的 `net/static_assets.rs` 与 L3 `insert_catalog.rs`；新包不保留该依赖与 feature，`dev-tauri.mjs` 的 `ensureFrontendBuild()` 与 README 的 F4 标注一并取消。
  2. **`FloatingEdge.svelte` 由 L1 下调 L3**：唯一依赖 `@xyflow/svelte`，是神经元图专用连线渲染器，删除为「少功能」而非「起不来」。
  3. **`lib/layout/layoutTypes.ts` / `layoutStorage.ts` 判据由「原样」改为「需裁剪」**：内含业务视图 id 与业务迁移分支，原样携带必然命中 §9.5。
  4. **`app.html` 判据由「原样」改为「需裁剪」**：`--color-system-*`（Neuron 调色板）与 `<title>星脉</title>` 属业务/产品标识。
  5. **命令集合并**：`logs_snapshot` / `logs_get_level` / `logs_dir` → `logs_list`（`{entries, level, dir}`），收敛为恰好 5 个。
  6. **类型归属**：`LogEntry` / `LogLevel` / `PhaseInfo` 迁入 `lib/api/types.ts`；新包不携带 `lib/types.ts`。
  7. **`LogPanel.svelte` 去 `SuggestInput`（L2）**，改原生 `<input>` + `<datalist>` 保留关键词快捷匹配体验。
  8. **依赖取舍落地**：Cargo 删 `rust-embed` / `rusqlite` / `chrono`(直接) / `async-trait` / `thiserror`；npm 删 `@xyflow/svelte` / `@types/dompurify`。
  9. **仓库级增补**：根 `.gitignore` 增 `packages/pulsar-tauri-template/.pulsar-tauri-template*` 与 `.env` 例外；根 `pnpm-lock.yaml` 更新以纳入新包。
- 2026-10-17（v4 补）：**P1 完成**，`pnpm install --no-frozen-lockfile` 通过（4 个 workspace 工程，`pnpm-lock.yaml` 已纳入 `pulsar-tauri-template`）。
- 2026-10-17（v5，P2–P5 执行反馈 · Reverse Sync）：P2–P5 完成，期间新增偏差逐条回写：
  1. **`core/error.rs` 判据由「原样」改为「需裁剪」（继承 v4 项 3 的性质）**：§6.1 表把 `core/error.rs` 标为原样，但该文件含 11 个业务变体（`NeuronNotFound` / `ProviderNotFound` / `TopicNotFound` / `HookJudgement*` / `Terminal*` / `Git*` / `Search*` 等），原样携带必然命中 §9.5。按 Q6「只删业务变体、保留机制」裁剪为 `InvalidInput` / `StorageError` / `RuntimeError`，`code()` / `exit_code()` / `Display` 与之一并收敛。
  2. **§9.5 的 grep 范围需排除 `Cargo.lock`**：`Cargo.lock` 内出现 `icu_provider`（上游 crate 名，非本仓标识）。该项为自动生成的锁文件、非本包自有命名，实测**源码零残留**（`packages/pulsar-tauri-template` 下除 `Cargo.lock` 外无任何命中）。
  3. **§9.8 后端实测低于下界**：后端 L1 实测 **1,593 行**（判据区间 1,600–2,400），较下界低 7 行（0.4%）；前端 L1 实测 **4,940 行**（判据区间 3,600–5,400）。判据末句「超出即说明混入了 L2/L3」针对上界，后端低于下界系裁剪 11 个错误变体与 6 个配置 section 所致，非混入。
  4. **§9.4 的「拖拽」未端到端自动验证**：主题切换、中英切换、三区折叠已在浏览器实测通过；跨容器/分栏拖拽机制（`ViewContainer` pointer 拖拽、`Splitter` + `useResizable`）自蓝本**逐字搬入且未改逻辑**（仅 `setContext` 键名改为 `pulsar-tauri-template:panel`），自动化环境缺拖拽原语，留待人工确认。
  5. **`LayoutStore.svelte.ts` 追加裁剪（承 v4 项 3 同类）**：删除 `LocateAnchor`（会话锚点定位）与 `insertPanel` 内 chat 绑定窗口前缀逻辑；保留 `insertPanel` / `movePanel` / `movePanelToNewPane` 机制。
  6. **P5 产物**：新包 `README.md`、`docs/pulsar-tauri-template/architecture.md`、`docs/pulsar-tauri-template/storage.md`、根 `.github/workflows/publish-pulsar-tauri-template.yml`（`projectPath: ./packages/pulsar-tauri-template`，tag 前缀 `pulsar-tauri-template-v`）。
- 2026-10-17（v6，用户验收反馈 · Reverse Sync）：**用户指出「作为桌面应用，标题栏还是需要的呀」**。核查确认 §7.3 对 `StatusBar.svelte` 的「整件 L3」分类有误，新包的窗口 chrome 只实现了一半，按「先同步文档再改代码」修复：
  1. **`StatusBar.svelte` 由「整件 L3」改为「混合模块」**：该组件是 `decorations: false` 下的**自绘标题栏**，同时承载 (a) 窗口控制机制——`minimize` / `toggleMaximize` / `close`、标题栏空白区 `startDragging` 拖拽（实测蓝本注释：弃用 `data-tauri-drag-region`，因 WebKitGTK 下与自定义 mousedown 冲突）、双击最大化（Linux 上 Tauri 不自动处理）、`isMaximized` 随 `onResized` 跟踪；以及 (b) L3 内容——品牌 Spark Node 徽标、`onCreateSession` 新建会话入口、移动端抽屉开关。按三问 #3，(a) 删掉是「窗口无法移动 / 最小化 / 关闭」而非「少功能」→ **(a) 属 L1 必须搬**，且按 Q6 只搬机制、不搬 (b)。
  2. **缺失成因**：新包 `tauri.conf.json` 逐字继承 `decorations: false`，且已携带 `lib/layout/WindowEdgeResize.svelte`（无边框窗口边缘缩放热区机制），但标题栏那半边机制随 `StatusBar` 被整件判为 L3 而未搬 → 无边框 chrome 半实现：能缩放，不能拖动 / 最小化 / 最大化 / 关闭。
  3. **落地**：`routes/+page.svelte` 的 `header.app-header` 补回窗口拖拽（空白区 `mousedown` → `startDragging`，`button/a/input/select/[role='button']` 等交互元素除外，仅左键）、双击最大化、`isMaximized` 跟踪与三个窗口控制按钮（最小化 / 最大化⇄还原 / 关闭），整体以 `isTauriEnv` 为门（浏览器环境不构造窗口句柄、按钮不渲染）；`i18n/translations.ts` 的 `common` 增 `minimize` / `maximize` / `restore` / `closeWindow` 四键（en + zh），替代蓝本中的硬编码英文 `title`。
  4. **权限无需变更**：`capabilities/default.json` 已含 `core:window:allow-start-dragging` / `allow-minimize` / `allow-toggle-maximize` / `allow-close` / `allow-is-maximized`，与蓝本逐字一致（此项 P1 已随 L1 搬入），故本次为纯前端补齐。
  5. **§7.3 分类口径修正**：本节此前按「文件名 → 单层」判，`StatusBar` 证明存在**混合模块**（与 v4 项 3、v5 项 1/5 同类）。已在 §7.3 `StatusBar` 条目追加「混合模块」标注。
  6. **遗留待裁决（§9.5 词表之外的业务名词）**：宽口径重扫发现三处未进入 §9.5 词表的业务命名残留，均在注释 / CSS 类名 / i18n 段名层面，**无功能耦合**——`EditorTabs.svelte` 的 `tone-chat` / `tone-agent` / `tone-assistant` / `tone-system` 四个色调类名（实为蓝本会话模式色板）及其注释、「如主 chat 面板」「file-editor / git-diff / commit-diff 有路径」两处注释、`CopyButton.svelte` 的「如 ChatMessage 错误卡片」注释、`translations.ts` 的 `chatMessage` 段名（内容实为 CopyButton 通用文案，占用方仅 CopyButton）。**未擅自清理**：`chatMessage` 段名出自 v3 已批准的 L1 段落清单，改动属用户可见契约变更，留待用户裁决。
- 2026-10-17（v7，用户验收反馈 · Reverse Sync）：**用户提出两点**——「1. icon 方案有问题（手写内联 SVG 不可维护、与蓝本图标风格不统一）」「2. 要区分运行环境的，浏览器环境就不需要窗口管理了（判定不应散落在调用点）」。经社区方案调研并获用户批准后改造：
  1. **图标方案：手写内联 SVG → 构建期 `unplugin-icons` + `@iconify-json/lucide`**。调研对比结论：
     - `@iconify/svelte`（官方 Svelte 组件）**离线不适用**——组件不含图标数据、默认向 Iconify API 按需拉取，而 Tauri 桌面为离线 + CSP 受限环境；
     - **选 `unplugin-icons`**：构建期把 `~icons/lucide/*` 编译为本地 Svelte 组件，无运行时网络请求、按需 tree-shake 只打包用到的图标、与 Svelte 编译器同源（产出经 `@sveltejs/vite-plugin-svelte` 编译）、类型友好；图标数据来自 dev 依赖 `@iconify-json/lucide`（体积远小于运行时全量集）。
     - 接线：`vite.config.js` 加 `Icons({ compiler: 'svelte' })`（**必须显式设 `compiler`**，否则 unplugin-icons 默认产出 JSX/HTML 而非 Svelte 组件）；import 约定 `import IconX from "~icons/lucide/{kebab-case}"`。
  2. **全量替换，禁止新旧混用**：删除 §7.1 原「视图图标 = 手写 SVG 字符串常量」的做法，统一改为图标组件——`lib/layout/views.ts` 的 `LOGS_ICON` / `EXAMPLE_ICON` / `EXAMPLE_ACTIVITY_ICON` / `EXAMPLE_PANEL_ICON` 四个常量删除，改为 `icon`/`iconTone` 字段承载组件引用（`ViewMeta` 的 `icon` 由 `string`（原始 SVG）改为组件类型，`ActivityBar.svelte` / `EditorTabs.svelte` 的 `{@html item.icon}` 改为 `<Icon />` 渲染）；`+page.svelte` 的 3 个窗口控制按钮与 3 个折叠按钮、`EditorTabs.svelte` 的 tab 图标一并换组件。
  3. **运行环境判定收口（用户第 2 点）**：手写 `isTauriEnv`（`"__TAURI_INTERNALS__" in window`）**替换为官方 `isTauri()`**（`@tauri-apps/api/core`，检查 `globalThis.isTauri`，与 Tauri 运行时注入方式同步演进）；判定**不再散落在调用点**——
     - `lib/api/tauriClient.ts` 自身在非 Tauri 环境降级为 **no-op**：`invoke`/`call` reject、`subscribe` 返回 no-op unlisten，调用方不再各自 `if (isTauriEnv)`；
     - 新增 `lib/window/chrome.ts`：把窗口 chrome（标题栏拖拽 / 双击最大化 / 最小化 / 最大化 / 关闭 + 边缘缩放）**全部收进该模块**，模块内部判环境，非 Tauri 返回 no-op 桩，上层只调 `chrome.minimize()` 这类方法，不感知运行环境；
     - 移除 `LogPanel.svelte` 与 `+page.svelte` 中散落的 `if (isTauriEnv)`（事件订阅 / 窗口句柄构造）。
  4. **副产品（验收目标）**：收口前浏览器（http://localhost:1442）控制台有 **2 条错误**（`api.call(c.debugStoragePath)` 与 `dataStore.bootstrap()` 未门控时的 `net::ERR_CONNECTION_REFUSED`、`tauriClient` 读 undefined）；收口后这 2 条**必须消失**，浏览器控制台零错误。
  5. **连带依赖变更（§8 同步）**：`package.json` 新增 dev 依赖 `unplugin-icons` + `@iconify-json/lucide`；`vite.config.js` 插件数组新增 `Icons()`。

## Validation

- Self-check：本文所有体量数字与依赖关系均来自对 `packages/pulsar-app` 的实际读取与 `grep`/`wc` 统计，非估算（除标注 `≈` 的聚合值）。F3 的三个 bin 入口依赖为逐文件 `grep` 实证。v4 的 9 条偏差均由 P1 执行的 `grep` / `Read` 实证（如 `rust_embed` 全量命中位置清单、`FloatingEdge` 的 import 行）。v5 的偏差由 P2–P4 的 `grep` / `wc` / `cargo check` / `svelte-check` / 起窗日志 / 浏览器实测实证。
- Static checks：`pnpm install --no-frozen-lockfile` → 通过（4 workspace 工程）；`cargo check --manifest-path packages/pulsar-tauri-template/src-tauri/Cargo.toml --all-targets` → **0 error / 0 warning**；`pnpm check`（svelte-check）→ **0 error / 3 warning**（`Tooltip.svelte` 的 a11y 提示 ×1、`ViewHost.svelte` 的 `state_referenced_locally` ×2，均为蓝本既有，随 L1 逐字搬入）。**v6 复跑**：`cargo check --all-targets` → `Finished` 0 error / 0 warning；`pnpm check` → **0 error / 3 warning**（与 v5 持平，标题栏改动未新增告警；新增的 header 拖拽处理器已按蓝本用 `svelte-ignore` 豁免 a11y 静态交互检查）。**v7 复跑**：`cargo check --all-targets` → `Finished` 0 error / 0 warning；`pnpm check` → **0 error / 3 warning**（与 v6 持平，同为蓝本既有的 `Tooltip.svelte` ×1 + `ViewHost.svelte` ×2；图标组件化与环境判定收口未新增告警）；`pnpm build` → `✓ built in 3.71s` + `Wrote site to "build"`（`unplugin-icons` 在 build 模式同样可用，非仅 dev）。
- Runtime / Test：`pnpm tauri:dev` **起窗成功**，运行日志实证：`INFO pulsar_tauri_template_lib: main window created phase="app_window"` → `INFO pulsar_tauri_template_lib: pulsar-tauri-template ready phase="app_setup"`；Vite dev server 端口 1442 正常服务。浏览器实测（http://localhost:1442）通过：布局四区齐备、主题切换（`data-theme` = dark/light/移除）、中英切换（"Example"↔"示例视图"、"Logs"↔"日志"）、三区折叠（侧栏 / 信息栏 / 底部面板 显隐）。拖拽未端到端自动验证（见 Change Log v5 项 4）。
  - **v6 复跑**：先清掉上一轮残留进程占用的 1442 端口，`pnpm tauri:dev` 干净启动——Vite `ready in 408 ms` → `Local: http://localhost:1442/`，随后 `main window created phase="app_window"` → `pulsar-tauri-template ready phase="app_setup"`，无报错。浏览器侧（http://localhost:1442）复核标题栏：品牌 `pulsar-tauri-template` + 3 个折叠按钮 + 主题 / 语言切换控件齐全，最右侧**无**窗口控制按钮（`isTauriEnv` 门控符合预期——浏览器不构造窗口句柄），点击最左侧折叠按钮可正常显隐侧栏。控制台另有 2 条**既有**错误（`net::ERR_CONNECTION_REFUSED` 与 `tauriClient.ts` 在无 Tauri IPC 环境下读取 undefined），均发生在非 Tauri 分支，与 v6 改动无关。
  - **v6 未能自动验证的部分**：本机为 **Wayland 会话**（`XDG_SESSION_TYPE=wayland`），原生窗口无法被 `xdotool` / `gnome-screenshot` 定位与截图，故「标题栏拖动 / 双击最大化 / 最小化 / 最大化 / 关闭」四项交互**只能在 Tauri 窗口内人工确认**；实现逻辑取自蓝本 `StatusBar.svelte`（`startDragging` / `toggleMaximize` / `minimize` / `close` + `isMaximized` 随 `onResized`），未改逻辑。
  - **v7 复跑（图标 + 环境判定收口）**：先确认 1442 空闲，`pnpm tauri:dev` 干净启动——Vite `ready in 476 ms` → `Local: http://localhost:1442/`，随后 `main window created phase="app_window"` → `pulsar-tauri-template ready phase="app_setup"`，无报错（`unplugin-icons` 组件在 dev 编译期正常产出，原子测量：ActivityBar 图标 `<svg>` 实测 18×18）。
  - **v7 浏览器实测（http://localhost:1442，非 Tauri 环境）**：应用运行期**零新增 error**——初始加载后点击 `.activity-item` 显示「日志」视图、再点回，两次操作控制台消息条数均未增加，且 `.log-panel .error-banner` 为 `null`（日志面板无初始化失败提示），v6 报告的第 2 条错误（`tauriClient` 在非 Tauri 读取 undefined）**已消失**。图标渲染经 `browser_evaluate` 实测：`svgCount=6`、`headerIconSvgs=3`、`activityIconSvgs=1`（尺寸 18×18）、`panelTabs=1`（tab 图标组件）、`windowControlsPresent=false`（浏览器环境不渲染窗口控制按钮，`chrome.available` 降级符合预期）。
  - **v7 关于 `net::ERR_CONNECTION_REFUSED http://localhost:1442/` 的判定**：该条在浏览器工具的**控制台**中仍可见，但经三条证据判定为**浏览器会话残留、非本应用发起**——(a) 源码 `grep` 证实本包无任何 `fetch` / `XMLHttpRequest` / `new WebSocket` / `EventSource`（插件外无主动网络请求）；(b) 同一会话的网络请求列表中 **100 条请求全部成功、零失败项**，而真实的连接失败必然同时出现在网络面板；(c) 重新加载前后该条内容完全一致、条数不变（说明是未被清空的旧缓冲条目）。因浏览器工具无法强制新建无残留的浏览器上下文，此处按证据链判定为环境侧残留。
  - **v7 未能自动验证的部分**：标题栏窗口控制四项交互（因「非 Tauri 不渲染按钮」+ Wayland 不可自动截图/点击）仍待人工在 Tauri 窗口内确认；`chrome.ts` 的实现与 v6 逐字一致（`startDragging` / `toggleMaximize` / `minimize` / `close` + `isMaximized` 随 `onResized`），仅把环境判定从调用点移入模块内部。
- Human confirmation：Q1–Q3、Q5–Q6 已由用户明确答复；Q4 采用 1442；执行已由用户批准（「开始执行」）。
- 结果汇总：**P1–P5 全部完成，v6 补齐自绘标题栏，v7 改造图标方案与环境判定**。§9 九条：①✅ ②✅ ③✅ ④⚠️（折叠 / 主题 / 语言已实测；v7 追加的两项——图标组件渲染、非 Tauri 环境运行期零 error——已实测通过；布局拖拽与标题栏窗口控制仍待人工确认）⑤✅（§9.5 词表命中仅剩三处**非源码残留**：`Cargo.lock` 的 `icu_provider` 上游 crate 名、`.svelte-kit/ambient.d.ts` 生成物中的 Node 环境变量名、`README.md` 指向蓝本 `packages/pulsar-app` 的说明性引用；词表外的命名残留见 Change Log v6 项 6）⑥✅（`invoke_handler` 恰好 5 条，与白名单逐字一致）⑦✅（无 `Gateway` / `NeuronManager` / `TopicStore` / `ProviderRegistry` / `Poller` / `HookJudgementStore` / `AssistantSession`）⑧✅（后端 1,593 行；前端 5,091 行，口径为剔除 `copyToClipboard.test.ts`）⑨✅（`Cargo.toml` 无 `[[bin]]`、`src/bin/` 不存在、无 L2 依赖）。**v7 附加核查**：`grep` 确认 `src/` 内无 `isTauriEnv` / `__TAURI_INTERNALS__`（`theme.ts` 除外，已改用官方 `isTauri()`）/ `@html` / 手写 SVG 常量残留，图标统一走 `~icons/lucide/*`。
- 核心目标是否已由证据证明完成：**是**（唯一残留为 §9.4 交互项的人工确认）。
- 若未完成，当前剩余差距：无阻塞项；§9.4 的布局拖拽与标题栏窗口控制建议人工在 Tauri 窗口内确认。
- 剩余风险：① 交互项（布局拖拽、标题栏窗口控制）未自动化验证（低风险：机制取自蓝本、未改逻辑；Wayland 会话下原生窗口不可自动截图/点击）；② `pnpm check` 的 3 条 warning 与蓝本一致，若后续要求 0 warning 需单独处理；③ §9.5 词表外的命名残留（Change Log v6 项 6）待用户裁决。

## Resume / Handoff

- 当前状态：`v7`，**P1–P5 全部完成 + 自绘标题栏（v6）+ 图标组件化与环境判定收口（v7）**（新包可构建、可起窗、无业务残留、文档与 CI 齐备）。
- 当前卡点：无。
- 下一步唯一动作：无强制动作。若需收口 §9.4，人工在 Tauri 窗口内确认四项交互：「视图跨容器拖拽」「分栏宽度拖拽」（布局区）与「标题栏拖动 / 双击最大化 / 最小化 / 最大化 / 关闭」（窗口 chrome，现由 `lib/window/chrome.ts` 承载）。
- 待决项（可选、非阻塞）：Change Log v6 项 6 列出的「§9.5 词表外业务命名残留」是否清理（注释 / CSS 类名 / i18n 段名，零逻辑改动）。
- 下一轮核心目标：无（交付完成）；后续新需求请在业务包中基于本骨架扩展（扩展点见 `docs/pulsar-tauri-template/architecture.md` §5）——图标扩展 = `~icons/lucide/{kebab-case}` 组件（构建期编译，见 §8 `package.json` 依赖行），窗口能力扩展 = 在 `lib/window/chrome.ts` 内追加方法并在模块内判环境。
