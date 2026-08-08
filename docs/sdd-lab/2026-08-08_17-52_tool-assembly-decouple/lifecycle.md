# Lifecycle / 生命周期: tool-assembly-decouple

```yaml
status: done
result: 修复完成——discover 加 15s 超时；MCP 装配异步化（启动后台 + 刷新/保存 async，移除 block_on）；Connecting 状态与 StateChange::Tools 事件；前端事件刷新 + connecting 样式；装配互斥串行化后台与运行期装配。cargo test 145 通过、check 0 errors、build 成功；GUI 端到端待用户确认。
created_at: 2026-08-08 17:52
updated_at: 2026-08-08 17:52
owner: user
```

## Current Summary / 当前摘要

- 批准状态：用户已批准（requirements + technical-plan，2026-08-08 17:52）
- 当前状态：`done`（执行完成，静态/单测验证通过；GUI 端到端待用户确认）
- 当前核心目标：修复「工具面板刷新卡死」——MCP 装配加超时 + 异步化（工具与 agent 解耦、不阻塞应用启动）+ `Connecting` 状态 + `StateChange::Tools` 事件 + 前端事件刷新

## Execution Log / 执行记录

- 1. 2026-08-08 17:52: 用户报告 Bug（刷新卡死）；对话诊断：`discover_tools` 无超时 + 刷新路径 `block_on` 阻塞（rmcp 源码核实）；用户确认核心方向「工具有独立房间、工具与 agent 可分离」；对齐 Q1–Q6 决策。创建迭代 `tool-assembly-decouple`；`requirements.md` + `technical-plan.md` 落盘（状态 `planned`）。
- 2. 2026-08-08 17:52: 用户批准文档，进入执行。
- 3. 2026-08-08 17:52: Step 1 — `core/mcp.rs`：`LIST_TIMEOUT_MS = 15_000`，`discover_tools` 包超时（内部 `list_all_tools_with_timeout` 可注入超时便于测试）；`McpServerStatusKind` 增加 `Connecting`；新增回归单测 `discover_times_out_when_server_does_not_respond`。
- 4. 2026-08-08 17:52: Step 2 — `core/events.rs` 新增 `StateChange::Tools`；`core/gateway.rs`：`build_registry` 拆分为 `assemble_local_tools`（native+config 同步）+ `assemble_mcp_progressive`（逐 server 连接/发现、渐进替换共享状态、可广播）；启动期 `with_state_emitter` 后台 spawn 装配（不阻塞启动）；`save_tool_config`/`reassemble_tools` 走 async `assemble_and_replace`（移除 `block_on`）；新增 `assemble_lock`（`tokio::sync::Mutex`）串行化后台与运行期装配。
- 5. 2026-08-08 17:52: Step 3 — 前端：`types.ts` `McpServerStatusKind` 加 `connecting`；`dataStore.svelte.ts` 加 `tools` 分支（`toolsVersion`）与 `StateChangePayload` kind；`ToolPanel.svelte` 监听 `STATE_CHANGED_EVENT`（tools）自动刷新 + connecting 样式（脉冲）。
- 6. 2026-08-08 17:52: Step 4 — 验证：`cargo test --lib` 145 passed；`cargo check --all-targets` 通过；`pnpm check` 0 errors；`pnpm build` 成功。回写 requirements AC 与 lifecycle 为 `done`。

## Next Action / 下一步唯一动作

- 用户实际运行应用，确认 GUI 端到端：启动后台装配自动出现、刷新有界返回、connecting 占位展示。
