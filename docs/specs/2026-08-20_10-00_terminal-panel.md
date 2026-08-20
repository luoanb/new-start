# 新增集成终端面板：PTY 终端 + execute_command 可见执行

- 日期：2026-08-20
- 状态：Approved（2026-08-20 用户批准「v1 = 方案 A + B1，B2 二期独立 spec」，进入实现）
- 关联：`docs/specs/2026-08-04_10-30_cmd-exec-tool.md`（execute_command v1，其「独立用户终端面板（形态 B）」列为 Out，本次落地）、`docs/specs/2026-07-25_20-42_agent-tools.md`（工具系统）

## Goal

在 Pulsar（星脉）Tauri 桌面端新增**对标 VS Code 集成终端的可交互终端面板**（xterm.js + PTY），并让 Agent 的 `execute_command` 具备「终端可见执行」能力（命令在终端面板中实时显示输出，用户可介入）。

## Done Contract

- 完成：终端面板可作为独立视图在底部 panel 容器打开，支持多 tab 可交互 shell；`execute_command` 增加 `visible_terminal` 语义后，agent 命令输出实时可见。
- 证明：`cargo check --all-targets` 通过、`cargo test --lib` 相关用例通过、`pnpm check` 通过；运行期手动冒烟（面板交互 + agent 可见执行）。
- 仍未完成：任一核心链路未跑通（spawn/输出/输入/kill），或 agent 可见执行未接入。

## Scope

### In

- 后端独立模块 `src-tauri/src/terminal/`（用户要求：**独立文件夹，不放 `core/`**）。
- PTY 会话管理（多会话 tab、cwd/shell 指定、resize、kill、退出事件）。
- Tauri IPC：spawn / write / resize / kill / list + 高频输出事件 `app://terminal-output` + 退出事件 `app://terminal-exit`。
- 前端 `TerminalPanel.svelte`（xterm.js）接入现有视图系统（`viewRegistry` + panel 容器，无需改布局类型）。
- `execute_command` 接入终端（方式 A：`visible_terminal` 参数，模型行为不变）。
- B1：agent 创建的终端 tab 用户可直接接管输入（PTY 双向，前端放开输入权，≈0 成本）。
- 安全护栏复用（denylist / 超时 / 并发 / 截断 / 日志脱敏）。

### Out（后续迭代）

- Agent 与用户**共享同一终端会话**（方式 B，交互冲突风险高，二期再评估）。
- 终端配置持久化（默认 shell 记忆、主题/字号设置）。
- 拖拽 tab、终端搜索、富文本复制等 VS Code 高级体验。
- TUI（pulsar-tui）与 CLI（pulsar-cli）的终端能力。

## Facts（代码事实）

- `Tool` trait：`name() / description() / parameters() / async execute(args) -> AppResult<String>`（`core/tool_registry.rs`）。
- `execute_command` 由 `ExecuteCommandTool` 实现（`core/cmd_exec.rs`），`run_guarded_shell` 为共享护栏执行器（denylist 前缀/词边界、超时 `[1000, 120000]`、并发 Semaphore(4)、输出 64KB 截断、日志脱敏）；生产注册于 `Gateway::build_base_registry`（`core/gateway.rs:1197-1201`，`registry.register_core(...)`）。
- 工具执行链路：`RoundExecutor::execute_tools` → `tool.execute(args)` → 结果 `ToolResultItem` 落库回传模型（`core/round_executor.rs:282-350`）。
- 事件通道：`StateChange` + `STATE_CHANGED_EVENT`（`app://state-changed`，`core/events.rs`）；高频终端输出应**独立事件名**，不混入全量拉取。
- 布局系统：`ViewContainerId = "sidebar" | "info" | "panel"`（`src/lib/layout/layoutTypes.ts`）；panel 容器默认挂 `poller`、`logs`；新增视图 = `viewRegistry` 注册一条记录（`src/lib/layout/views.ts:52-113`），`+page.svelte` 无需改动。
- 前端依赖现状：`@tauri-apps/api@2`、Svelte 5、CodeMirror；**无 xterm**。
- 后端 Cargo：tokio 已含 `process`；**无 PTY 依赖**；`portable-pty` 为同步阻塞 API，需线程 + mpsc 桥接 tokio/tauri event loop。
- 日志规则（`.cursor/rules/logging-observability.mdc`）：shell 命令调用属关键节点，禁止记录命令原文与输出正文。

## Restated Understanding（已决决策）

1. **形态**：应用内集成终端面板（对标 VS Code 集成终端），非只读流、非外部 VS Code 扩展。
2. **Agent 接入**：v1 即接入（方式 A —— 增强 `execute_command`，`visible_terminal: bool` 默认 `false`；模型仍拿到聚合结果，行为不变）。方案 B 经深度研究拆为 B1（tab 用户可接管，建议并入 v1）/ B2（真共享会话注入，二期独立 spec），详见「方案 B 深度研究」。
3. **范围**：仅 Tauri GUI（pulsar-app）。
4. **后端目录**：`src-tauri/src/terminal/`，独立于 `core/`。
5. **安全策略**：用户手动终端无额外护栏；agent 可见执行沿用 v1 护栏，不提升权限。

## 接口契约设计

### 1. 后端模块（`src-tauri/src/terminal/`）

```text
src-tauri/src/terminal/
  mod.rs        # 模块入口、公开类型（SessionInfo: id/cwd/shell/pid/exit_code…）
  session.rs    # TerminalSession：portable-pty 封装（spawn / write / resize / kill / read 循环）
  manager.rs    # TerminalManager：session_id ↔ Arc<Mutex<TerminalSession>> 注册表
  bridge.rs     # TerminalBridge：execute_command 接入终端时输出旁路广播（Option 注入）
  commands.rs   # Tauri invoke 命令 + 事件发射（lib.rs 注册 handler）
```

- `portable-pty`（wezterm 出品，Unix pty / Windows ConPTY）创建伪终端；默认 shell `$SHELL`（Win `cmd.exe`），cwd 默认 workspace 根目录。
- 每会话一个阻塞读线程 → `mpsc::channel` → tokio 侧 `app.emit("app://terminal-output", ...)`；会话进程退出 → emit `app://terminal-exit`。

### 2. IPC 契约

| 方向 | 通道 | 载荷 |
|---|---|---|
| invoke | `terminal_spawn` | `{ cwd?, shell?, cols?, rows? }` → `{ session_id }` |
| invoke | `terminal_write` | `{ session_id, data }` |
| invoke | `terminal_resize` | `{ session_id, cols, rows }` |
| invoke | `terminal_kill` | `{ session_id }` |
| invoke | `terminal_list` | → `[SessionInfo]` |
| event | `app://terminal-output` | `{ session_id, data }` |
| event | `app://terminal-exit` | `{ session_id, exit_code }` |

### 3. execute_command 可见执行（方式 A）

- `ExecuteCommandTool::new(bridge: Option<TerminalBridge>)`；`parameters` 新增 `visible_terminal: bool`（描述：在终端面板可见会话中执行并实时回显输出）。
- `visible_terminal=true`：命令交给 `bridge` 在**新专用 tab** 会话中执行（默认不塞入用户当前 tab），stdout/stderr 逐 chunk emit；同时仍返回聚合 JSON（exit_code/stdout/stderr/timed_out）给模型。
- 新增 `run_guarded_pty`：复用 `run_guarded_shell` 的护栏语义（denylist 前置校验、超时夹紧 + kill、并发 Semaphore、输出截断、日志脱敏）。
- 未注入 bridge 或 `visible_terminal=false`：走既有 `sh -c` 管道路径，行为完全不变。

### 4. 前端集成

- 依赖：`@xterm/xterm`（注意包名，v5.x）+ `@xterm/addon-fit`。
- 新组件 `TerminalPanel.svelte`：tab 栏（+ 新建按钮，VS Code 语义）+ xterm 实例 + 事件监听（output/exit）+ resize 适配。
- `viewRegistry` 注册 `terminal` 视图并默认挂入 panel 容器；补 i18n keys（`views.terminal` 等）；深色 oklch 风格对齐 `ToolResultBlock`。

## 安全

| 场景 | 护栏 |
|---|---|
| 用户手动使用终端 | 无额外护栏（等价本机终端） |
| agent `visible_terminal` 执行 | 复用 v1：denylist / 超时 / 并发 / 截断 / 日志脱敏；`visible_terminal` 仅加可见性不提升权限 |

## 实现要点（里程碑）

1. **M1（后端）** ✅：`Cargo.toml` 加 `portable-pty`；`terminal/` 四文件（session/manager/bridge/commands）+ `lib.rs` 注册 module/state/handler；单会话 spawn → 输出事件 → kill 跑通。
2. **M2（前端）** ✅：装 `@xterm/xterm` + `@xterm/addon-fit`；`TerminalPanel.svelte` + 视图注册 + 多 tab；布局 v11 迁移（panel 容器补 terminal）。
3. **M3（Agent 接入）** ✅：`ExecuteCommandTool` 注入 bridge（`with_terminal`）+ `visible_terminal` 参数 + `run_guarded_pty`；gateway 三处装配接线（build 初始 / 后台 MCP / 运行期 reassemble）。
4. **M4（打磨）** ✅：VS Code 风格主题 + i18n（views.terminal）+ tab 标题（agent 命令文本）+ exited 会话输入拦截；单元测试（session 4 + manager 1 + cmd_exec 护栏 7）；`cargo check --all-targets`、`cargo test --lib`（299 passed）、`pnpm check` 本次改动零新增错误。

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是（方案已按用户四项确认收敛：形态 / Agent 接入 / 范围 / 独立目录）。
- 若否，偏差在哪里：无。
- 是否需要调整本轮目标或范围：待用户确认下列 Open Questions 后定稿。

## 方案 B 深度研究（共享会话：agent 注入 + 读取）

> 研究范围：让 agent 向「用户正在使用的终端会话」注入命令并读取对应输出。结论：**v1 走方案 A；B 拆 B1/B2 两级演进**（见文末）。

### 业界参照（已验证的成熟做法）

| 方案 | 机制 | 可借鉴点 | 局限 |
|---|---|---|---|
| VS Code Shell Integration（OSC 633） | shell 钩子发射标记序列切分命令生命周期：bash 用 `PROMPT_COMMAND` + `trap DEBUG`；zsh 用 `precmd` + `preexec`；pwsh 用 `prompt`；fish 用 `fish_prompt`/`fish_preexec`。序列：`633;A` 激活 / `633;B` 命令开始 / `633;C` 执行 / `633;D[;exit]` 结束(带退出码) / `633;E;cmd[;nonce]` 命令文本 / `633;P;prop` 属性。扩展 API：`onDidStartTerminalShellExecution` / `onDidEndTerminalShellExecution` / `execution.read()` | **输出归属的标准解法**：shell 主动标记，非 buffer 快照 hack | 依赖受控 shell；用户自定义 prompt 可能覆盖注入点 |
| terminal-mcp（Rust crate） | 长生命周期交互 shell 会话：one-shot `exec` vs 交互式 `shell_*`（spawn/send_line/output/wait_for/snapshot/close）；PTY 模式 | 交互纪律：**「send 后必 output/wait_for 确认，绝不批量猜测发送」**；控制字符走 `send_control`；黑名单禁止交互程序直启，强制 spawn-bash-then-send | 面向模型会话，非面向用户可见面板 |
| tmux 共享终端审计 | `tmux send-keys` 注入 + `tmux capture-pane` 取屏；多人 attach 实时观看 | 会话持久化 + 共享 attach 的天然载体 | `capture-pane` 仅屏幕快照（滚动行数受限、需剥 ANSI）；Windows 不可用；不适合产品化通用终端 |
| Prode（VS Code 扩展） | 基于 Shell Integration API 读取命令输出实时反馈给 agent | 验证了「shell integration + agent 自动反应」的产品路径 | 依赖 VS Code 宿主 |
| telepty | 会话注入/attach 控制面，会话按名寻址 | agent 会话可寻址、可注入 | 面向 CLI 编排，非内嵌终端 |

### 核心机制（若自研，增量 = 注入脚本 + 标记解析 + 交互工具）

1. **注入**：spawn 会话时向 shell 注入钩子脚本（bash/zsh 优先，pwsh/fish 次之），发射自定义 OSC 标记：命令开始（带 nonce）、命令结束（带退出码）、cwd 变更。
2. **解析**：读线程维护状态机解析标记流 → 把原始字节流按命令切块 → 归属到对应 agent 调用。
3. **交互工具**（agent 侧新增）：`terminal_send`（写 stdin + 换行）、`terminal_wait_for`（模式匹配 + 超时）、`terminal_send_control`（Ctrl+C/Ctrl+D）。
4. **输入竞争**：写锁 + 「agent 操作中」UI 状态；send 后必 wait_for 的纪律；与用户同时打字可能混入 —— **VS Code sendText 同款已知限制，无完美解**。

### 问题矩阵

| 问题 | 业界解法 | 采用机制 |
|---|---|---|
| 输出归属 | OSC 633 标记 / capture-pane | 自研 shell integration 标记切块 |
| 输入竞争 | sendText 不解决；terminal-mcp 用「send 后必确认」纪律 | 写锁 + UI 状态 + wait_for 纪律 |
| 交互式命令 | spawn-bash-then-send + wait_for 模式匹配 | `terminal_wait_for` 模式匹配 + 超时 |
| 状态污染 | —— | 调用前记录/设置 cwd；会话粒度隔离 |
| 生命周期 | —— | 不随意 kill；kill 仅限 agent 自建会话或用户确认 |

### 工程评估

- **核心增量**：注入脚本 + 标记解析状态机 + 交互式工具 ≈ **方案 A agent 接入部分的 1.5–2 倍工作量**。
- **边界情况多**：非交互 shell 需静默降级（退回整段归并）、Windows ConPTY + pwsh 注入、用户自定义 prompt 覆盖注入点、vim/htop 全屏程序期间的归属、shell 版本差异。
- **风险**：注入失败导致归属错乱；与用户 prompt 定制冲突。

### 结论与演进建议

- **v1 按方案 A**（隔离、确定性高、改动小）。
- **方案 B 拆两级**：
  - **B1（低成本，建议并入 v1）**：agent 创建的终端 tab **用户可直接接管输入**（PTY 本就是双向的，前端放开输入权即可）——解决「用户接管」约 80% 的诉求，工作量 ≈ 0。
  - **B2（真方案 B，二期独立 spec）**：agent 向指定会话（含用户会话）注入 + OSC 标记解析 + 交互式工具。

## Open Questions

- [x] 是否接受引入 `portable-pty` 依赖（跨平台 PTY，纯 Rust，无系统级 C 风险）。→ **已确认**。
- [x] agent 可见执行默认开「新专用 tab」而非用户当前 tab。→ **已确认**。
- [x] `portable-pty` / `@xterm/xterm` 具体版本号（实现时以最新稳定为准，此处不锁死）。→ **已确认**。
- [x] **Agent 接法最终定夺**：v1 = 方案 A + B1（tab 用户可接管）→ **已确认**；B2（真共享会话）→ **二期独立 spec 立项**。

## Change Log

- 2026-08-20：初稿。基于方案讨论落盘，用户已确认形态 / Agent 接入（v1 即接入）/ 范围（仅 GUI）/ 独立目录四项决策；待批准后进入实现。
- 2026-08-20：用户确认 Q1（portable-pty）/ Q3（新专用 tab）/ Q4（版本不锁死）；完成方案 B 深度研究（业界参照：VS Code OSC 633 shell integration、terminal-mcp、tmux 共享审计、Prode、telepty）。结论：v1 走方案 A + B1（tab 用户可接管，≈0 成本）；B2 二期独立 spec。待用户定夺最后一项 Open Question 后进入实现。
- 2026-08-20：用户批准「v1 = 方案 A + B1，B2 二期独立 spec」，状态置 Approved，进入实现。

## Validation

- Self-check：方案与现有代码事实（cmd_exec 护栏、viewRegistry、events、gateway 组装点）对齐。
- Static checks：`cargo check --all-targets`、`pnpm check`（实现阶段执行）。
- Runtime / Test：`cargo test --lib`（session/manager/护栏用例）；手动冒烟——打开终端面板交互输入输出、`execute_command` 带 `visible_terminal` 实时回显、denylist 仍拒绝危险命令。
- Human confirmation：用户批准本 spec 后进入实现。
- 结果汇总：未执行，待批准。
- 核心目标是否已由证据证明完成：否（尚未实现）。
- 剩余风险：portable-pty 阻塞 API 与 tokio 桥接细节；高频输出事件对前端性能的影响（v1 以 chunk 聚合控制频率）。

## Resume / Handoff

- 当前状态：方案已落盘（Draft，Approval Pending）。
- 当前卡点：待用户确认 Open Questions 并批准执行。
- 下一步唯一动作：用户批准后按 M1→M4 实施。
- 下一轮核心目标：跑通「终端面板可交互 + agent 可见执行」最小闭环。
