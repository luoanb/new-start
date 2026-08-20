# 终端浏览器支持：WebSocket PTY 网关（桌面内嵌）

- 日期：2026-08-20
- 状态：Draft（Approval Pending）
- 关联：`docs/specs/2026-08-20_10-00_terminal-panel.md`（终端面板 v1，其「范围：仅 Tauri GUI」本节扩展）、`src-tauri/src/terminal/`（PTY 会话/管理器，本期复用）
- 背景：用户反馈（a）浏览器运行时终端被禁用不合理；（b）桌面端终端字母间距异常（疑似 WebKitGTK 渲染/测量问题，尚未定位）。本期同时解决两者：加 WebSocket 网关让浏览器可用，浏览器端即可直接复现并定位字体问题。

## Goal

让 Pulsar 终端功能在**浏览器（非 Tauri）运行时也可用**：Tauri 进程内嵌 WebSocket PTY 网关，浏览器前端通过 WebSocket 连接同一个 `TerminalManager`，与桌面 IPC 共用会话。桌面模式保持 IPC 不变（零行为差异）。

## Done Contract

- 完成：非 Tauri 环境下终端面板不再显示禁用提示，而是通过 WebSocket 连接本地网关，可 spawn/write/resize/kill/list + 接收 output/exit 事件；桌面 IPC 路径行为不变；浏览器（chromium/firefox）下可真实交互并完成字体问题复现与修复。
- 证明：`cargo check --all-targets`、`cargo test --lib` 相关用例、`pnpm check`；运行期冒烟——`tauri dev` 跑起后浏览器打开 `http://localhost:5173` 终端面板可交互。
- 仍未完成：浏览器连不上网关（连接态无恢复），或桌面 IPC 回归（桌面终端不可用）。

## Scope

### In

- Tauri 内嵌 `tokio-tungstenite` WebSocket server（绑定 `127.0.0.1`，端口可配，默认 `43110`），复用 `TerminalManager` 会话。
- JSON 帧协议：`spawn / write / resize / kill / list`（client→server）+ `spawned / output / exit / list / error`（server→client）；二进制输出 base64 编码。
- 会话输出事件重构为全局 broadcast（`tokio::sync::broadcast`），桌面 IPC 与 WebSocket 双路转发，消除重复读线程。
- 前端 transport 抽象：`IpcTransport`（现状）/ `WsTransport`（新增，含断线重连与连接状态），`TerminalPanel` 按 `isTauriEnv` 选择；移除禁用提示，改为连接状态展示。
- 浏览器环境下用 chromium/firefox 复现终端渲染，定位并修复字母间距问题（桌面 WebKitGTK 同根因）。
- 依赖：`tokio-tungstenite`、`futures-util`（Cargo）；前端无新依赖。

### Out（后续迭代）

- 远程部署形态（无 Tauri 进程的独立网关服务）——v2 独立 spec。
- 鉴权 token（v1 仅绑定回环地址，预留 `?token=` 字段，前端不启用）。
- 多浏览器并发写同一会话的输入冲突协调。
- 方案 B2（共享会话注入）。

## Facts（代码事实）

- `terminal/`（M1 完成）：`TerminalSession::spawn` 每会话一个阻塞读线程 → `mpsc` → 命令层 tokio task 内 `app.emit("app://terminal-output")`（`terminal/commands.rs`）；`TerminalManager` 为会话注册表，`app.manage` 注入。
- 事件名：`app://terminal-output` / `app://terminal-exit`（`terminal/session.rs` 常量）。
- 前端 `TerminalPanel.svelte`（M2 完成）：`use:mountTerminal` 创建 xterm；`onMount` 中 `if (!isTauriEnv) return;` 直接降级禁用（本次要替换）；`api.invoke("terminal_*")` + `listen("app://terminal-*")`。
- `isTauriEnv` 来自 `$lib/api`（`window.__TAURI_INTERNALS__` 探测）。
- Cargo 现状：tokio（含 sync/process/io-timeout）、tauri 2、`portable-pty`；**无 tungstenite / broadcast**。
- 布局/视图：终端面板已注册于 panel 容器（v11），浏览器模式布局照常可用。

## Restated Understanding（已决决策）

1. 浏览器支持**正式立项**（用户确认），不再维持「仅 Tauri GUI」的浏览器降级。
2. v1 形态：**Tauri 内嵌 WS 网关，浏览器需同机有运行中的桌面进程**（开发场景：vite dev + tauri dev 同跑；浏览器与桌面共享同一会话集）。
3. 桌面 IPC 路径保持不动，WS 为增量通道；会话输出双路转发。
4. 安全：绑定 `127.0.0.1` 回环，v1 不设 token（同机等价本机终端风险），协议预留 token 字段。

## 接口契约设计

### 1. 后端（`src-tauri/src/terminal/`）

```text
ws.rs         # start_ws_server(listener, manager, app): 每连接任务 + 帧编解码 + 事件转发
events.rs     # （新）会话输出/退出 broadcast 广播器；commands.rs 与 ws.rs 共同订阅
```

- 启动：`lib.rs` setup 中 `tokio::spawn(start_ws_server(...))`；端口 `PULSAR_TERMINAL_WS_PORT` env → 默认 `43110`；绑定 `127.0.0.1`。
- 会话输出不再由 commands.rs 独占：读线程 → mpsc → 一个共享任务转发到 `broadcast`；桌面监听（当前 tabs）与 ws 连接各自订阅。
- `ws.rs` 连接生命周期：认证（预留 token 比对，v1 恒过）→ 请求循环 → 会话事件转发；断连清理该连接订阅。

### 2. WebSocket 协议（JSON 文本帧）

| 方向 | type | 载荷 |
|---|---|---|
| c→s | `spawn` | `{ cwd?, shell?, cols?, rows? }` |
| c→s | `write` | `{ sessionId, data: base64 }` |
| c→s | `resize` | `{ sessionId, cols, rows }` |
| c→s | `kill` | `{ sessionId }` |
| c→s | `list` | —— |
| s→c | `spawned` | `{ sessionId }` |
| s→c | `output` | `{ sessionId, data: base64 }` |
| s→c | `exit` | `{ sessionId, exitCode }` |
| s→c | `list` | `{ sessions: SessionInfo[] }` |
| s→c | `error` | `{ message }` |

### 3. 前端 transport 抽象

```text
src/lib/terminal/transport.ts
  type TerminalTransport = {
    spawn(opts): Promise<string /*sessionId*/>;
    write(id, data: Uint8Array): Promise<void>;
    resize(id, cols, rows): Promise<void>;
    kill(id): Promise<void>;
    list(): Promise<TerminalSessionInfo[]>;
    onOutput(cb), onExit(cb), onSpawned(cb): () => void;  // 返回退订
    status(): "connecting" | "connected" | "disconnected";
  }
  ipcTransport(): TerminalTransport      // 现有 Tauri 逻辑迁移
  wsTransport(url): TerminalTransport    // 浏览器；base64 编解码 + 自动重连
```

- `TerminalPanel.svelte`：`const transport = isTauriEnv ? ipcTransport() : wsTransport(WS_URL)`；`WS_URL = import.meta.env.VITE_TERMINAL_WS_URL ?? "ws://127.0.0.1:43110"`。
- 禁用提示替换为状态栏（连接中/已断开 + 重试），tab 栏与 xterm 逻辑不随 transport 变化。

## 安全

| 场景 | 护栏 |
|---|---|
| 桌面 IPC | 不变（tauri 命令级权限） |
| WebSocket（回环） | 绑定 127.0.0.1；v1 无 token（同机等价本机终端），协议预留 `token` 字段供 v2 |
| agent `visible_terminal` | 不变（走既有护栏） |

## 实现要点（里程碑）

1. **W1（后端）**：Cargo 加 `tokio-tungstenite` + `futures-util`；输出事件 broadcast 重构（commands.rs 改订阅）；`terminal/ws.rs` 帧协议 + 连接管理；`lib.rs` 启动 ws server；`cargo check`。
2. **W2（前端）**：`transport.ts` 抽象 + `ipcTransport`/`wsTransport`；TerminalPanel 改用 transport，移除禁用提示改连接状态；`pnpm check`。
3. **W3（浏览器验证 + 字体修复）**：`tauri dev` 与 vite 同跑，chromium/firefox 打开浏览器终端真实交互；复现桌面字母间距问题（webkit/chromium 对照），定位根因（cell 测量 / 字体解析 / 渲染器）并修复（候选：单字体栈、DOM renderer、字号/dpr 修正）；全量 check + 冒烟。

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是（浏览器可用性 + 终端字体渲染质量，均属终端面板体验闭环）。
- 若否，偏差在哪里：无。
- 是否需要调整本轮目标或范围：待用户确认 Open Questions 后定稿。

## Open Questions

- [ ] v1 形态确认：Tauri 内嵌 WS 网关、浏览器需同机桌面进程在跑（远程独立网关 v2）？→ 待确认。
- [ ] 回环无 token 可接受？（等价本机终端风险，协议预留 token 字段）→ 待确认。
- [ ] 端口默认 `43110`、`PULSAR_TERMINAL_WS_PORT` 可覆盖？→ 待确认。

## Change Log

- 2026-08-20：初稿。背景 = 浏览器禁用终端 + 桌面端字母间距异常；方案 = 内嵌 WS 网关复用 TerminalManager + 前端 transport 抽象 + 浏览器环境复现字体问题。待用户确认 Open Questions 后批准执行。

## Validation

- Self-check：复用既有 `TerminalManager`/事件名，桌面路径零改动，改动面收敛在增量通道。
- Static checks：`cargo check --all-targets`、`pnpm check`。
- Runtime / Test：`cargo test --lib`（broadcast 转发、协议编解码用例）；手动冒烟——tauri dev + 浏览器同跑，两端共享会话交互；chromium/firefox 下终端字体间距恢复正常或定位到 WebKitGTK 特有根因。
- Human confirmation：用户批准本 spec 后进入实现。
- 结果汇总：未执行，待批准。
- 剩余风险：WS 协议错误处理（非法帧/超大帧）、断线重连竞态、浏览器并发写同一会话；broadcast 背压（高频输出 v1 沿用 chunk 聚合）。
