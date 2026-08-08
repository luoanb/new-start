# Technical Plan / 技术方案: tool-assembly-decouple

## Requirement Baseline / 需求基线

- 对应需求文档：`docs/sdd-lab/2026-08-08_17-52_tool-assembly-decouple/requirements.md`
- 需求确认状态：已确认（Q1–Q6 全部确认，2026-08-08）
- 本方案覆盖范围：MCP 装配超时 + 异步化（工具与 agent 解耦）+ `Connecting` 状态 + `StateChange::Tools` 事件 + 前端事件刷新

## Current Project Facts / 当前项目事实

- `core/gateway.rs`：
  - `with_state_emitter`（L70）启动期同步调用 `build_registry`（L108），MCP 连接内嵌 `block_on`——MCP 挂起会阻塞应用启动。
  - `reassemble_tools`（L341）/ `save_tool_config`（L313）运行期同样 `build_registry`；`reassemble_tools` 由 async command（`lib.rs` L157）在 tauri worker 线程调用，`block_on` 内嵌存在阻塞/死锁条件。
  - `build_registry`（L800）：native + config 同步注册 + `assemble_mcp_servers`（L828，逐个 `block_on(connect)` / `block_on(discover_tools)`，失败 warn+skip）。
  - `replace_tools`（L349）：原子替换 `tool_registry` / `mcp_server_statuses` 两个 `Arc<RwLock<_>>`。
- `core/mcp.rs`：
  - `connect`（L83）→ `serve_with_timeout`（L271，`CONNECT_TIMEOUT_MS = 15_000`）有超时。
  - `discover_tools`（L122）→ `peer.list_all_tools().await` **无超时**（rmcp 3.1.1 源码核实：`client.rs` L1618 `list_all_tools`、L1559 `list_tools` 走 `send_request`，无内置请求超时）。
  - `McpServerStatusKind`（L43）= Connected / Failed / Disabled，无 Connecting。
- `core/events.rs`：`StateChange` kind = topics / conversations / poller / sessions / neurons，无 tools。
- 前端 `stores/dataStore.svelte.ts`：`handleStateChanged` 分支（topics/conversations/poller/sessions/neurons），无 tools；`ToolPanel.svelte`（L34）用本地 `$state` + onMount `refresh()`，**未接入事件**。
- agent 侧对 registry 实时读、零缓存：`engine.rs` L184 `agent_mode` 每次取 `list_definitions()`；`neuron_manager.rs` L75 `available_tools_block` 每次读。→ 装配方式/时机对 agent 完全透明。

## Solution Options / 方案候选

| 决策点 | 候选 | 选定 | 原因 |
|---|---|---|---|
| discover 超时 | 不超时（现状）/ 加超时 | **加超时（`LIST_TIMEOUT_MS = 15_000`，与 connect 对齐）** | 无超时是刷新卡死的直接原因；15s 与 connect 一致，语义统一 |
| 装配执行方式 | A 只加超时（仍 `block_on`）/ B 异步装配 | **B 异步装配** | 用户明确「工具有独立房间、不阻塞应用启动」；B 同时消除启动阻塞与刷新 worker 阻塞 |
| 启动装配 | 同步（现状）/ 后台 spawn | **后台 spawn（本地同步 + MCP 异步）** | 本地工具（native+config）毫秒级、启动即可用；MCP 后台连完自动登记 |
| 刷新/保存装配 | `block_on`（现状）/ async await | **async await（无 `block_on`）** | 移除 worker 线程内嵌阻塞，根除死锁条件 |
| 事件通知 | 无 / `StateChange::Tools` | **新增 `StateChange::Tools`** | 启动后台装配完成/进度变化需推给前端，面板自动刷新 |
| 装配期间状态 | 无 / Connecting | **`McpServerStatusKind::Connecting`** | 用户确认需要「连接中」占位 |
| 刷新返回时机 | 提交即返回 / 等装配结束 | **等装配结束（有界 ≤15s）** | 用户确认；前端「刷新中」有上界，简单一致 |

## Decision / 方案决策

- Selected：`discover_tools` 加 15s 超时；MCP 装配异步化——启动期后台 spawn（本地工具同步就绪，MCP 后台连接，完成后原子替换 + emit `Tools`），刷新/保存走 async await（移除 `block_on`）；新增 `Connecting` 状态与 `StateChange::Tools` 事件；刷新命令保持「等装配结束再返回」。
- Why：用户实际观察「启动正常、刷新卡死」指向装配执行环境差异（启动=同步上下文独立 runtime，刷新=async worker 内嵌 `block_on`）；「工具与 agent 分离」要求装配不阻塞 agent 主流程；超时兜底保证任何 MCP 不响应都有界。
- Decision Owner：用户（已确认）
- Decision Time：2026-08-08 17:52

## Open Questions / 开放问题

- 无（Q1–Q6 已在需求文档确认）。执行中若发现方案与代码现实冲突，先回写本方案再改代码。

## API Design / API 设计

### Contract Scope

- 变更类型：修改（装配流程）+ 扩展（超时、状态、事件）。
- 消费方：gateway（装配）、前端 dataStore / ToolPanel（事件 + 状态展示）；engine / assistant / neuron_manager 零改动（registry 只读接口不变）。
- 真相源文件：`core/mcp.rs`、`core/gateway.rs`、`core/events.rs`、`lib.rs`、`src/lib/stores/dataStore.svelte.ts`、`src/lib/components/ToolPanel.svelte`、`src/lib/types.ts`。

### core/mcp.rs（扩展）

```rust
/// 单 server 装配期 tools/list 发现超时（与 CONNECT_TIMEOUT_MS 对齐）。
const LIST_TIMEOUT_MS: u64 = 15_000;

/// McpServerStatusKind 增加 Connecting（装配进行中）。
pub enum McpServerStatusKind { Connected, Failed, Disabled, Connecting }

// discover_tools：对 peer.list_all_tools() 包 tokio::time::timeout，
// 超时返回可读错误 → 装配方 warn + skip + Failed 状态。
pub async fn discover_tools(self: &Arc<Self>) -> AppResult<Vec<McpTool>> {
    let tools = tokio::time::timeout(
        Duration::from_millis(LIST_TIMEOUT_MS),
        self.peer.list_all_tools(),
    )
    .await
    .map_err(|_elapsed| AppError::RuntimeError(format!(
        "mcp[{}]: tools/list 发现超时（{}ms）", self.name, LIST_TIMEOUT_MS
    )))?
    .map_err(|e| AppError::RuntimeError(format!("mcp[{}]: tools/list 失败: {e}", self.name)))?;
    // ... 组装 McpTool 不变
}
```

### core/events.rs（扩展）

```rust
pub enum StateChange {
    Topics,
    Conversations,
    Poller(PollerStatus),   // 现有
    Sessions,
    Neurons,
    Tools,                  // 新增：工具装配进度/结果变化
}
```

### core/gateway.rs（修改：装配拆分为「本地同步 + MCP 异步」）

- 拆分 `build_registry`：
  - `fn assemble_local(storage_root) -> ToolRegistry`：native（`ExecuteCommandTool`）+ 配置驱动（`dynamic_tools.json` → HttpTool/CommandTool），同步、无网络。
  - `async fn assemble_mcp_async(registry: &ToolRegistry, storage_root) -> AppResult<(ToolRegistry, Vec<McpServerStatus>)>`：逐个 server `connect().await` / `discover_tools().await`（各自超时，失败 warn+skip + Failed），成功则将工具 `register_source(.., Mcp)` 并入新 registry；**无 `block_on`**。
- 启动路径 `with_state_emitter`：
  1. `let mut registry = assemble_local(root)`；
  2. 构造 Gateway（registry 就绪，agent 立即可用本地工具）；
  3. 若配置含启用 MCP server：先写入初始 statuses（全部 `Connecting`）+ `emit(StateChange::Tools)`；
  4. `tauri::async_runtime::spawn` 后台执行 `assemble_mcp_async`，每 server 完成即原子替换 `statuses`（+该 server 工具并入 registry）并 `emit(StateChange::Tools)`；全部完成后最终替换并 emit。
  - 注：也可简化为「开始 emit 一次 Connecting + 结束 emit 一次最终结果」；实现取「每 server 完成增量 emit」以支撑 server 逐个出现的展示语义（Q6）。
- 运行期 `reassemble_tools` / `save_tool_config`：
  - 直接 `let mut registry = assemble_local(root);` 后 `await assemble_mcp_async(...)` → `replace_tools` → emit；**去掉 `tauri::async_runtime::block_on`**。
- `list_tool_info` / `mcp_server_statuses` 读取不变（读锁 clone）。

### 前端

- `src/lib/types.ts`：`McpServerStatus["status"]` 增加 `"connecting"`。
- `src/lib/stores/dataStore.svelte.ts`：`handleStateChanged` 增加 `tools` 分支 → `refreshTools()`（`invoke("list_tools")` + `invoke("list_mcp_servers")`）。
- `src/lib/components/ToolPanel.svelte`：
  - 订阅 `app://state-changed`（tools kind）→ `refresh(true)`；启动 onMount 的初始加载保持。
  - MCP server 状态增加 connecting 样式（与 connected/failed 并列的视觉态）。

## Execution Steps / 执行步骤

- Step 1：`core/mcp.rs` — `LIST_TIMEOUT_MS` + `discover_tools` 超时 + `McpServerStatusKind::Connecting`；补超时单测（不响应 server → 超时失败）。
- Step 2：`core/events.rs` 新增 `Tools`；`core/gateway.rs` 拆分 `assemble_local` / `assemble_mcp_async`；启动后台 spawn + 增量 emit；`reassemble_tools` / `save_tool_config` 改 async await；`lib.rs` 命令签名适配；补装配单测。
- Step 3：前端 `types.ts` + `dataStore` tools 分支 + `ToolPanel` 事件订阅与 connecting 样式。
- Step 4：验证 — `cargo test --lib` / `cargo check --all-targets`；前端 `pnpm check` / `pnpm build`；回写 lifecycle。

## Risks / 风险

- 启动时 MCP 工具暂缺：面板先显示 connecting，连完自动出现（可接受，Q6 已确认）。
- 增量 emit 的事件量：每个 server 完成 emit 一次，数据量小（server 数级），可接受；若担心可退化为「开始 + 结束」两次 emit。
- 后台 spawn 装配与运行期手动重装配并发：运行期重装配应取消/忽略进行中的后台装配结果（以最后一次为准）——实现时用装配代次（generation）或互斥（`Mutex` 串行化装配）保证。
