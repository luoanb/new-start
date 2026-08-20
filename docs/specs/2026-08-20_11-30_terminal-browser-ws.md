# 终端浏览器支持：WebSocket PTY 网关（axum 同端口整合）

- 日期：2026-08-20
- 状态：Approved（2026-08-20 架构修订后再确认）
- 关联：`docs/specs/2026-08-20_10-00_terminal-panel.md`（终端面板 v1，其「范围：仅 Tauri GUI」本节扩展）、`src-tauri/src/terminal/`（PTY 会话/管理器，本期复用）
- 背景：用户反馈（a）浏览器运行时终端被禁用不合理；（b）桌面端终端字母间距异常（疑似 WebKitGTK 渲染/测量问题，尚未定位）。本期同时解决两者：加 WebSocket 通道让浏览器可用，浏览器端即可直接复现并定位字体问题。架构修订：用户要求 WS 作为公共服务、终端仅其一业务，且 HTTP RPC 与 WS 公用同一监听（axum 同端口 `/ws` 端点），前端 WS 地址从远程连接配置自动推导。

## Goal

让 Pulsar 终端功能在**浏览器（非 Tauri）运行时也可用**：Tauri 进程内嵌 WebSocket PTY 网关，浏览器前端通过 WebSocket 连接同一个 `TerminalManager`，与桌面 IPC 共用会话。桌面模式保持 IPC 不变（零行为差异）。

## Done Contract

- 完成：非 Tauri 环境下终端面板不再显示禁用提示，而是通过 WebSocket 连接本地网关，可 spawn/write/resize/kill/list + 接收 output/exit 事件；桌面 IPC 路径行为不变；浏览器（chromium/firefox）下可真实交互并完成字体问题复现与修复。
- 证明：`cargo check --all-targets`、`cargo test --lib` 相关用例、`pnpm check`；运行期冒烟——`tauri dev` 跑起后浏览器打开 `http://localhost:5173` 终端面板可交互。
- 仍未完成：浏览器连不上网关（连接态无恢复），或桌面 IPC 回归（桌面终端不可用）。

## Scope

### In

- **axum `/ws` 端点**（与 HTTP RPC 同端口、同监听、同 token 鉴权；路径区分 `/rpc` 与 `/ws`），复用 `TerminalManager` 会话。WS 为**公共服务**：帧带 `topic` 信封按业务分发，终端是第一个业务（`topic: "terminal"`）。`terminal_ws` 配置节废弃，WS 随 `server` 节启用。
- JSON 帧协议（信封化）：`{ topic: "terminal", type, ... }`——`spawn / write / resize / kill / list`（client→server）+ `spawned / output / exit / list / error`（server→client）；二进制输出 base64 编码。
- 会话输出事件重构为全局 broadcast（`tokio::sync::broadcast`），桌面 IPC 与 WS 双路转发，消除重复读线程。
- 前端 transport 抽象：`ipcTransport`（现状）/ `wsTransport`（新增，含断线重连与连接状态），`TerminalPanel` 按 `isTauriEnv` 选择；移除禁用提示，改为连接状态展示。**WS 地址从远程连接配置自动推导**（`http(s)://host:port` → `ws(s)://host:port/ws?token=`），零新增配置。
- 浏览器环境下用 chromium/firefox 复现终端渲染，定位并修复字母间距问题（桌面 WebKitGTK 同根因）。
- 依赖：`futures-util`、axum 内置 `ws`（Cargo；`tokio-tungstenite` 降为 dev-dependency 仅作测试客户端）；前端无新依赖。

### Out（后续迭代）

- 远程部署形态（无 Tauri 进程的独立网关服务）——v2 独立 spec。
- 终端之外的更多 WS 业务（如实时状态推送）——协议已支持 topic 分发，业务另行立项。
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
2. v1 形态：**WS 挂载于 axum `/ws`，浏览器需同机有运行中的桌面进程**（开发场景：vite dev + tauri dev 同跑；浏览器与桌面共享同一会话集）。
3. 桌面 IPC 路径保持不动，WS 为增量通道；会话输出双路转发。
4. 安全：WS 与 HTTP RPC **共用 `server` 节的 token 白名单鉴权**（`auth_middleware`；浏览器 WS 无法自定义头，token 走 `?token=`，与 SSE 一致）。白名单为空 → 放行（等价仅本机可达的免鉴权）；非空 → 必须携带白名单内 token。
5. 架构（用户确认）：WS 为**公共服务**，终端是第一个业务（`topic` 信封分发）；HTTP 与 WS **同端口**（一个 bind、一套防火墙规则、一套鉴权）；前端 WS 地址**从远程连接配置（`pulsar:remoteUrl`）自动推导**，零新增配置。

## 接口契约设计

### 1. 后端

```text
net/ws.rs         # （新）通用 WS 接入：axum WebSocketUpgrade + auth 后的连接循环、
                  #   topic 信封分发 + 各业务事件转发（终端 output/exit 带 topic 信封推送）
terminal/ws.rs    # （改造）终端业务 handler：帧解析（WsRequest）、命令执行、响应/错误帧
                  #   （含 topic 信封）；不再持有 TcpListener / tungstenite
events.rs         # 会话输出/退出 broadcast 广播器；commands.rs 与 net/ws.rs 共同订阅
net/mod.rs        # router 增 .route("/ws", get(ws::handle_ws))；NetState 增 terminal 字段
```

- 启动：`server` 节 enabled 时 `run_server` 启动 axum（路由含 `/ws`）；WS 不独立启动、无独立端口。
- 会话输出不再由 commands.rs 独占：读线程 → mpsc → 一个共享任务转发到 `broadcast`；桌面监听（当前 tabs）与 ws 连接各自订阅。
- `/ws` 连接生命周期：auth_middleware（token 白名单，`?token=` query）→ `on_upgrade` → 请求/事件双路 select；断连清理该连接订阅。

### 2. WebSocket 协议（JSON 文本帧，信封化）

所有帧带 `topic` 信封；v1 仅 `"terminal"` 业务。

| 方向 | topic | type | 载荷 |
|---|---|---|---|
| c→s | terminal | `spawn` | `{ cwd?, shell?, cols?, rows? }` |
| c→s | terminal | `write` | `{ sessionId, data: base64 }` |
| c→s | terminal | `resize` | `{ sessionId, cols, rows }` |
| c→s | terminal | `kill` | `{ sessionId }` |
| c→s | terminal | `list` | —— |
| s→c | terminal | `spawned` | `{ sessionId }` |
| s→c | terminal | `output` | `{ sessionId, data: base64 }` |
| s→c | terminal | `exit` | `{ sessionId, exitCode }` |
| s→c | terminal | `list` | `{ sessions: SessionInfo[] }` |
| s→c | terminal | `error` | `{ message }` |

示例：`{"topic":"terminal","type":"spawn","cwd":"/tmp"}` → `{"topic":"terminal","type":"spawned","sessionId":"term-0001"}`。

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
  wsTransport(url): TerminalTransport    // 浏览器；topic="terminal" 信封 + base64 编解码 + 自动重连
```

- `TerminalPanel.svelte`：`const transport = isTauriEnv ? ipcTransport() : wsTransport(wsUrlFromConfig())`。
- **WS 地址推导**：`wsUrlFromConfig()` = 远程连接配置 `pulsar:remoteUrl`（如 `http://192.168.1.10:8787`）→ 协议换 ws + `/ws` + token query；缺省地址 `http://127.0.0.1:8787`。不再使用 `VITE_TERMINAL_WS_URL` / `43110`。
- 禁用提示替换为状态栏（连接中/已断开 + 重试），tab 栏与 xterm 逻辑不随 transport 变化。

## 安全

| 场景 | 护栏 |
|---|---|
| 桌面 IPC | 不变（tauri 命令级权限） |
| WebSocket `/ws` | 与 HTTP RPC 共用 `server.tokens` 白名单（`auth_middleware`，token 走 `?token=`）；白名单为空 → 放行（默认场景）；非空 → 必须携带白名单内 token。监听地址由 `server.host` 决定（默认 127.0.0.1） |
| agent `visible_terminal` | 不变（走既有护栏） |

## 实现要点（里程碑）

1. **W1（后端）**：Cargo `tokio-tungstenite` 降为 dev-dependency；输出事件 broadcast 重构（commands.rs 改订阅）；`terminal/ws.rs` 重构为纯业务 handler（帧协议 + 命令执行，含 topic 信封）；`net/ws.rs` 通用 WS 接入（axum on_upgrade + topic 分发 + 事件转发）；`net/mod.rs` NetState 增 terminal 字段 + router 加 `/ws`；`config.rs` 删 `terminal_ws` 节；`lib.rs` 接线；`cargo check`。
2. **W2（前端）**：`transport.ts` wsTransport 加 topic 信封；TerminalPanel WS 地址从远程连接配置推导（删 `VITE_TERMINAL_WS_URL` / 43110）；`pnpm check`。
3. **W3（浏览器验证 + 字体修复）**：`tauri dev` 与 vite 同跑，chromium/firefox 打开浏览器终端真实交互；复现桌面字母间距问题（webkit/chromium 对照），定位根因（cell 测量 / 字体解析 / 渲染器）并修复（候选：单字体栈、DOM renderer、字号/dpr 修正）；全量 check + 冒烟。

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是（浏览器可用性 + 终端字体渲染质量，均属终端面板体验闭环）。
- 若否，偏差在哪里：无。
- 是否需要调整本轮目标或范围：架构修订已按用户要求落地（公共服务 + 同端口 + 前端推导），无其他偏差。

## Open Questions

- [x] v1 形态确认：WS 挂载 axum `/ws`（同端口）、浏览器需同机桌面进程在跑（远程独立网关 v2）→ 已确认。
- [x] 鉴权：WS 复用 `server.tokens` 白名单（`?token=` query，与 SSE 一致）→ 已确认并入实现。
- [x] 端口策略：HTTP 与 WS 同端口合并（axum 路径区分）→ 用户选择「同端口合并」。
- [x] 前端 WS 地址：从远程连接配置（`pulsar:remoteUrl`）自动推导 → 用户选择「从远程连接推导」。

## Change Log

- 2026-08-20：初稿。背景 = 浏览器禁用终端 + 桌面端字母间距异常；方案 = 内嵌 WS 网关复用 TerminalManager + 前端 transport 抽象 + 浏览器环境复现字体问题。待用户确认 Open Questions 后批准执行。
- 2026-08-20（修订 1）：端口与监听地址由环境变量改为 `config.json` `terminal_ws` 节（`host`/`port`，默认 `127.0.0.1:43110`）；host 可改绑非回环（如 `0.0.0.0`）以支持局域网访问，安全表同步标注改绑即暴露给所在网段。
- 2026-08-20（修订 2，架构重构）：按用户要求——WS 升级为**公共服务**（帧带 `topic` 信封按业务分发，终端为第一业务）；WS 从独立 tokio-tungstenite 服务**迁入 axum 同端口 `/ws`**（与 HTTP RPC 一个 bind、一套 token 鉴权）；`terminal_ws` 配置节废弃，WS 随 `server` 节启用；前端 WS 地址从远程连接配置（`pulsar:remoteUrl`）自动推导，删除 `VITE_TERMINAL_WS_URL` / `43110`；`tokio-tungstenite` 降为 dev-dependency（测试客户端）。

## Validation

- Self-check：复用既有 `TerminalManager`/事件名，桌面路径零改动，改动面收敛在增量通道（net/ws.rs + terminal/ws.rs 改造 + 前端推导）。
- Static checks：`cargo check --all-targets`、`pnpm check`。
- Runtime / Test：`cargo test --lib`（broadcast 转发、协议编解码、/ws 端到端用例）；手动冒烟——tauri dev + 浏览器同跑，两端共享会话交互；token 白名单场景下浏览器无 token 应握手失败；chromium/firefox 下终端字体间距恢复正常或定位到 WebKitGTK 特有根因。
- Human confirmation：用户批准本 spec 后进入实现（架构修订已获用户决策确认）。
- 结果汇总：未执行，待批准。
- 剩余风险：WS 协议错误处理（非法帧/超大帧）、断线重连竞态、浏览器并发写同一会话；broadcast 背压（高频输出 v1 沿用 chunk 聚合）。
