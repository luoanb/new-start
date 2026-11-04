# client-plugin 代码契约

> 文档状态：待确认
> 契约标识：CTR-PSR-007
> 所属领域：pulsar（Pulsar 插件宿主与前端插件体系；`PSR` 代号由 CTR-PSR-001 声明）
> 上位依据：《代码契约规则》
> 适用范围：浏览器侧插件上下文、RPC 客户端、事件订阅、本地化与样式注入
> 下位产物：实现代码、契约一致性检查结果

## 职责边界

负责：

- 定义浏览器侧插件收到的上下文及其固定成员。
- 提供 RPC 调用、事件订阅、本地化词典与样式注入的默认实现。
- 提供插件声明入口与旧组件包装工具。
- 组装上下文时对未提供的成员填充默认实现。

不负责：

- 插槽的登记、渲染与校验。该职责属于 `slot-registry` 契约。
- 插件生命周期与启停。浏览器侧没有宿主式的生命周期管理。
- 事件流的服务端产生与重放。
- 适配器与编译。本能力所在的主入口不得引入任何 Node 依赖。

## 对外操作

`createClientContext(options: ClientContextOptions) -> ClientContext`

- 输入条件：`options.id` 为字符串。
- 输出语义：返回六成员上下文；未传入的成员使用默认实现。传入的成员原样使用，不被包装。
- 操作级错误：无。

`api.call<T>(method: string, args?: unknown) -> Promise<T>`

- 输入条件：`method` 为字符串。
- 输出语义：向 RPC 端点发起 `POST`，请求体为 `{ method, args }`，返回响应中的 `result`。
- 操作级错误：非 2xx 响应抛错，message 优先取响应体 `error`，否则为 `rpc failed: <method>`。

`events.on(type: string, handler: (payload: unknown) => void) -> () => void`

- 输入条件：`type` 为字符串，`handler` 为函数。
- 输出语义：登记处理器，按需建立唯一连接；返回的函数只移除该处理器。
- 操作级错误：帧解析失败时向控制台输出错误，不向订阅者抛错。

`locale.t(key: string) -> string`

- 输出语义：返回 `messages` 中的对应值；缺词时返回 key 本身。

`styles.add(id: string, css: string) -> () => void`

- 输入条件：`id` 与 `css` 为字符串。
- 输出语义：按 id 注入或更新一份样式；返回的函数删除该份样式。
- 操作级错误：无。

`defineClientPlugin(plugin: ClientPlugin) -> ClientPlugin`

- 输入条件：`plugin` 为函数。
- 输出语义：原样返回该函数，不做包装。
- 操作级错误：非函数时抛 `TypeError`。

`legacy(component: LegacySlotComponent) -> SlotComponent`

- 输出语义：返回包装后的组件，每次渲染先清空容器，再调用原组件。
- 操作级错误：无。

## 公共类型

`ClientContext`

- `id`：宿主写入的插件 id。
- `slots`：插槽注册表能力，语义见 `slot-registry` 契约。
- `api`：RPC 客户端。
- `events`：事件订阅。
- `locale`：本地化词典。
- `styles`：样式注入。

`ClientContextOptions`

- `id`：必填。
- `rpcEndpoint`：RPC 端点，缺省 `/api/rpc`。
- `eventsEndpoint`：事件端点，缺省 `/api/events`。
- `slots` / `api` / `events` / `locale` / `styles`：可注入的成员，缺省时新建默认实现。

`ClientApi`

- `call`：见上。

`ClientEvents`

- `on`：见上。

`Locale`

- `messages`：只读词典。
- `t`：见上。

`StyleSheet`

- `add`：见上。

`ClientPlugin`

- `(ctx: ClientContext) -> void | () => void | Promise<void | () => void>`。返回函数时作为插件级卸载。

`LegacySlotComponent`

- `(container, view) -> void | () => void`，由 `slot-registry` 契约统一定义。

## 错误

当前实现不定义稳定错误标识：

- `client plugin must be a function`：`defineClientPlugin` 收到非函数，以 `TypeError` 抛出。
- `rpc failed: <method>`：RPC 响应非 2xx 且响应体无 `error`。
- 其余失败以原生错误透传。

## 整体保证

- 上下文成员固定为六项。插件只能通过 `slots`、`api`、`events`、`locale`、`styles` 与 `id` 接触宿主能力，不存在额外的服务容器或依赖注入入口。
- 未提供的成员使用默认实现，且默认实现互不共享状态：每次调用 `createClientContext` 未显式传入时都会新建实例。
- RPC 一次调用对应一次 `POST`。调用不排队、不重试、不合并，异常直接交给调用方。
- 事件连接惰性建立。首个订阅者出现时才创建连接，后续订阅者复用同一连接。
- 事件按类型分发。同一类型可注册多个处理器，各处理器独立调用，单个处理器抛错不影响同类型其他处理器。
- 样式注入按 id 幂等。同一 id 重复注入只保留一份元素并更新内容；撤销函数只删除自己那次注入的元素。
- 本地化缺词不抛错，回落为 key。

## 按需扩展

订阅的清理语义：

- `events.on` 返回的函数只把该处理器从对应类型中移除。
- 某类型的处理器全部移除后，只删除该类型的登记记录，**不关闭连接**。
- 只有显式调用 `EventStream.dispose()` 才会关闭连接并清空全部订阅。

样式注入的清理语义：

- 撤销函数是幂等的：若该 id 已被后续注入替换，旧撤销函数不删除新元素。

已知现状：

- 浏览器侧没有依赖声明或服务容器。插件之间无法在浏览器侧声明「需要某个服务」，也无法在运行期收到依赖启停通知。
- 事件流没有序号与重放，也没有断线自动重连。连接中断后订阅者不再收到后续事件。
- `ClientEvents` 的载荷类型为 `unknown`，事件名与载荷形状没有统一约束。
- 主入口不得引入 Node 模块。这是包的分层约束，任何依赖 Node 的成员都必须留在适配器子路径。

取证范围：

- `packages/pulsar-plugin-client/src/index.ts`
- `packages/pulsar-plugin-client/src/types.ts`
- `packages/pulsar-plugin-client/src/define.ts`
- `packages/pulsar-plugin-client/src/legacy.ts`
- `packages/pulsar-plugin-client/src/rpc.ts`
- `packages/pulsar-plugin-client/src/events.ts`
- `packages/pulsar-plugin-client/src/locale.ts`
- `packages/pulsar-plugin-client/src/styles.ts`

待决策项：

- 浏览器侧是否需要插件间依赖声明与服务容器。
- 事件流是否需要断线重连与漏帧补偿。
- 事件名与载荷是否需要集中契约。
