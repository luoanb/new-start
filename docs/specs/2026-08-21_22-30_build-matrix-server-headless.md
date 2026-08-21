# 构建矩阵：服务器版 + App 内置服务器（静态托管可选）

## 背景

当前 pulsar-app 是单一 Tauri GUI，内嵌网络服务（`config.json` `server` 节控制启停，仅 API：RPC/SSE/WS）。需求：通过构建方式组合出两种部署形态：

1. **服务器版**：headless 独立可执行程序，无 GUI，只跑网络服务，并托管前端静态资源（远程浏览器一个地址全功能）。
2. **App 内置服务器版**：Tauri GUI 正常，内嵌 server，**可选是否**托管前端静态资源。

## 现状事实

- `lib.rs::run()` 完成 Gateway 初始化 + 条件启动 `net::run_server`（`lib.rs` L1643-1730）。
- `TerminalEventHub::new(app: AppHandle)` 依赖 Tauri；`new_for_test()` 无 AppHandle 但 `#[cfg(test)]`。
- `app_log::init` 的 emit 参数可传 None（`pulsar-cli` 已有先例）。
- `rust-embed = "8"` 已在项目使用（`core/insert_catalog.rs`）。
- 前端为纯静态 SPA（adapter-static，fallback index.html），构建输出 `build/`。
- `net::router` 目前对全部路由挂全局 `auth_middleware`（L51-54）。

## 方案

### 1. 共享服务器启动核心

在 lib crate 抽出公共初始化（供 GUI 与 headless 复用，避免行为分叉）：

```
build_server_state(storage_root, terminal_hub) -> Result<NetState>
```

含：ConversationStore → Gateway → StateEmitter（headless 用空实现 + broadcast）→ TerminalManager → NetState。`neuron bootstrap / poller` 等后台任务由调用方按入口决定（GUI 维持现状；headless 启动 bootstrap + poller）。

### 2. 静态资源服务（Cargo feature `embed-static`）

- 新增 `src/net/static_assets.rs`：

```rust
#[derive(rust_embed::Embed)]
#[folder = "../build"]
struct FrontendAssets;   // 编译期嵌入 build/
```

- `net::router` 在 feature 开启时追加 SPA fallback：按 path 从内嵌资源取文件（带正确 content-type），未命中回退 `index.html`（history 路由）。
- **鉴权作用域收紧**：`auth_middleware` 从全局 `.layer()` 改为仅作用于 API 子路由（`route_layer`），静态资源免鉴权（否则浏览器首屏拿不到 JS/CSS）。

### 3. headless server 入口

- 新增 `src/bin/pulsar-server.rs`：tokio runtime + 公共初始化 + `net::run_server`，无 Tauri/GTK 依赖。
- 配置来源：`config.json` `server` 节（与 GUI 一致），CLI `--host / --port / --token` 可覆盖。
- `TerminalEventHub` 新增非 test 构造 `new_headless()`（`app = None`，去掉 `cfg(test)` 限定）。
- 服务器版**固定**启用静态托管（单端口全功能是部署版价值所在）。

### 4. 构建矩阵

| 产物 | 命令 | GUI | server API | 静态托管 |
|---|---|---|---|---|
| pulsar-app（桌面） | `pnpm tauri build`（默认） | ✅ | 按 config | 可选（feature `embed-static`） |
| pulsar-app + 静态 | `pnpm tauri build --features ...`（透传 feature） | ✅ | 按 config | ✅（`embed-static`） |
| pulsar-server | `cargo build --release --bin pulsar-server` | ❌ | 恒启动 | ✅（固定） |

- Tauri CLI 的 feature 透传：`tauri build` 支持 `--features`，或直接 `cargo build --bin pulsar-app`（Tauri 2 支持纯 cargo 构建）。
- 静态资源嵌入两份互不影响：GUI WebView 用 Tauri 内嵌副本；server（无论 GUI 内还是 headless）用 rust-embed 副本。

## 边界

- 不改前端代码、不改 RPC/SSE/WS 协议。
- 不改 `net::run_server` 的 API 行为；静态托管是叠加项。
- 编译期要求 `build/` 存在（Tauri `beforeBuildCommand` 已保证 GUI 路径；headless 单独构建需先 `pnpm build`）。

## 风险

| 风险 | 缓解 |
|---|---|
| `rust-embed` 编译期找不到 `../build` 导致构建失败 | feature 门控 + 文档注明先构建前端；`build.rs` 不做运行期探测 |
| Gateway 初始化抽取改变 GUI 行为 | 抽取为纯搬移，GUI 入口回归验证（跑一遍现有测试 + 启动冒烟） |
| `route_layer` 收紧鉴权遗漏静态路径 | 静态路径不挂鉴权；API 四条路由逐一核对 |

## Done Contract

- [x] `net/static_assets.rs` + feature `embed-static`：router 可托管 SPA 前端，`/healthz` 等 API 不受影响
- [x] `pulsar-server` headless binary 可启动并 `curl /healthz`、`curl /`（HTML）通过
- [x] `TerminalEventHub::new_headless()` 就绪，WS `/ws` 终端在 headless 下可用
- [x] GUI 构建 + 现有 `cargo test` 全绿（无回归）
- [x] 构建矩阵三形态产物均可出包

## 实施记录（2026-08-21）

- 新增 [src/net/static_assets.rs](../../packages/pulsar-app/src-tauri/src/net/static_assets.rs)：rust-embed 嵌入 `../build/`，SPA fallback（未命中回退 index.html），`no-store` 缓存头。
- [net/mod.rs](../../packages/pulsar-app/src-tauri/src/net/mod.rs)：`auth_middleware` 由全局 `.layer()` 改为 API 子路由 `route_layer`；`embed-static` 开启时叠加 `fallback_service(spa)`。
- 新增 [src/server_runtime.rs](../../packages/pulsar-app/src-tauri/src/server_runtime.rs)：`build_server_runtime` 公共初始化（Gateway + 分域服务 + 终端），GUI（`lib.rs::run`）与 headless 复用。
- [terminal/events.rs](../../packages/pulsar-app/src-tauri/src/terminal/events.rs)：新增 `new_headless()`（无 AppHandle，仅 WS 广播）。
- 新增 [src/bin/pulsar-server.rs](../../packages/pulsar-app/src-tauri/src/bin/pulsar-server.rs)：headless 入口，CLI `--host/--port/--token` 覆盖 `config.json` `server` 节；storage_root = cwd 下 `.pulsar`。
- [Cargo.toml](../../packages/pulsar-app/src-tauri/Cargo.toml)：`embed-static` feature；`pulsar-server` 声明 `required-features = ["embed-static"]`（固定静态托管）。

冒烟验证（release + embed-static，`/tmp/pulsar-server-smoke`）：

```
/healthz                    200 "ok"
/ (index.html)              200 (SvelteKit HTML)
/some/client/route          200 (SPA fallback)
/manifest.webmanifest       200
POST /rpc {"cmd":"status"}  200 {"ok":true,...}
/ws 握手                    101
--token s3cret 时：
/rpc 无 token              401     /rpc Bearer s3cret   200
/events 无 token           401     / (静态) 无 token    200
```

构建命令：

```
# 桌面版（GUI + 按 config 启停 server，API only）
pnpm tauri build
# 桌面版 + server 静态托管
pnpm tauri build --features embed-static     # 或 cargo build --features embed-static
# 服务器版（headless，固定静态托管）
cargo build --release --bin pulsar-server --features embed-static
```

## Resume / Handoff

实现顺序：① feature + static_assets + router 改造 → ② 抽公共初始化 → ③ pulsar-server binary → ④ TerminalEventHub headless 构造 → ⑤ 构建验证。
