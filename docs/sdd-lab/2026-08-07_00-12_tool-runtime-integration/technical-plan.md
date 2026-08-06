# Technical Plan / 技术方案: tool-runtime-integration

## Requirement Baseline / 需求基线

- 对应需求文档：`docs/sdd-lab/2026-08-07_00-12_tool-runtime-integration/requirements.md`
- 需求确认状态：已确认（多轮讨论收敛，Q1–Q5 全部确认）
- 本方案覆盖范围：三通道工具装配（native / config / mcp，已完成）+ **运行期手动重装配**（保存即生效）+ **前端配置管理 UI（列表展示 + 弹窗编辑写回 JSON）**

## Current Project Facts / 当前项目事实

- `core/tool_registry.rs`：`Tool` trait；`ToolRegistry = HashMap<String, ToolBox>`（`Arc<dyn Tool>` + `source`），derive Clone；`register`（native 保留 insert 门禁）/ `register_source`（config/mcp 豁免）；`list_definitions()` / `definitions_for()` / `execute(name, args)`；内置 dead_code 工具未注册。**值类型、未共享化**。
- `core/gateway.rs`：字段 `tool_registry: Option<ToolRegistry>`；启动期三通道装配（`ExecuteCommandTool` + `dynamic_tools.json` 注册 + `assemble_mcp_servers`）；`list_tool_info()` / `mcp_server_statuses()`（装配期只读 `Vec<McpServerStatus>`）；registry clone 传给 NeuronManager 与 Engine。
- `core/engine.rs`：`tool_registry: Option<ToolRegistry>`；agent_mode 取全量 `list_definitions()`；`tool_calls` → `reg.execute(...).await`。
- `core/assistant_mode.rs` / `core/neuron_manager.rs`：`tool_registry: ToolRegistry` 字段（clone 持有）。
- `core/tool_config.rs`：`McpServerConfig` / `HttpToolConfig` / `CommandToolConfig` / `McpServersFile` / `DynamicToolsFile` / `ToolConfigReader`（仅 Deserialize + 读取，缺省空、非法报错）。**无 Serialize、无写回、无校验函数**。
- `core/dynamic_tool.rs`：`HttpTool::from_config` / `CommandTool::from_config`；`extract_placeholders` / `build_params_schema` / `render_template` / `render_url`；命令模板复用 cmd_exec 护栏（denylist / 超时 / 并发 / 截断 / 脱敏）。
- `core/mcp.rs`：`McpServerClient`（rmcp 3.1.1，stdio `TokioChildProcess` / streamable-http）；`discover_tools` → `McpTool`；连接失败 warn + skip。
- `lib.rs`：tauri commands 已有 `list_tools` / `list_mcp_servers`（均 async fn、内部同步调用 gateway）。
- 前端：Svelte + TypeScript；`ToolPanel.svelte`（只读列表：MCP server 状态 + 工具分组，无编辑）；`types.ts` 有 `ToolInfo` / `ToolSource` / `McpServerStatus`。
- 配置根目录：存储根（`.agent-app/`），`mcp_servers.json` + `dynamic_tools.json` 并列。

## Solution Options / 方案候选

> 关键决策均已在多轮讨论中由用户确认，此处记录选项与选定项，供追溯。

| 决策点 | 候选 | 选定 | 原因 |
|---|---|---|---|
| MCP 客户端 | rmcp 稳定版 / 自实现 JSON-RPC | **rmcp 稳定版（3.1.1）** | 官方 SDK 活跃维护、两种 transport 现成、锁稳定版隔离 API 变动 |
| 运行阶段语义 | 纯 A 启动期装配 / 会话中热增 / **纯 A + 运行期手动重装配** | **纯 A + 运行期手动重装配** | 用户澄清「运行期间改动并手动触发更新的」；保存即生效、全量重建、无需重启；无热监听复杂度 |
| 重装配范围 | 部分重建 / **全量重建** | **全量重建** | 用户确认；native + config + mcp 统一重建，MCP 连接按新配置重连，语义简单一致 |
| 配置管理 UI 形态 | 列表内联编辑 / **列表展示不变 + 弹窗编辑** | **列表展示不变 + 弹窗编辑** | 用户确认；只读列表保持治理视图，编辑收敛在弹窗，写回 JSON |
| 配置驱动 DynamicTool | 全量保留 / HTTP-only / 砍掉 | **全量保留** | 命令模板 + HTTP 都保留，但命令模板复用 execute_command 护栏为硬约束 |
| insert 门禁 | MCP 豁免 + 项目自有保留 / 动态全豁免 / 运行时文件 | **动态通道豁免**（config/mcp 豁免，native 保留） | 动态工具自描述（schema 即契约）；native 工具与「No Spec, No Code」治理同构 |
| MCP 配置位置 | 独立 mcp_servers.json / 并入 config.json | **独立 mcp_servers.json** | 对齐 claude_desktop_config.json 惯例，迁移零成本 |
| registry 共享方式 | Arc<tokio::RwLock> / **Arc<std::sync::RwLock>** | **Arc<std::sync::RwLock>** | 读操作均为短临界区（clone 结果/工具后即释放锁，不跨 await）；写操作一次性替换（build_registry 无 await）；同步读在 tauri command 中免 block_on |

## Decision / 方案决策

- Selected：rmcp 稳定版；**纯 A + 运行期手动重装配（保存即生效、全量重建）**；配置驱动全量保留；insert 门禁动态通道豁免 + native 保留；独立 `mcp_servers.json` + `dynamic_tools.json`；范围含前端面板（列表 + 弹窗编辑）。
- Why：对齐社区 MCP 事实标准；运行期改动通过 UI 保存显式触发（无热监听复杂度）；`ToolDefinition.source` 支撑审计与前端分组；registry 共享化（`Arc<RwLock>`）使运行期重装配成为可能。
- Decision Owner：用户（已确认）
- Decision Time：2026-08-07 00:12（原始）；2026-08-07（追加：运行期重装配 + 配置管理 UI）

## Open Questions / 开放问题

- [x] Q1 前端工具面板形态：**DockPane 面板**（已确认 2026-08-07）——可折叠浮动面板。
- [x] Q2 配置驱动工具配置文件：**独立 `dynamic_tools.json`**（已确认 2026-08-07）——与 `mcp_servers.json` 并列，均在存储根目录。
- [x] Q3 运行期重装配触发：**保存即生效**（已确认 2026-08-07）——UI 保存写回 JSON 并立即触发重装配，无需重启。
- [x] Q4 重装配范围：**全量重建**（已确认 2026-08-07）——native + config + mcp 统一重建。
- [x] Q5 配置管理 UI 形态：**列表展示不变 + 弹窗编辑**（已确认 2026-08-07）。

## API Design / API 设计

### Contract Scope

- 变更类型：扩展（`ToolConfigReader` 读写 + 校验、registry 共享化、gateway 重装配、commands、ToolPanel 弹窗）+ 已完成（三通道装配、`ToolDefinition.source`、`ToolRegistry.register_source`、list commands、只读列表）。
- 消费方：engine agent_mode（`list_definitions` / `execute`）、assistant mode、neuron_manager、前端工具面板（列表 + 弹窗编辑）。
- 真相源文件：`core/models.rs`、`core/tool_registry.rs`、`core/tool_config.rs`、`core/gateway.rs`、`lib.rs`、`src/lib/types.ts`。

### ToolRegistry（扩展：共享化 + get_tool）

- `ToolRegistry` 自身保持 `&self` 同步 API（`list_definitions` / `definitions_for` / `execute` 不变）。
- 新增 `get_tool(&self, name) -> Option<Arc<dyn Tool>>`：供调用方在读锁保护下 clone 工具引用、释放锁后再 await `execute`，**读锁不跨 await**。
- 所有持有方统一改为 `Arc<RwLock<ToolRegistry>>`（`std::sync::RwLock`）：

```rust
pub struct Gateway {
    tool_registry: Arc<RwLock<ToolRegistry>>,      // 原 Option<ToolRegistry>
    mcp_server_statuses: Arc<RwLock<Vec<McpServerStatus>>>, // 原 Vec<McpServerStatus>
    ...
}
```

- 消费方同步改造：`Engine.tool_registry` / `AssistantMode.tool_registry` / `NeuronManager.tool_registry` 均改为 `Arc<RwLock<ToolRegistry>>`。
- 读取范式：`list_definitions()` / `get_tool()` 在 `read()` 守卫内 clone 出结果/引用后立即释放，再进入任何 await。

### core/tool_config.rs（扩展：写回 + 校验）

- `McpServerConfig` / `HttpToolConfig` / `CommandToolConfig` / `McpServersFile` / `DynamicToolsFile` 增加 `Serialize`（写回 JSON，`serde_json::to_string_pretty`）。
- 新增写回：`save_mcp_servers(&self, &McpServersFile)` / `save_dynamic_tools(&self, &DynamicToolsFile)`——原子写（临时文件 + rename），文件缺失时创建。
- 新增校验 `validate_tool_config(&ToolConfigView) -> AppResult<()>`（保存前校验，失败拒绝保存不触发重装配）：
  - MCP server：`name` 非空且不重复；`transport ∈ {"stdio", "http"}`；stdio 必须 `command` 非空；http 必须 `url` 非空。
  - HTTP 工具：`name` 非空且不重复；`url` 非空；method 归一化后 ∈ {GET, POST, PUT, DELETE}。
  - Command 工具：`name` 非空且不重复；`template` 非空；模板过 `cmd_exec::is_denied`（装配期即拒绝危险模板，execute 时再兜底参数注入）。

### ToolConfigView（前后端共享配置视图）

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfigView {
    pub mcp_servers: Vec<McpServerConfig>,
    pub http_tools: Vec<HttpToolConfig>,
    pub command_tools: Vec<CommandToolConfig>,
}
```

- 由 `ToolConfigReader` 聚合读取（`mcp_servers.json` + `dynamic_tools.json`），是弹窗编辑与写回的单一数据形状。

### Gateway（扩展：装配提取 + 运行期重装配）

```rust
/// 全量装配（无 await；MCP 连接用 block_on）。启动期与重装配共用。
pub fn build_registry(storage_root: &Path) -> AppResult<(ToolRegistry, Vec<McpServerStatus>)>;

impl Gateway {
    /// 读取当前配置（供弹窗编辑）。
    pub fn get_tool_config(&self) -> AppResult<ToolConfigView>;
    /// 保存配置：校验 → 原子写回两个 JSON → build_registry 全量重建 → 原子替换 registry 与 statuses。
    pub async fn save_tool_config(&self, view: ToolConfigView) -> AppResult<ToolConfigView>;
}
```

- `save_tool_config` 顺序：① `validate_tool_config`（失败返回可读错误，不写文件不触发）→ ② 写回 `mcp_servers.json` / `dynamic_tools.json` → ③ `build_registry(root)`（从新文件装配）→ ④ `*reg.write() = new_registry; *status.write() = new_statuses` → ⑤ 返回 `get_tool_config()`。
- 重装配期间正在执行的旧工具调用不中断：执行路径已 clone `Arc<dyn Tool>`，替换 registry 不影响在飞调用。

### Tauri commands（lib.rs 扩展）

- `list_tools() -> Vec<ToolInfo>`（已有）：改为读锁 clone。
- `list_mcp_servers() -> Vec<McpServerStatus>`（已有）：改为读锁 clone。
- `get_tool_config() -> ToolConfigView`（新增）：同步读取当前配置。
- `save_tool_config(view: ToolConfigView) -> ToolConfigView`（新增）：`gateway.save_tool_config(view).await`；校验失败返回 `TauriResult` 错误（前端展示具体错误）。

### 前端

- `src/lib/types.ts` 扩展：`McpServerConfig` / `HttpToolConfig` / `CommandToolConfig` / `ToolConfigView`（字段与后端 serde 一致）。
- `src/lib/components/ToolPanel.svelte`：
  - 列表展示不变（来源分组 + MCP server 状态 + 空态）。
  - 新增「编辑配置」按钮 → 弹窗（modal）编辑器：MCP Servers / HTTP Tools / Command Tools 三个分区；每条可编辑字段、可删除；底部「添加」新增条目；「保存」→ `invoke("save_tool_config", { view })` → 成功后关闭弹窗并 `refresh()`；失败在弹窗内展示错误，不关闭。
  - 弹窗实现：内联于 ToolPanel（Svelte `{#if}` + 覆盖层），或拆 `ToolConfigEditor.svelte`（执行时按体量取舍，倾向独立组件便于测试）。

## Exec Scheme Bridge / 方案桥接

### 1. 改动依赖范围内的能力与代码现实

| 能力 | 现状 | 证据 |
|---|---|---|
| ToolSource / ToolDefinition.source | 已完成 | `core/models.rs` |
| register_source（config/mcp 豁免 insert） | 已完成 | `core/tool_registry.rs` |
| 三通道装配（native/config/mcp） | 已完成（仅启动期） | `core/gateway.rs` 装配处 |
| ToolConfigReader 读取 | 已完成（仅读） | `core/tool_config.rs` |
| HttpTool / CommandTool | 已完成 | `core/dynamic_tool.rs` |
| McpServerClient | 已完成（装配期连接） | `core/mcp.rs` |
| registry 共享化 | **需改**（值类型 → `Arc<RwLock>`） | `tool_registry.rs`、`gateway.rs`、`engine.rs`、`assistant_mode.rs`、`neuron_manager.rs` |
| tool_config 写回 + 校验 | **需增** | `core/tool_config.rs` |
| 运行期重装配命令 | **需增**（`build_registry` / `save_tool_config` / `get_tool_config`） | `core/gateway.rs`、`lib.rs` |
| 前端弹窗编辑 | **需增**（现 ToolPanel 只读） | `src/lib/components/ToolPanel.svelte` |
| 前端配置类型 | **需增**（现无 `ToolConfigView`） | `src/lib/types.ts` |
| rmcp | 已锁 3.1.1 | `Cargo.toml` |

### 2. 外部依赖：包与本任务用到的精确 API

| 包（版本来源） | 本任务依赖的具体 API | 备注 |
|---|---|---|
| rmcp（crates.io，锁 3.1.1） | `ClientHandler` + `serve(transport)` → `RunningService` + `peer().list_all_tools()` / `call_tool_once`；`TokioChildProcess`（stdio）；`StreamableHttpClientTransport`（http） | 已落地 `core/mcp.rs`，不新增依赖 |
| serde / serde_json（已有） | `Serialize` / `Deserialize`（配置写回）；`to_string_pretty` | 已有 |
| std::sync::RwLock（标准库） | 共享 registry / statuses；写锁一次性替换 | 无新依赖 |
| tokio（已有 1.53.1） | `async_runtime::block_on`（build_registry 内 MCP 连接） | 已有 |

### 3. 设计契约

**技术文档出处**：本迭代 `requirements.md`（Q1–Q5 已确认）+ 多轮讨论收敛结论

**契约正文**：

```rust
pub struct Gateway {
    tool_registry: Arc<RwLock<ToolRegistry>>,
    mcp_server_statuses: Arc<RwLock<Vec<McpServerStatus>>>,
}

impl ToolRegistry {
    pub fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>>; // 读锁内 clone，不跨 await
}

pub struct ToolConfigView {
    pub mcp_servers: Vec<McpServerConfig>,
    pub http_tools: Vec<HttpToolConfig>,
    pub command_tools: Vec<CommandToolConfig>,
}

impl ToolConfigReader {
    pub fn save_mcp_servers(&self, file: &McpServersFile) -> AppResult<()>;   // 原子写回
    pub fn save_dynamic_tools(&self, file: &DynamicToolsFile) -> AppResult<()>; // 原子写回
}

pub fn validate_tool_config(view: &ToolConfigView) -> AppResult<()>; // 非法即拒绝保存
pub fn build_registry(storage_root: &Path) -> AppResult<(ToolRegistry, Vec<McpServerStatus>)>; // 启动期 + 重装配共用

// Tauri commands
async fn save_tool_config(view: ToolConfigView) -> TauriResult<ToolConfigView>; // 校验→写回→全量重装配→原子替换
async fn get_tool_config() -> TauriResult<ToolConfigView>;                      // 弹窗编辑源
```

**相对技术文档的增量说明**：

| 项目 | 说明 |
|---|---|
| 沿用 | rmcp 稳定版；insert 分级（动态豁免 / native 保留）；独立 `mcp_servers.json` + `dynamic_tools.json`；三通道并行装配 |
| 改写 | 运行阶段语义由「纯 A 只读面板」改为「纯 A + 运行期手动重装配（保存即生效、全量重建）」；registry 由值类型共享为 `Arc<RwLock>`；`mcp_server_statuses` 装配期只读 → 重装配后可更新 |
| 新增 | `ToolConfigView`（前后端共享配置形状）；`ToolConfigReader` 写回 + `validate_tool_config` 校验；`build_registry` 提取复用；`get_tool_config` / `save_tool_config` commands；ToolPanel 弹窗编辑 |

## Execution Steps / 执行步骤

> 已执行的 Step 0–6（三通道装配 + 只读面板）记录在 lifecycle.md。以下为**运行期重装配 + 配置管理 UI** 的新增步骤。按「No Approval, No Execute」，本方案获批后执行。

### Step A. registry 共享化 + `build_registry` 提取

- `core/tool_registry.rs`：新增 `get_tool(name) -> Option<Arc<dyn Tool>>`；`execute` 保持 `&self` 兼容。
- `core/gateway.rs`：`tool_registry: Option<ToolRegistry>` → `Arc<RwLock<ToolRegistry>>`；`mcp_server_statuses: Vec<McpServerStatus>` → `Arc<RwLock<Vec<McpServerStatus>>>`；提取 `build_registry(storage_root)`（native + config + mcp，MCP 用 `block_on`），启动期与重装配共用。
- 消费方改造：`engine.rs` / `assistant_mode.rs` / `neuron_manager.rs` 字段与调用点改为 `Arc<RwLock<ToolRegistry>>`；`execute` 调用改为「读锁 `get_tool` clone → 释放锁 → `tool.execute(...).await`」；`list_definitions` 改为「读锁内 clone definitions → 释放锁」。
- 验收：`cargo test --lib` 全绿（既有 gateway / engine / registry 测试随改造通过）。

### Step B. tool_config 写回 + 校验

- `core/tool_config.rs`：各配置结构加 `Serialize`；新增 `save_mcp_servers` / `save_dynamic_tools`（原子写：临时文件 + rename）；新增 `ToolConfigView` 与聚合读取 `view()`；新增 `validate_tool_config`。
- 验收：单测覆盖写回往返（save → read 一致）、缺省/非法文件、校验各分支（transport 非法 / stdio 缺 command / http 缺 url / name 空 / 重名 / 命令模板过 denylist）。

### Step C. Gateway 重装配命令

- `core/gateway.rs`：`get_tool_config()`（同步读）；`save_tool_config(view)`（校验 → 写回 → `build_registry` → 原子替换 registry + statuses）。
- `lib.rs`：注册 `get_tool_config` / `save_tool_config` commands。
- 验收：单测——保存合法配置后 registry 更新、statuses 更新；非法配置被拒（文件内容不变）；重装配后 `list_tool_info` 反映新工具集。

### Step D. 前端弹窗编辑

- `src/lib/types.ts`：新增 `McpServerConfig` / `HttpToolConfig` / `CommandToolConfig` / `ToolConfigView`。
- `ToolPanel.svelte`：保留列表展示；新增「编辑配置」按钮 + 弹窗编辑器（MCP / HTTP / Command 三区，编辑 / 删除 / 新增），「保存」→ `invoke("save_tool_config")` → 成功后 `refresh()`；失败展示错误。
- 验收：`pnpm check`（svelte-check）0 error；`vite build` 通过。

### Step E. 检查与回写

- 命令：`cargo test --lib`；`cargo check --all-targets`；`pnpm --filter agent-app build`。
- 回写 `lifecycle.md`（status → done / result，追加 Step A–E 记录）。

## Risk And Mitigation / 风险与缓解

| 风险 | 缓解 |
|---|---|
| 共享化改造波及 engine / assistant / neuron_manager 调用点 | 变更集中在读取范式（读锁 clone 后释放）；`get_tool` 收敛执行路径；逐调用点 grep 核对 |
| 读锁跨 await（死锁/长阻塞） | 硬约束：读锁内仅 clone，不 await；`execute` 前释放锁 |
| 保存与重装配非原子（写文件成功、装配失败） | 先校验（拒绝非法）→ 写回 → 装配 → 替换；装配失败返回错误但文件已写（下次保存可修正），registry 保持旧引用（在飞调用不中断） |
| stdio MCP server = 本地代码执行（npx） | 显式配置才启用；启动日志提示来源；v2 用户确认 |
| 命令模板后端注入面 | 复用 cmd_exec denylist / 超时 / 并发 / 截断 / 脱敏；保存时模板过 denylist 拒绝 |
| 弹窗编辑复杂度 | 配置形状收敛为 `ToolConfigView` 单一视图；编辑器分区组件化；非法输入由后端校验兜底 |

## Execute Checkpoint / 执行检查点

- 当前理解：三通道装配已完成；新增**运行期手动重装配（保存即生效、全量重建）** + **前端配置管理 UI（列表 + 弹窗编辑写回 JSON）**。
- 核心目标：registry 共享化（`Arc<RwLock>`）→ `ToolConfigReader` 写回 + 校验 → `build_registry` 复用 → `save_tool_config` / `get_tool_config` commands → ToolPanel 弹窗编辑。
- 下一步动作：本方案获批后进入 Step A。
- 风险：共享化波及调用点、读锁跨 await、保存/装配原子性、弹窗编辑复杂度。
