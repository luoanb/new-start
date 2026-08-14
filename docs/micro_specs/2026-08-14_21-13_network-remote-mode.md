# Spec: 网络远程通道（桌面内嵌 HTTP Server + 前端双模式）

- 日期：2026-08-14
- 状态：待评审
- 影响包：`packages/pulsar-app`（Svelte 前端 + Tauri/Rust 后端）

## 1. 背景与需求

当前前端（Svelte webview）与后端（Rust core）的唯一通信方式是 **Tauri IPC**：`invoke()` 调用 `#[tauri::command]`（54 个，注册于 `lib.rs` 的 `invoke_handler`），后端通过 `app://state-changed` 事件推送状态变更。该方式绑定桌面进程内，浏览器或其他设备无法访问后端能力。

需求：

1. **保持现状**：Tauri IPC 继续作为默认通信方式，行为完全不变。
2. **新增网络通道**：在 Rust core 之上新增网络适配器，让同一套后端能力能通过网络协议被访问。
3. **前端双模式**：前端支持**本机模式**（Tauri IPC）与**远程模式**（网络通信），切换对业务层透明。
4. **配置开关**：内嵌服务通过配置文件（`.pulsar/config.json`）控制是否开启，默认关闭。

## 2. 现状分析

| 层 | 现状 |
| --- | --- |
| 前端数据层 | [dataStore.svelte.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/stores/dataStore.svelte.ts)：单例 store，直接依赖 `invoke()` 与 `listen()`；`bootstrap()` 全量拉取 + `app://state-changed` 事件增量刷新 |
| 后端命令层 | [lib.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/lib.rs#L838-L894)：54 个 `#[tauri::command]` 全部注册在 `invoke_handler`，按 Chat / Info / Topic / Poller / Neuron / Logs 分组 |
| 后端门面 | [gateway.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/gateway.rs)：`Gateway` 业务门面 + 分域 Tauri State（`Arc<NeuronManager>` / `Arc<Mutex<TopicStore>>` / `SessionTracker` / `ProviderRegistry` 等），命令层只做薄适配 |
| 事件抽象 | [events.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/events.rs)：`StateChange` 枚举（serde `tag = "kind"`）+ `StateEmitter = Arc<dyn Fn(StateChange)>`，已从 Tauri emit 解耦，可直接复用 |
| 配置体系 | [config.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/config.rs)：`AppConfigFile` 读写 `.pulsar/config.json`，顶层按领域分键（`poller` / `neuron`），未建模键经 `extra` 无损保留 |
| 架构原则 | [architecture.md](file:///home/lab/Documents/trae_projects/new-start-wt/docs/pulsar/architecture.md)：业务只在 core 实现一次，Tauri Commands / CLI / TUI 均为入口适配器 → 网络通道即第四个适配器，不触碰 core |

### 核心约束

- 本机路径零改动：`server` 配置缺省关闭时，行为与今天完全一致。
- 网络路径同样遵守架构硬规则：**不持 Gateway / 域锁跨网络 I/O**（Clone-out then await）。
- 命令参数 / 返回的序列化结构需提取共享，避免网络层与 Tauri 层重复定义。
- 不引入独立 server bin、gRPC、WebSocket、用户体系（本期范围外）。

## 3. 已确认的关键决策

| 决策点 | 结论 |
| --- | --- |
| API 风格 | **统一 RPC 端点** `POST /rpc`，载荷 `{ cmd, params }`，与 54 个 command 一一对应 |
| 事件推送 | **SSE** `GET /events`，复用 `StateChange` 序列化格式（前端 handler 可复用） |
| 部署形态 | **桌面内嵌 server**：Tauri 进程内按配置启动 axum HTTP server |
| 鉴权 | 后端维护 **token 白名单**：远程请求须携带列表内 token；本机访问（`127.0.0.1`）默认免鉴权 |
| 配置开关 | `.pulsar/config.json` 顶层 `server` 键控制是否开启，默认 `enabled: false` |

## 4. 后端设计

### 4.1 配置节扩展（config.rs）

对齐 `PollerSection` / `NeuronSection` 模式，新增顶层 `server` 键：

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerSection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,   // 是否启动内嵌 HTTP server；缺省 = false
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,    // 默认 127.0.0.1；跨机访问需显式改绑
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,       // 默认 8787
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<Vec<String>>, // 远程访问白名单 token
}
```

- `AppConfigFile` 增加 `pub server: Option<ServerSection>`。
- 无 `server` 键或 `enabled != true` → 不启动，等价现状。
- `tokens` 为空 + `host` 为 `127.0.0.1` → 仅本机免鉴权访问。

### 4.2 内嵌 HTTP Server（新增 `net` 模块）

- 位置：`src-tauri/src/net/`（`rpc.rs` + `sse.rs` + `auth.rs` + `mod.rs`）。
- 依赖：axum（Tauri v2 已内置 tokio）。
- 生命周期：在 [lib.rs run() setup](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/lib.rs#L751-L753) 中读取 `server` 节；未启用直接跳过；启用则 `tauri::async_runtime::spawn` 启动，持有已 clone 的分域 State 与 `StateEmitter`。

端点契约：

| 端点 | 说明 |
| --- | --- |
| `POST /rpc` | 载荷 `{ cmd, params }`；分发到对应 command 薄封装，返回统一响应 `{ ok: true, data }` 或 `{ ok: false, error: { code, message } }` |
| `GET /events` | SSE 流：`StateChange` 序列化后按 `app://state-changed` 同格式推送（事件名 + payload），前端 handler 零改写 |
| `GET /healthz` | 存活检查（供前端探测/重连） |

鉴权中间件（`auth.rs`）：

- 请求源地址为 `127.0.0.1` / `::1` 且 `tokens` 为空 → 放行（本机免鉴权）。
- 其余请求要求 `Authorization: Bearer <token>`，token ∈ `tokens` 白名单；不匹配 → `401`。
- 监听地址非 loopback 时，所有请求一律要求 token。

### 4.3 命令分发（rpc.rs）

- 将 lib.rs 中 54 个 command 的参数 / 返回结构提取为共享 serde 类型（`net::rpc::*`），Tauri 命令层与 RPC 层共同引用，杜绝双份定义。
- `POST /rpc` 按 `cmd` 名分发到对应处理函数；处理函数内部逻辑与 lib.rs command 保持一致（调用 `Gateway` / 分域 State → 成功后 `state_emit(StateChange)`）。
- 长命令（`send_chat_message` / `call_model` / `converse_session`）在网络路径同样 `async` 化，遵守"不持锁跨网络 I/O"。

## 5. 前端设计

### 5.1 统一 API 客户端抽象（新增 `src/lib/api/`）

把 `dataStore` 对 `invoke` / `listen` 的直接依赖收口到接口：

```
src/lib/api/
  types.ts        // ApiClient 接口：invoke<T>(cmd, params) + subscribe(handler) + health()
  tauriClient.ts  // 本机模式：现有 invoke + listen（行为与今天完全一致）
  httpClient.ts   // 远程模式：fetch POST /rpc + EventSource /events + /healthz
  index.ts        // 按连接配置选择客户端实例
```

接口形态：

```ts
export interface ApiClient {
  invoke<T>(cmd: string, params?: Record<string, unknown>): Promise<T>;
  subscribe(handler: (payload: StateChangePayload) => void): () => void;
  health(): Promise<boolean>;
}
```

> 实现注记（2026-08-14 落地时确认）：原生 `EventSource` **无法设置自定义请求头**，
> SSE 通道的 token 经 query 参数传递（`GET /events?token=...`）；后端 auth 中间件
> 同时接受 `Authorization: Bearer <token>`（POST /rpc）与 `?token=`（GET /events），
> 命中白名单任一即放行。token 建议使用字母数字，避免 percent-encode 歧义。

### 5.2 dataStore 收口

- `invoke(...)` → `api.invoke(...)`；`listen(STATE_CHANGED_EVENT, ...)` → `api.subscribe(...)`。
- 本机模式由 `tauriClient` 承载，行为与今天逐字节一致，回归零风险。
- `bootstrap()` 流程不变，仍为全量拉取 + 事件增量刷新。

### 5.3 模式与连接配置

- 连接配置存 `localStorage`：`pulsar:connMode`（`local` | `remote`）、`pulsar:remoteUrl`、`pulsar:remoteToken`。
- 启动时由 `api/index.ts` 解析配置决定客户端实例；`dataStore` 无感知。
- 运行时切换模式需重新 `bootstrap()`（首启加载时应用配置）。
- 连接入口：可在设置区增加最小配置 UI（模式选择 + 地址 + token 输入），本期可先用 localStorage 直写 + 手动验证，UI 视需要补。

## 6. 实施步骤（拆分）

1. **后端配置**：`ServerSection` + `AppConfigFile.server` + 单测（缺省关闭 / enabled 解析 / tokens 序列化）。
2. **后端 RPC 层**：共享 serde 类型提取（`net::rpc`）→ `POST /rpc` 分发 → `GET /events` SSE → `GET /healthz` → 鉴权中间件。
3. **后端接线**：setup 读取配置、条件启动 server、注入分域 State 与 `StateEmitter`。
4. **前端抽象**：`api/` 四文件 + `dataStore` 收口（invoke/listen → api）。
5. **远程客户端**：`httpClient`（fetch + EventSource 重连 + token 头）。
6. **验证**：
   - 本机模式回归（`server.enabled` 缺省 / false：桌面功能全量正常）。
   - 远程模式：`enabled: true` + 本机 token 访问 `http://127.0.0.1:8787`；浏览器 `EventSource` 收到状态变更。
   - 鉴权：错误 token 401、无 token 401、本机免鉴权放行。
   - 跨机：改绑 `0.0.0.0` + token 白名单访问。

## 7. 风险与边界

- **本机回归**：配置缺省关闭 → server 完全不启动，Tauri IPC 路径零改动，风险最低。
- **长命令超时**：网络请求需合理超时（fetch 无默认超时），超时语义与取消需明确（本期先返回超时错误，不做流式进度）。
- **端口冲突**：绑定失败时记录错误日志并回退为关闭状态（不阻断应用启动）。
- **SSE 断线**：`EventSource` 自动重连；重连后由前端按需 `bootstrap` 或依赖事件增量补齐。
- **并发与锁**：网络路径遵守"Clone-out then await"，不持 Gateway / 域锁跨网络 I/O。
- **安全**：默认仅监听 loopback；非 loopback 一律要求 token 白名单；不引入加密传输（HTTPS 超出本期范围，远程部署需前置反代）。
- **明确不做**：独立 server bin、gRPC / WebSocket、用户体系 / 登录、HTTPS、流式模型输出通道。
