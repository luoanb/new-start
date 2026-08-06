# Requirements / 需求文档: tool-runtime-integration

## Restated Understanding / 需求复述

- 我理解当前需求是：为 agent-app 建立**运行阶段工具集成**能力，三个通道并行——项目自有 native 工具（维持现状，`execute_command` 仅存，dead_code 工具不上架）、配置驱动 DynamicTool（命令模板 + HTTP 两种后端）、MCP 工具（rmcp 稳定版客户端，stdio + streamable-http）。
- 当前核心目标是：工具集按配置装配（启动期装配 + 运行期**手动触发重新装配**）；insert 门禁分级（项目自有 native 工具保留、动态通道豁免）；`ToolDefinition` 增加 `source` 字段支撑治理；前端提供工具面板（列表只读展示 + 弹窗编辑配置，保存即生效）。
- 当前边界是：无运行期热监听自动重扫，配置变更通过 UI 保存显式触发重装配（保存即生效，无需重启）。
- 暂不处理：会话中热增 / file watcher 自动重扫 / 用户确认机制（v2）、dead_code 工具（get_neuron 等 9 个）上架、MCP 工具描述增强 insert、agent 模式按 neuron 白名单过滤。

## Scope / 范围

- In:
  - `ToolRegistry` 支持三类来源注册（native / config / mcp），`ToolDefinition` 增加 `source` 字段。
  - 配置驱动 DynamicTool：命令模板 + HTTP 两种后端，具名工具声明；命令模板后端复用 `ExecuteCommandTool` 安全护栏（硬约束）。
  - MCP 客户端适配（rmcp 稳定版）：stdio + streamable-http，`tools/list` 发现，豁免 insert 门禁。
  - 配置：独立 `mcp_servers.json` + 独立 `dynamic_tools.json`，启动期读取；运行期 UI 保存写回并触发重新装配。
  - **运行期手动重装配**：注册表运行时可变（共享化），保存配置后重建全量工具集（native + config + mcp），MCP 连接按新配置重连。
  - **前端配置管理 UI**：工具列表只读展示（来源分组 + server 状态）+ 弹窗编辑（新增/修改/删除 MCP server 与动态工具），保存写回 JSON 并触发重装配。
  - 单元测试、`cargo check`、前端构建验证与文档回写。
- Out:
  - 运行期热监听自动重扫（file watcher，v2）。
  - dead_code 工具上架（get_neuron / list_neurons / update_neuron / get_network / create_neuron / select_neuron_candidates / get_current_time / echo / calculate）。
  - MCP 工具描述增强 insert（可选，非门禁）。
  - 用户确认机制（v2）。
  - agent 模式按 neuron `tool_ids` 白名单过滤（机制现状保持不变）。

## User Interaction / 用户交互

- 触发入口：GUI 可折叠 DockPane 面板新增「工具」视图（Q1 已确认）。
- 用户操作路径：打开工具视图 → 查看按来源分组（native / config / mcp）的工具列表 → 查看单个工具描述与参数 → 查看 MCP server 连接状态 → 点击「编辑」/「添加」进入弹窗修改配置 → 保存（写回 JSON + 触发重新装配）→ 列表与状态刷新。
- 系统反馈：工具列表展示注册结果；MCP server 状态（connected / failed / disabled）；空配置显示空态；保存后立即重新装配并刷新列表；配置非法 → 拒绝保存并提示具体错误。
- 状态变化：运行期注册表可变更（保存配置触发全量重装配）；MCP 连接按新配置重建。
- 异常/边界交互：MCP server 连接失败 → 面板标注 failed，该 server 工具不出现在列表；保存时校验失败 → 不写文件不触发，面板展示错误；重装配期间正在执行的旧工具调用不中断（旧注册表引用直至完成）。
- 不应发生的交互：面板不提供运行期热监听自动重扫；不提供绕过配置文件的直接工具级增删（变更必须写回配置并重装配）。

## Acceptance Criteria / 验收标准

- [ ] 启动期装配：gateway 装配后 registry 含 `execute_command` + 配置驱动工具 + 健康 MCP server 的工具。
- [ ] insert 门禁分级：native 工具注册无 insert 时报错/panic；config / mcp 来源注册豁免 insert。
- [ ] `ToolDefinition.source` 正确标记来源，前端面板按来源分组展示。
- [ ] 命令模板后端复用 `ExecuteCommandTool` 安全护栏（denylist、超时、并发、输出截断、日志脱敏），有测试覆盖。
- [ ] HTTP 后端具名工具（固定端点）可配置、可调用，有测试覆盖。
- [ ] MCP 客户端：stdio 与 streamable-http 各至少一条接入路径可用（mock server 单测或冒烟）。
- [ ] 单个 MCP server 连接失败不阻塞应用启动（warn + skip），面板显示 failed。
- [ ] 运行期重装配：保存配置（MCP server / 动态工具变更）后 registry 更新、MCP 连接重建、面板刷新，无需重启。
- [ ] 配置校验：非法配置（未知 transport / 缺 url 或 command / 非法命令模板）保存被拒并给出可读错误。
- [ ] 前端工具面板：列表只读展示 + 弹窗编辑（增删改）写回配置。
- [ ] `cargo test --lib` 全绿；`cargo check --all-targets` 通过；`vite build` 通过。

## Constraints / 约束

- 业务约束：neuron `tool_ids` 白名单授权边界不变；agent 模式工具集是全局的（[engine.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/agent-app/src-tauri/src/core/engine.rs#L185-L207) 取 registry 全量 definitions）。
- 技术约束：rmcp 锁稳定版（3.x），不锁 beta；命令模板后端不得绕过 `ExecuteCommandTool` 安全模型；日志脱敏沿用 `log_redact`；insert 门禁只对 native 来源生效；重装配使用读锁取工具、写锁重建（读锁不跨 await，避免与重装配写锁长阻塞）。
- 时间/兼容性约束：不引入破坏性变更；新增配置文件不影响既有 `config.json` 读取。

## Open Questions / 开放问题

- [x] Q1 前端工具面板形态：**DockPane 面板**（已确认 2026-08-07）——可折叠浮动面板，展示工具列表与 server 状态。
- [x] Q2 配置驱动工具配置文件：**独立 `dynamic_tools.json`**（已确认 2026-08-07）——与 `mcp_servers.json` 并列，均在存储根目录。
- [x] Q3 运行期重装配触发：**保存即生效**（已确认 2026-08-07）——UI 保存写回 JSON 并立即触发重装配，无需重启。
- [x] Q4 重装配范围：**全量重建**（已确认 2026-08-07）——native + config + mcp 统一重建。
- [x] Q5 配置管理 UI 形态：**列表展示不变 + 弹窗编辑**（已确认 2026-08-07）。

## Requirement Decisions / 需求决策

- 2026-08-07 00:12（补充）:
  - 决策：运行阶段语义采用**纯 A + 运行期手动重装配**（启动期装配 + UI 保存显式触发全量重装配，无需重启）；通道**并行**（native 维持 + 配置驱动全量保留 + MCP rmcp 稳定版）；insert 门禁**分级**（MCP 等动态通道豁免、schema 即契约，项目自有 native 工具保留）；MCP 配置独立 `mcp_servers.json`；范围**含前端面板（列表展示 + 弹窗编辑）**。
  - 原因：多轮讨论收敛——社区 MCP 事实标准对齐、与仓库「No Spec, No Code」治理同构、运行期改动通过显式保存触发（无热监听复杂度）、动态工具自描述豁免。
- 2026-08-07（追加）:
  - 决策：配置变更**保存即生效**（写回 JSON + 立即全量重装配）；重装配范围**全量重建**；UI 保持**列表展示 + 弹窗编辑**。
  - 原因：用户澄清「运行期间改动并手动触发更新的」，否定「重启生效」设定。
