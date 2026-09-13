# Spec: 连接设置弹窗样式微调 + 后端连接服务启停开关

- 日期：2026-09-13
- 状态：待评审（Execution Approval: Pending）
- 影响包：`packages/pulsar-app`（Svelte 前端 + Tauri/Rust 后端）

## Goal

- 要解决什么问题：
  1. 连接设置（ConnectDialog）与设置（SettingsDialog，含主题）弹窗标题字重过重。
  2. 连接设置弹窗宽度偏窄，内容拥挤。
  3. 连接设置需要新增一个开关，用于**实时启停桌面后端的连接服务**（内嵌 axum HTTP server，供远程模式接入）。
- 验收结果：
  - 两个弹窗标题字重随设计令牌（DESIGN.md：弹窗标题 `--fs-lg` 字重 500）。
  - 连接设置弹窗宽度放宽。
  - 桌面端（Tauri）打开连接设置可见开关；切换后立即启动/停止内嵌服务，并写入 `config.json` 的 `server.enabled`，重启后保持。

## Done Contract

- 什么算完成：三项改动落地；后端启停命令可被前端调用；开关状态与服务实际运行状态一致。
- 由什么证明：`cargo build`（或 `cargo check`）+ 前端 `svelte-check`/构建通过；人工在桌面端验证「开→远程可访问、关→不可访问」。
- 哪些情况仍算未完成：仅写 config 而未实时启停；或开关仅桌面端 GUI 显示但后端命令不可用。

## Scope

- In：
  - ConnectDialog / SettingsDialog 标题字重：600 → 500。
  - ConnectDialog 弹窗宽度：420px → 520px。
  - 后端 `net` 模块支持运行时启停内嵌 server（graceful shutdown），新增 IPC 命令 `set_server_enabled`。
  - `server_info` 反映运行时实际状态。
  - 前端 ApiClient 增加 `setServerEnabled`；ConnectDialog 增加 Toggle（仅 Tauri 显示）；i18n（en/zh）。
- Out：
  - 浏览器远程模式下控制对端服务；监听地址/端口/token 的 UI 编辑（仍走 config.json）。
  - HTTPS、用户体系、服务自动重启等超出本期范围。

## Facts / Constraints

- 已确认事实（见 [network-remote-mode spec](file:///home/lab/Documents/trae_projects/new-start-wt/docs/micro_specs/2026-08-14_21-13_network-remote-mode.md)）：
  - 内嵌 server 由 [lib.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/lib.rs#L1860-L1946) setup 阶段按 `config.json` `server.enabled` 条件启动，`net::run_server` 一次性 `axum::serve(...).await` 常驻。
  - `ServerInfo { version, enabled, host, port, static_enabled, auth_required }` 由桌面 IPC `server_info` 与远程 `GET /api/config` 共用。
  - [config.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/infra/config.rs#L104-L119) 已有 `ServerSection { enabled, host, port, tokens }`，`ConfigStore::update` 原子读改写并保留未建模键。
  - 覆盖链：env(`PULSAR_HOST/PORT/TOKEN`) > config `server` 节 > 内置默认。
- 技术/业务约束：
  - 本机 Tauri IPC 路径零改动；开关只影响内嵌 server。
  - 后端 `state_emit` 目前以启动期 `server_enabled` 常量决定是否转发到 SSE 广播通道；运行时启停后该门控必须改为「始终转发」或读共享状态。
  - 架构硬规则：不持 Gateway / 域锁跨网络 I/O。
- 已知风险：
  - 端口占用：启用时 bind 失败需把错误回传前端，且不留下「config=true 但未运行」的不一致。
  - 反复启停的句柄管理（避免重复监听 / 泄漏任务）。

## Open Questions

- [x] 开关语义：**实时启停 + 持久化**（已确认）。
- [x] 显示范围：**仅桌面端 Tauri**（已确认）。

## Restated Understanding

- 我理解当前任务是：在连接设置弹窗里加一个开关，实时启停桌面后端的内嵌连接服务，并持久化到 config.json；同时把两个弹窗标题字重调轻、连接设置弹窗加宽。
- 当前核心目标是：让用户无需手改 `config.json`、无需重启，即可在桌面端开关远程接入服务。
- 当前边界是：仅桌面端 Tauri；不改监听 host/port/token 的编辑方式。
- 暂不处理：远程模式控制对端服务、HTTPS、自动重启。

## 接口契约设计

后端（新增 `net/control.rs`，`net` managed state）：

```rust
/// 内嵌 server 运行时控制器（Tauri managed state: Arc<ServerController>）。
pub struct ServerController {
    storage_root: PathBuf,
    gateway: Gateway,
    state_emit: StateEmitter,
    events_tx: broadcast::Sender<StateChange>,
    terminal: Arc<TerminalManager>,
    terminal_hub: TerminalEventHub,
    inner: Mutex<Option<RunningServer>>, // 运行句柄；None = 未运行
}

struct RunningServer {
    shutdown: oneshot::Sender<()>, // 触发 graceful shutdown
    host: String,
    port: u16,
}

impl ServerController {
    /// 依据 env > config > 默认 解析生效的 ServerConfig（不读 enabled）。
    fn resolve_config(&self) -> ServerConfig;
    /// 启动（幂等）：bind → spawn serve(graceful) → 记录句柄 → 持久化 enabled=true。
    /// bind 失败返回 Err，不持久化、不残留句柄。
    pub async fn start(&self) -> Result<ServerInfo, String>;
    /// 停止（幂等）：触发 shutdown → 清句柄 → 持久化 enabled=false。
    pub fn stop(&self) -> Result<ServerInfo, String>;
    /// 运行状态 + 生效 host/port/tokens 组合为 ServerInfo。
    pub fn info(&self) -> ServerInfo;
    pub fn is_running(&self) -> bool;
}
```

Tauri 命令：

```rust
#[tauri::command]
async fn set_server_enabled(
    controller: State<'_, Arc<ServerController>>,
    enabled: bool,
) -> Result<ServerInfo, String>; // true→start, false→stop；返回最新 ServerInfo

#[tauri::command]
fn server_info(controller: State<'_, Arc<ServerController>>) -> ServerInfo; // 改为反映运行时状态
```

前端 ApiClient：

```ts
interface ApiClient {
  // ...existing
  setServerEnabled(enabled: boolean): Promise<ServerInfo>;
}
// tauriClient: invoke("set_server_enabled", { enabled })
// httpClient: throw new Error("本机后端服务启停仅桌面端可用")
```

## Checkpoint Summary

- 当前任务理解：三项改动；1、2 为纯样式（已完成），3 为跨层功能。
- 当前核心目标：桌面端可实时启停内嵌连接服务。
- 当前进度：样式两项已改；item 3 待批准后实施。
- 下一步 1：后端 `net` 拆分 bind/serve 并新增 `ServerController` + `set_server_enabled`，`server_info` 改读运行时状态，`state_emit` 去掉启动期门控。
- 下一步 2：前端 ApiClient 加 `setServerEnabled`；ConnectDialog 加 Toggle（仅 Tauri）；i18n en/zh。
- 涉及文件 / 模块：
  - `src-tauri/src/net/mod.rs`、`src-tauri/src/net/control.rs`（新）、`src-tauri/src/lib.rs`
  - `src/lib/api/{types,tauriClient,httpClient}.ts`
  - `src/lib/components/ConnectDialog.svelte`
  - `src/lib/i18n/translations.ts`
- 风险：端口占用回滚、句柄幂等、state_emit 门控调整。
- 验证方式：`cargo check`；前端构建/`svelte-check`；桌面端手动开/关验证远程可达性变化。
- Execution Approval: `Approved`

## Change Log

- 2026-09-13: 初稿；样式两项（标题字重 600→500、弹窗 420→520px）先行落地。
- 2026-09-13: 用户批准。后端落地 `net/control.rs::ServerController`、`net::{bind_listener, serve_with_shutdown}`、命令 `set_server_enabled`、`server_info` 改读运行时状态、`state_emit` 去掉启动期门控；前端 `ApiClient.setServerEnabled`、`ConnectDialog` 启停开关（仅 Tauri）、`Toggle` 增加 `onchange`、i18n en/zh。

## Validation

- Self-check: 变更点与 spec 一致；样式两项按 DESIGN.md 令牌；开关仅 Tauri 渲染。
- Static checks: `cargo check` ✅（0 error）；`cargo check --tests` ✅；`pnpm check`（svelte-check）✅ 0 error（20 条均为既有文件 warning，与本改动无关）；`pnpm build` ✅。
- Runtime / Test: 未运行桌面端手动验证。
- Human confirmation: 待人工在桌面端确认「开→远程可访问 / 关→不可访问」及弹窗观感。
- 结果汇总：编译与静态检查通过；功能链路已接通。
- 核心目标是否已由证据证明完成：部分——静态证据已具备，运行时人工确认待补。
- 若未完成，当前剩余差距：桌面端手测开关与远程可达性。
- 剩余风险：端口占用时的错误回显观感；`state_emit` 始终推送至广播通道的微小开销（无订阅者时静默失败）。

## Resume / Handoff

- 当前状态：三项改动均已实现并通过静态检查。
- 当前卡点：无（待人工桌面端验收）。
- 下一步唯一动作：在桌面端打开连接设置，切换开关并核对远程可达性与 config.json 持久化。
- 下一轮核心目标：人工验收通过后收尾。
