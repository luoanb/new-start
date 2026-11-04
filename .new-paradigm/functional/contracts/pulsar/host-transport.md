# host-transport 代码契约

> 文档状态：待确认
> 契约标识：CTR-PSR-005
> 所属领域：pulsar（Pulsar 插件宿主与前端插件体系；`PSR` 代号由 CTR-PSR-001 声明）
> 上位依据：《代码契约规则》
> 适用范围：宿主对浏览器暴露的 HTTP 端点、事件流与启动图
> 下位产物：实现代码、契约一致性检查结果

## 职责边界

负责：

- 提供固定的内建 HTTP 端点：健康检查、宿主状态、启动图、事件流、RPC 调用、浏览器运行时脚本与入口页面。
- 维护「宿主 → 浏览器」的单向事件流。
- 生成浏览器启动图，并保证页面级共享依赖的全局唯一性。
- 把路由分发与处理器异常转换为统一的 HTTP 响应。

不负责：

- 插件自身贡献的路由。它们由适配器通过宿主提供的受限上下文注册。
- 插件生命周期与状态变迁。本能力只读取宿主状态。
- 事件的产生原因。事件类型与载荷由产生方决定，本能力只做转发。
- 浏览器侧的消费与重连策略。该职责属于前端插件体系。

## 对外操作

`GET /health -> text/plain`

- 输出语义：固定返回 `ok`，状态码 200。
- 操作级错误：无。

`GET /api/state -> HostState`

- 输出语义：返回宿主状态快照，字段语义见 `plugin-host` 契约中的 `PluginState` 与 `HostState`。
- 操作级错误：无。

`GET /api/boot -> BootGraph`

- 输出语义：返回当前启动图。只包含已持有 fiber 且已产出 Client 描述的条目。
- 操作级错误：共享依赖冲突时抛错，由统一错误处理转为 500。

`GET /api/events -> text/event-stream`

- 输出语义：建立 SSE 连接，先写一行注释帧 `: connected`，随后持续下发事件帧。
- 帧格式固定为 `data: <JSON>\n\n`，JSON 为 `{ "type": string, "payload": unknown }`。
- 连接关闭时从订阅集合中摘除。
- 操作级错误：无。

`POST /api/rpc -> { result: unknown }`

- 输入条件：请求体为 JSON，含字符串 `method`；HTTP 方法必须为 `POST`。
- 输出语义：调用宿主 RPC 注册表中的同名方法，返回 `{ result }`。
- 操作级错误：非 POST 返回 405 与 `{ error: "method not allowed" }`；缺少 `method` 返回 400 与 `{ error: "missing method" }`；方法不存在或执行抛错返回 500 与 `{ error: <message> }`。

`GET /client/runtime.js -> application/javascript`

- 输出语义：返回宿主自带的浏览器运行时脚本原文。
- 操作级错误：无。

`GET / -> text/html`

- 输出语义：返回入口页面。页面只包含一个内联 `importmap` 与一个加载 `/client/runtime.js` 的模块脚本。
- 操作级错误：无。

`BootGraph`

- `rev`：启动图版本号，字符串形态的整数。
- `plugins`：`{ id, client }` 数组。
- `runtimes`：适配器名到浏览器侧入口 URL 的映射，只含声明了入口的适配器。
- `imports`：模块名到 URL 的全局映射，由参与本次启动图的适配器合并去重。

`ClientDescriptor` 由执行环境适配器能力统一定义，本契约只规定其被原样写入启动图，不重复定义字段。

## 错误

除 RPC 的三种显式状态码外，当前实现不定义稳定错误标识：

- 路由未命中返回 404 与 `{ error: "not found" }`。
- 处理器抛错时，若响应头尚未写出则返回 500 与 `{ error: <message> }`；若已写出则直接结束响应。

## 整体保证

- 端点路径固定。上述七个路径不受插件装配结果影响，始终可用。
- 启动图只描述可执行条目。持有 fiber 且产出 Client 描述的条目才进入 `plugins`。
- 共享依赖全局唯一。`imports` 中同一模块名只能对应一个 URL；出现同键不同值时整个启动图生成失败，而不是静默取其一。
- 同一启动图内 `runtimes` 与 `imports` 都按适配器去重，不随插件数量重复。
- `rev` 只在宿主状态发生变化时递增：条目装载、卸载、启停、以及适配器上报的 Client 变更。单纯的读取不改变 `rev`。
- 事件流是单向且无状态的。服务端不保存历史帧，新连接只能收到建立之后产生的事件。
- 事件帧与 RPC 响应都不携带调用方身份或会话信息，宿主不区分订阅者。

## 按需扩展

路由匹配语义：

- 内建端点以「精确路径」注册；插件的静态产物以「前缀」注册，最长前缀优先。
- 路由注册时的方法名不参与匹配。分发只比较路径，方法判断由各处理器自行完成。因此 `/api/rpc` 对任意 HTTP 方法都会进入同一处理器，再由处理器返回 405。

已知现状：

- 事件流没有序号、版本号与重放。断线重连后无法判断是否漏帧，也无法请求补发。
- 事件流的版本语义与启动图的 `rev` 没有关联。两个数值互不约束，调用方无法用它们对齐「事件对应哪一版启动图」。
- `/client/runtime.js` 直接读取源文件返回，未设置缓存头。
- 入口页面的 `importmap` 在每次请求时即时生成，可能与随后请求的 `/api/boot` 属于不同 `rev`。

取证范围：

- `packages/pulsar-host/src/host.ts`（端点注册、分发与响应）
- `packages/pulsar-host/src/router.ts`（精确与前缀匹配）
- `packages/pulsar-host/src/rpc.ts`（RPC 注册与调用）
- `packages/pulsar-host/src/events.ts`（SSE 订阅与广播）
- `packages/pulsar-host/src/types.ts`（`BootGraph`、`HostState`）

待决策项：

- 事件流是否需要序号与断线重放，以及是否与 `rev` 对齐。
- HTTP 错误是否需要稳定错误标识，而不是自由文本。
- 入口页面与 `/api/boot` 是否应保证同一 `rev`。
