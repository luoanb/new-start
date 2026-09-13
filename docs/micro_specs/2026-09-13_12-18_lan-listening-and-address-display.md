# Spec: 局域网监听开关 + 运行地址展示/复制

- 日期：2026-09-13
- 状态：待评审（Execution Approval: Pending）
- 影响包：`packages/pulsar-app`（Svelte 前端 + Tauri/Rust 后端）
- 前置：[连接设置弹窗 + 后端服务启停开关](file:///home/lab/Documents/trae_projects/new-start-wt/docs/micro_specs/2026-09-13_12-03_connect-dialog-server-toggle.md)

## Goal

- 要解决什么问题：
  1. 当前内嵌 server 默认只监听 `127.0.0.1`（仅本机），无法从局域网访问；需支持「是否开启局域网监听」开关。
  2. 运行中的连接需要带上协议并可一键复制：形如
     - `Local:   http://localhost:1432/`
     - `Network (eth0): http://192.168.1.4:1432/`（复制的仅为 URL，另附网卡名称）
- 验收结果：桌面端连接设置中可切换局域网监听；开启后展示本机与局域网访问地址（含协议与末尾 `/`），每行可复制；局域网访问自动要求访问令牌。

## Done Contract

- 什么算完成：局域网开关实时生效并持久化（`server.host` = `0.0.0.0` / `127.0.0.1`）；运行态展示 Local + 各网卡 Network 地址并支持复制；开启局域网且白名单为空时自动生成并持久化 token。
- 由什么证明：`cargo check` + 前端构建/`svelte-check` 通过；桌面端人工验证「开关局域网→同网段设备用 Network 地址 + token 可访问；关闭→不可访问」。
- 哪些情况仍算未完成：仅展示地址但无法复制；局域网开启后仍无鉴权（token 泄漏/裸奔）。

## Scope

- In：
  - 后端：`server.host` 复用为监听范围开关（`0.0.0.0` ↔ `127.0.0.1`）；`ServerInfo` 增 `lan` + `lan_addresses`；新增命令 `set_server_lan` 与 `server_access_token`；局域网且白名单为空时自动生成并持久化 token。
  - 前端：连接设置新增「局域网访问」开关；运行态展示 Local / Network(网卡名) 地址行（含协议 + 尾 `/`）+ 复制按钮；token 行（有则显示）+ 复制。
  - i18n en/zh。
- Out：
  - HTTPS / 自定义证书；端口编辑 UI；headless `pulsar-server` 的鉴权改造（仍按现有 env/config 行为）。
  - 多网卡下选择「仅绑定某块网卡」（本期内 `0.0.0.0` 全网卡）。

## Facts / Constraints

- 已确认事实：
  - [auth.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/net/auth.rs#L34-L75) 仅按「白名单是否非空」鉴权，**不按来源地址区分**；`0.0.0.0` + 空白名单 = 同网段任何人可完全控制后端。
  - [ServerSection](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/infra/config.rs#L104-L119) 已有 `enabled/host/port/tokens`；`host` 默认 `127.0.0.1`。
  - [net/mod.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/net/mod.rs) 的 `ServerInfo` 由桌面 IPC 与远程公开端点 `GET /api/config`（免鉴权）共用。
- 技术/业务约束：
  - **token 不得进入 `GET /api/config`**（公开端点），否则泄漏；token 只经桌面 IPC 返回。
  - 覆盖链保持 env(`PULSAR_HOST`) > config `server` 节 > 默认；env 设置时开关状态以 env 为准（UI 反映生效 host）。
  - 端口占用时不得留下「配置已改但未运行」的不一致。
- 已知风险：
  - 切换局域网需重启监听（瞬间断连）；网卡枚举依赖新增依赖 `if-addrs`。
  - 自动生成 token 会改变「0.0.0.0 + 空白名单」的既有行为（安全正向变化）。

## Open Questions

- [x] 局域网绑定方式：**绑 `0.0.0.0`（所有网卡）**（已确认）。
- [x] Network 行展示：**仅 URL 可复制，另附网卡名称**（已确认）。
- [x] 局域网鉴权：**白名单为空时自动生成 token**（已确认）。

## Restated Understanding

- 我理解当前任务是：给内嵌连接服务加「局域网监听」开关，并把运行中的访问地址按 `Local` / `Network(网卡名)` 展示、带协议、可复制；局域网开启时自动保证有 token。
- 当前核心目标是：桌面端一键开放/收回局域网访问，并能直接复制地址去另一台设备连接。
- 当前边界是：仅桌面端；HTTP；`0.0.0.0` 全网卡；不改 headless。
- 暂不处理：HTTPS、自定义 host、端口编辑、单网卡绑定。

## 接口契约设计（2026-09-13 重构：对齐 Linux 服务管理）

职责分层：「配置调整」与「服务启停」分离。

**配置**（`config.json` `server` 节，全部是配置项）：`enabled`（随应用自启，≈ enable）/ `lan` / `port` / `tokens`。

后端 `ServerInfo`（公开端点与 IPC 共用；`token` 仅 IPC）：

```rust
pub struct LanAddress { pub name: String, pub ip: String }

pub struct ServerInfo {
    pub version: &'static str,
    pub running: bool,          // 服务是否正在运行（≈ is-active）
    pub enabled: bool,          // 配置：是否随应用自启（≈ is-enabled）
    pub host: String,           // 运行中的实际绑定（未运行则配置值）
    pub port: u16,
    pub static_enabled: bool,
    pub auth_required: bool,
    pub lan: bool,              // 配置：是否允许局域网访问
    pub lan_addresses: Vec<LanAddress>,
    pub token: Option<String>,  // 仅 IPC 下发；公开端点恒 None
}
```

Tauri 命令（仅 3 个）：

```rust
#[tauri::command] fn server_info(controller: State<'_, Arc<ServerController>>) -> ServerInfo;
/// 配置调整（≈ enable/disable）：只写 config，不改运行态。
#[tauri::command] fn server_config(controller: State<'_, Arc<ServerController>>, patch: ServerConfigPatch) -> Result<ServerInfo, String>;
/// 服务启停（≈ start/stop）：只改运行态，不改配置。
#[tauri::command] async fn server_control(controller: State<'_, Arc<ServerController>>, action: ServerAction) -> Result<ServerInfo, String>;
// ServerConfigPatch { enabled?, lan?, port?, tokens? }；ServerAction = Start | Stop
```

绑定地址由配置 `lan` 派生（`lan` → `0.0.0.0`，否则 `127.0.0.1`；env `PULSAR_HOST` 优先）。

`ServerController`：

```rust
impl ServerController {
    pub fn auto_start(&self) -> bool;                                  // 配置 enabled
    pub fn info(&self) -> ServerInfo;
    pub fn set_config(&self, patch: ServerConfigPatch) -> Result<ServerInfo, String>; // 只落盘
    pub async fn control(&self, action: ServerAction) -> Result<ServerInfo, String>;  // 只改运行态
    // start() 内：非 loopback 且白名单为空 → 生成随机 token 并持久化
}
```

前端 `ApiClient`：

```ts
interface ApiClient {
  serverInfo(): Promise<ServerInfo>;
  serverConfig(patch: ServerConfigPatch): Promise<ServerInfo>;  // Tauri IPC；远程抛 unsupported
  serverControl(action: ServerAction): Promise<ServerInfo>;     // Tauri IPC；远程抛 unsupported
}
```

界面：配置区两个复选项（`随应用启动自动开启`、`局域网访问`）+ 两个按钮（`启动` / `停止`）；运行中展示 `本机` / `局域网(网卡名)` / `访问令牌` 地址行（各带复制）；配置与实际绑定不一致时提示「重启服务后生效」。

## Checkpoint Summary

- 当前任务理解：局域网监听开关 + 运行地址展示/复制 + 局域网鉴权（自动 token）。
- 当前核心目标：桌面端可开关局域网监听并复制可用的访问地址。
- 当前进度：需求与三个决策点已确认；待批准后实施。
- 下一步 1：后端 —— `ServerInfo` 扩展、`set_server_lan` / `server_access_token` 命令、start 内自动生成 token、`if-addrs` 依赖。
- 下一步 2：前端 —— ConnectDialog 局域网开关 + Local/Network/Token 地址行（CopyButton）；ApiClient/i18n。
- 涉及文件 / 模块：
  - `src-tauri/Cargo.toml`、`src-tauri/src/net/{mod.rs,control.rs}`、`src-tauri/src/lib.rs`
  - `src/lib/api/{types,tauriClient,httpClient}.ts`
  - `src/lib/components/ConnectDialog.svelte`
  - `src/lib/i18n/translations.ts`
- 风险：新增依赖 `if-addrs`；局域网切换重启监听；token 生成属行为变化。
- 验证方式：`cargo check`；`pnpm check` / `pnpm build`；桌面端手测开关与跨机访问。
- Execution Approval: `Approved`

## Change Log

- 2026-09-13: 初稿（待批准）。
- 2026-09-13: 用户批准。后端：deps 增 `if-addrs`/`getrandom`；`ServerInfo` 增 `lan`/`lan_addresses`（`LanAddress`），新增 `is_lan_host`/`lan_addresses` 辅助；`ServerController` 增 `set_lan`/`access_token`，`start` 在非 loopback 且白名单为空时自动生成并持久化 token；`set_lan` 启动失败回滚原 host；新增命令 `set_server_lan`/`server_access_token`。前端：`ServerInfo` 类型扩展、`ApiClient` 增 `setServerLan`/`accessToken`；ConnectDialog 增局域网开关 + Local/Network(网卡名)/Token 地址行（CopyButton 仅复制 URL）；i18n en/zh。
- 2026-09-13 修复：局域网关着仍展示 Network 行 —— Network 行未与 `serverLan` 绑定，已用 `{#if serverLan}` 包裹。
- 2026-09-13 修复（用户实测报错 `bind 127.0.0.1:8899 failed: 地址已在使用`）：`stop_inner` 仅发 graceful shutdown 信号便立即 rebind，旧 listener 未销毁导致 `0.0.0.0↔127.0.0.1` 同端口 `EADDRINUSE`。改为 `shutdown_running()` 持有 serve 任务 `JoinHandle`，信号后 `await` 其结束（1s 超时后 `abort` 强制释放端口）再 rebind；`stop`/`set_server_enabled` 随之 async。
- 2026-09-13 修复（用户实测：点击启动后 app 无响应）：`ServerController::start` 持有 `inner`(std::Mutex) 锁未释放便调用 `self.info()`，而 `info()` 再次加同一把锁 → 自死锁；主线程上的同步命令随之全部阻塞，界面无响应。改为作用域内持锁写入，调用 `info()` 前释放锁。
- 2026-09-13 交互调整（用户要求）：服务控制按钮改为**按状态显示单个 + 运行中追加重启**——停止态只显示「启动」；运行态显示「停止」+「重启」。`ServerAction` 增 `Restart`（内部 shutdown_running → start，配置改动即在重启时生效），未新增命令。另为控制/配置操作加**进行中态**（按钮内 spinner + 「处理中…」文案，操作期间禁用）。
- 2026-09-13 排查记录（用户反馈「开启局域网无链接 / 重启无效果」）：实测服务返回 `host=127.0.0.1, lan=false, lan_addresses=[wlo1/192.168.1.4]`，而 config.json 为 `lan:true`。原因是**当前进程仍由旧 dev 启动器拉起**（携带 `PULSAR_HOST=127.0.0.1`），env 覆盖 config，故实际只绑 loopback、重启重绑后仍是 loopback（故看起来无效果）。修复已在上一条（启动器不再注入 PULSAR_HOST），需重启 `pnpm tauri:dev` 生效。
- 2026-09-13 重构（对齐 Linux 服务管理，用户确认）：**配置调整与服务启停彻底分离**。`config.json` `server` 节新增 `lan`（`enabled` 语义改为"随应用自启"）；`ServerInfo` 改为 `running`（是否在跑）/ `enabled`（配置自启）/ `lan`（配置）/ `token`（公开端点恒 None）；命令收敛为 3 个：`server_info` / `server_config`（只落盘，含 enabled/lan/port/tokens 局部更新）/ `server_control`（start/stop，只改运行态）；删除 `set_server_enabled`/`set_server_lan`/`server_access_token`；桌面绑定地址改由 `lan` 派生（不再读 `host`，避免与 `lan` 冲突）；headless bin 同样支持 `lan` 派生绑定。前端 `ApiClient` 改为 `serverConfig`/`serverControl`；ConnectDialog 改为「配置复选项（自启、局域网）+ 启动/停止两个按钮 + 运行地址行」，并提示「配置已改，重启服务后生效」。

## Validation

- Self-check: token 未进入公开 `GET /api/config`（`ServerInfo.token` 仅 IPC 下发，公开端点恒 `None`）；局域网开启自动保证 token；配置与启停互不影响。
- Static checks: `cargo check` ✅；`cargo check --tests` ✅；`cargo check --bin pulsar-server --features embed-static` ✅；`pnpm check` ✅ 0 error（20 条既有 warning）；`pnpm build` ✅。
- 修复后复检：`cargo check` ✅ / `cargo check --tests` ✅ / `pnpm check` ✅。
- 重构后复检：`cargo check` + `cargo check --tests` + `cargo check --bin pulsar-server --features embed-static` + `pnpm check` + `pnpm build` 全通过。
- Runtime / Test: 未运行桌面端手动验证。
- Human confirmation: 待人工确认「开局域网→同网段用 Network 地址 + token 可访问；关→不可访问」「地址可复制，Local/Network 行含协议与尾 `/`」。
- 结果汇总：编译与静态检查通过；链路接通。
- 核心目标是否已由证据证明完成：部分——静态证据具备，运行时人工确认待补。
- 若未完成，当前剩余差距：桌面端手测局域网开关与跨机访问。
- 剩余风险：env `PULSAR_HOST` 设置时 UI 开关受 env 优先；多网卡时全部展示为 Network 行。

## Resume / Handoff

- 当前状态：局域网开关 + 地址展示/复制均已实现并通过静态检查。
- 当前卡点：无（待人工桌面端验收）。
- 下一步唯一动作：桌面端打开连接设置 → 开启服务 → 开局域网 → 复制 Network 地址 + token 到另一台设备验证。
- 下一轮核心目标：人工验收通过后收尾。
