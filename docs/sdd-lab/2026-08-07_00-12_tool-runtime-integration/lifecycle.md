# Lifecycle / 生命周期: tool-runtime-integration

```yaml
status: done
result: success
created_at: 2026-08-07 00:12
updated_at: 2026-08-07
owner: user
```

## Current Summary / 当前摘要

- 批准状态：技术方案已批准，执行完成（含运行期重装配 + 配置管理 UI 新需求）
- 当前状态：`done`（Step A–E 全部完成；`cargo test --lib` 143 通过、`cargo check --all-targets` 0 error、前端 `pnpm check` 0 error / `pnpm build` 通过）
- 当前核心目标：三通道（native / 配置驱动 / MCP）工具装配 + insert 门禁分级 + `ToolDefinition.source` 治理 + 前端 DockPane 工具面板 + 运行期保存即生效重装配 + 弹窗编辑写回 JSON

## Execution Log / 执行记录

- 1. 2026-08-07 00:12: 需求多轮讨论收敛（社区 MCP 标准对照、纯 A 语义、通道并行、insert 分级）；创建迭代 `tool-runtime-integration`；`requirements.md` 落盘；`technical-plan.md` 生成（状态 `planned`）。
- 2. 2026-08-07: Q1（前端 DockPane 面板）、Q2（独立 `dynamic_tools.json`）确认并回写三份文档。
- 3. 2026-08-07: 技术方案获批准，进入执行。
  - Step 0: 核实 rmcp 稳定版（3.1.1，client + child-process + streamable-http-client + reqwest features）与现有代码装配点。
  - Step 1: `models.rs` 新增 `ToolSource`（Native/Config/Mcp）与 `ToolDefinition.source`；`tool_registry.rs` 增加 `register_source`（Native 保留 insert 门禁，Config/Mcp 豁免）+ 测试。
  - Step 2: `tool_config.rs` 新增 `ToolConfigReader`（`mcp_servers.json` / `dynamic_tools.json`，仿 NeuronConfigReader 范式）+ 测试。
  - Step 3: `dynamic_tool.rs` 新增 `HttpTool` / `CommandTool`（命令模板复用 `cmd_exec` 安全护栏：denylist + 信号量 + 超时夹紧 + 输出截断）+ 测试。
  - Step 4: `mcp.rs` 新增 `McpServerClient`（stdio `TokioChildProcess` / streamable-http `StreamableHttpClientTransport`；启动期 connect + `tools/list` 发现 + `tools/call` 转发；失败 warn+skip）；`Cargo.toml` 增加 `http` 与测试用 dev-dependencies（axum / rmcp server / tokio io-util）。测试含 duplex 模拟 stdio 与 axum 本地 streamable-http mock server 全链路。
  - Step 5: `gateway.rs` 三通道装配（native 维持 + `dynamic_tools.json` 注册 + `assemble_mcp_servers` 收集 MCP 状态）；新增 `list_tool_info` / `mcp_server_statuses`。
  - Step 6: `lib.rs` 新增 `list_tools` / `list_mcp_servers` commands；前端新增 `ToolPanel.svelte`（Tools 面板：MCP server 状态 + 三通道工具列表），`views.ts` 注册 tab。
  - Step 7: 验证全部通过 —— `cargo test --lib` 131 passed / 0 failed；`cargo check --all-targets` 0 error；前端 `pnpm build` 通过、`pnpm check`（svelte-check）0 error。期间修复：duplex 测试 `server_handle.await` 死等（改为 abort 清理，对齐 rmcp 官方测试写法）；`tool_config` 测试固定临时目录并行踩踏（改为每测试唯一目录）；ToolPanel 非法 class 指令语法与 error-banner a11y 警告（改 button 元素）。
- 4. 2026-08-07: **需求变更（新需求确认）**：用户质疑「前端的配置管理呢？就给一个列表展示？」并澄清「什么时候说要重启，我没说过啊，运行运行期间改动并手动触发更新的啊」。确认：**运行期保存即生效（写回 JSON + 全量重装配）** + **前端配置管理 UI（列表展示不变 + 弹窗编辑）**。requirements.md 回写（Q3/Q4/Q5 确认）；technical-plan.md 更新为「纯 A + 运行期手动重装配」方案；lifecycle 状态 `done → executing`。
- 5. 2026-08-07: 进入 Step A–E 执行（进行中）：
  - Step A: registry 共享化（`Arc<RwLock>`）+ `get_tool` + `build_registry` 提取。
  - Step B: `ToolConfigReader` 写回（原子写）+ `validate_tool_config` 校验 + `ToolConfigView` 聚合视图。
  - Step C: `get_tool_config` / `save_tool_config` commands + 运行期重装配。
  - Step D: ToolPanel 弹窗编辑（MCP / HTTP / Command 三区增删改）写回 JSON。
  - Step E: `cargo test --lib` / `cargo check --all-targets` / 前端构建验证 + 回写本文件。
- 6. 2026-08-07: **Step A–E 全部完成**。
  - Step A: `ToolRegistry` 新增 `get_tool`（读锁内 clone 引用，不跨 await）；`Gateway.tool_registry` 与 `mcp_server_statuses` 改为 `Arc<RwLock<_>>`；提取 `build_registry`（native + config + mcp 全量装配，启动期与重装配共用）；`Engine` / `AssistantMode` / `NeuronManager` 消费方改为共享 registry，`execute` 走「读锁 get_tool → 释放锁 → await」。
  - Step B: `McpServerConfig` / `HttpToolConfig` / `CommandToolConfig` 等加 `Serialize` 与 skip_serializing_if；新增 `ToolConfigView` 聚合视图；`save_mcp_servers` / `save_dynamic_tools`（临时文件 + rename 原子写）；`validate_tool_config`（transport 合法 / stdio 需 command / http 需 url / name 非空且唯一 / method 枚举 / 命令模板过 denylist）。
  - Step C: `Gateway::get_tool_config`（同步读）+ `save_tool_config`（校验 → 原子写回 → `build_registry` 全量重建 → 替换共享 registry 与 statuses）；`lib.rs` 注册 `get_tool_config` / `save_tool_config` commands。
  - Step D: `types.ts` 新增 `McpServerConfig` / `HttpToolConfig` / `CommandToolConfig` / `ToolConfigView`；`ToolPanel.svelte` 保留列表展示 + 新增「编辑配置」弹窗（三区增删改，保存即生效，失败展示错误不关闭）。修复 Svelte 字面花括号插值（placeholder / label 用字符串字面量包裹）。
  - Step E: 验证全部通过 —— `cargo test --lib` 143 passed / 0 failed（新增 tool_config 写回往返 + 校验 9 条、gateway 重装配 + 拒绝保存 3 条）；`cargo check --all-targets` 0 error；前端 `pnpm check`（svelte-check）0 error、`pnpm build` 通过。
