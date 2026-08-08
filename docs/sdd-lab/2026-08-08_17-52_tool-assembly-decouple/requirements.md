# Requirements / 需求文档: tool-assembly-decouple

## Restated Understanding / 需求复述

- 用户报告 Bug：工具面板点击刷新后一直卡在「刷新中」；实际观察是应用能正常启动（说明启动期 MCP 装配正常），**未改任何配置**，点击刷新即卡死。
- 诊断结论（对话已确认）：根因是 `discover_tools`（MCP `tools/list` 发现）**无超时保护**；且刷新时 MCP 装配通过 `tauri::async_runtime::block_on` 在 async command 的 worker 线程上同步执行，存在阻塞/死锁条件，命令永不返回 → 前端 `refreshing` 永不复位。
- 核心需求：**工具系统与 agent 解耦**——工具（尤其 MCP）的装配有自己的「独立房间」，初始化成功与否不阻塞应用启动；MCP 装配必须有超时；前端能感知装配进度（事件通知 + 连接中状态）。
- 当前边界：本次只解耦装配流程与超时/事件/状态；热监听自动重扫、MCP 健康检查/自动重连不在范围。

## Scope / 范围

- In:
  - MCP `discover_tools` 增加超时（与 `connect` 的 15s 对齐）。
  - MCP 装配异步化：启动期后台装配（不阻塞应用启动）；刷新/保存走真正的 async 装配（移除 `block_on`）。
  - 本地工具（native + config）保持同步装配、启动即可用。
  - `McpServerStatusKind` 新增 `Connecting`（连接中），前端展示占位状态。
  - 新增 `StateChange::Tools` 事件；前端 dataStore 增加 `tools` 分支，ToolPanel 接入事件自动刷新。
  - 单元测试、`cargo check`、前端构建验证与文档回写。
- Out:
  - 运行期热监听自动重扫（file watcher，v2）。
  - MCP 连接健康检查 / 断线自动重连。
  - agent 与工具的更深度解耦（本次保持 registry 只读接口不变，仅解耦装配流程）。
  - 前端「刷新中」体验重构（本次维持按钮转圈 + 有界等待）。

## User Interaction / 用户交互

- 触发入口：工具面板刷新按钮；启动应用；工具配置弹窗「保存」。
- 用户操作路径（启动）：打开应用 → 界面立即出现 → 本地工具立即可用 → MCP server 显示「连接中」→ 连接完成后该 server 状态变 connected / failed，其工具自动出现在列表。
- 用户操作路径（刷新）：点击刷新 → 按钮短暂转圈（有上界，≤ 15s）→ MCP 装配结束一次返回 → 列表与状态刷新；失败 server 标注 failed。
- 系统反馈：MCP server 状态（connecting / connected / failed / disabled）；装配过程与结果通过 `app://state-changed`（Tools kind）推给前端，工具面板自动更新。
- 异常/边界交互：MCP server 连接或 discover 超时 → 标注 failed 并展示错误，该 server 工具不出现，其余装配正常；配置非法 → 保存被拒并提示（沿用现状）。
- 不应发生的交互：应用启动被 MCP 装配阻塞；刷新按钮无限转圈。

## Acceptance Criteria / 验收标准

- [x] MCP server 不响应 `tools/list` 时，刷新在 ~15s 内返回，server 标记 failed，**不再永久卡死**。（`discover_tools` 加 15s 超时；回归单测 `discover_times_out_when_server_does_not_respond` 通过）
- [x] 启动时 MCP 装配在后台执行，应用启动不被阻塞；MCP 工具连接完成后自动出现在面板（无需手动刷新）。（`with_state_emitter` 本地同步 + MCP 后台 spawn + `StateChange::Tools` 事件驱动）
- [x] MCP server 状态支持 connecting，并随装配进度更新为 connected / failed。（`McpServerStatusKind::Connecting`；ToolPanel connecting 样式）
- [x] 装配状态变化通过 `app://state-changed`（Tools kind）通知前端，工具面板自动刷新。（`StateChange::Tools` + dataStore `tools` 分支 + ToolPanel 监听）
- [x] `reassemble_tools` / `save_tool_config` 无 `block_on`，走真正的 async 装配。（`assemble_and_replace` + `assemble_mcp_progressive`，无 `block_on`）
- [x] 既有行为不回归：本地工具启动即可用；保存即生效；insert 门禁分级不变；MCP 单 server 失败不阻塞整体。（`cargo test --lib` 145 项全过）
- [x] `cargo test --lib` 全绿；`cargo check --all-targets` 通过；前端 `pnpm check` / build 通过。（145 passed；check 0 errors；build 成功）

> 验证说明：静态审查与单测已全部通过；GUI 端到端（启动后台自动出现、刷新有界返回、connecting 占位）需用户实际运行应用确认。

## Constraints / 约束

- 技术约束：rmcp 维持 3.1.1；registry 共享方式不变（`Arc<std::sync::RwLock>`，读锁不跨 await）；超时常量与 `connect` 对齐（15s）；日志沿用现有 `phase` 标记。
- 业务约束：agent 模式工具集语义不变（registry 实时读、零缓存）；neuron `tool_ids` 白名单授权边界不变。
- 兼容性约束：不引入破坏性变更；`ToolPanel` 只读列表展示结构不变，仅增加连接中状态与事件刷新。

## Open Questions / 开放问题

- [x] Q1 刷新卡死根因：**`discover_tools` 无超时 + 刷新路径 `block_on` 阻塞**（已确认，对话诊断 + rmcp 源码核实）。
- [x] Q2 修复粒度：**超时 + 异步装配**（已确认 2026-08-08）——工具应有独立房间，初始化不阻塞应用启动，工具与 agent 可分离。
- [x] Q3 事件通知：**需要**（已确认 2026-08-08）——新增 `StateChange::Tools`，前端监听刷新。
- [x] Q4 MCP 装配期间状态：**需要「连接中」占位**（已确认 2026-08-08）——`McpServerStatusKind::Connecting`。
- [x] Q5 刷新命令返回时机：**保持「等装配结束再返回」**（已确认 2026-08-08）——有界等待（≤15s），不改「提交即返回」。
- [x] Q6 工具出现的粒度：**以 server 为批次**（已确认 2026-08-08）——单个 server 的工具一批出现，server 与 server 之间逐个出现。

## Requirement Decisions / 需求决策

- 2026-08-08 17:52:
  - 决策：**工具系统与 agent 解耦**——MCP 装配异步化（启动后台、刷新/保存 async），不阻塞应用启动；`discover_tools` 加超时；新增 `StateChange::Tools` 事件与 `Connecting` 状态；刷新命令保持有界等待。
  - 原因：用户实际观察「启动正常、刷新卡死」证明装配执行环境存在差异（启动=同步上下文独立 runtime；刷新=async worker 内嵌 `block_on`）；用户明确要求「工具有自己的独立房间，工具和 agent 可以分离」；避免永久卡死必须补超时。
