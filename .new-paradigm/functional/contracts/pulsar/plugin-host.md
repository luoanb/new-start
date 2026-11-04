# plugin-host 代码契约

> 文档状态：待确认
> 契约标识：CTR-PSR-003
> 所属领域：pulsar（Pulsar 插件宿主与前端插件体系；`PSR` 代号由 CTR-PSR-001 声明）
> 上位依据：《代码契约规则》
> 适用范围：`PluginHost` 的启动、装载、卸载、启停与状态快照
> 下位产物：实现代码、契约一致性检查结果

## 职责边界

负责：

- 持有一棵 Cordis 插件树与一张运行时适配器注册表。
- 按条目装载插件：先交给适配器激活，再把结果挂进 Cordis fiber。
- 维护插件条目的注册、启用状态与 RPC 方法归属。
- 提供启停、卸载与状态快照。

不负责：

- profile 文件的解析与分层。该职责属于宿主装配能力。
- HTTP 端点协议与 boot graph 结构。该职责属于宿主传输能力。
- 编译、产物落盘与静态投递。该职责属于执行环境适配器。
- 浏览器侧插槽与渲染。该职责属于前端插件体系。

## 对外操作

`registerRuntime(runtime: PluginRuntime) -> void`

- 输入条件：`runtime.name` 在宿主内未被注册。
- 输出语义：把适配器登记到注册表，供后续按名查找。
- 操作级错误：同名重复注册抛错。

`start(options?: HostOptions) -> Promise<{ url: string }>`

- 输入条件：无。可在任意时刻调用一次。
- 输出语义：已启动时直接返回既有地址；否则解析产物目录与 dev 标志、创建产物根目录、按固定顺序装载内建服务、注册内建端点、监听端口，并置为已启动。
- 操作级错误：端口监听失败时异常传播，实例保持未启动。

`load(entry: PluginEntry) -> Promise<void>`

- 输入条件：宿主已启动；`entry.name` 未被占用。
- 输出语义：依次执行适配器激活与 fiber 挂载，随后递增 boot 版本号。
- 操作级错误：未启动或重名时抛错；激活或挂载失败时从条目表、已装载表与 RPC 归属表中移除该 name 后抛出。

`unload(name: string) -> Promise<void>`

- 输入条件：无。
- 输出语义：无对应 fiber 时直接返回；否则移除 fiber、RPC 归属与已装载记录，销毁 fiber，递增 boot 版本号。
- 操作级错误：无。未知 name 静默返回。
- 条目本身不被移除，因此该 name 之后仍可重新启用。

`setEnabled(name: string, enabled: boolean) -> Promise<void>`

- 输入条件：`name` 已在条目表中。
- 输出语义：目标状态与当前状态一致时直接返回；启用时重新激活并挂载，停用时执行卸载。
- 操作级错误：未知 name 抛错。

`state() -> HostState`

- 输入条件：无。
- 输出语义：返回当前全部条目的 `{ name, enabled, client, rpc }` 与宿主面板列表。
- 操作级错误：无。

`stop() -> Promise<void>`

- 输入条件：无。
- 输出语义：关闭 HTTP 服务并主动断开 keep-alive 连接；销毁根 fiber；清空条目、已装载、fiber 与 RPC 归属表；清除地址。
- 操作级错误：无。

## 公共类型

`HostOptions`

- `host`：监听地址，缺省 `127.0.0.1`。
- `port`：监听端口，缺省 `0` 表示由系统分配。
- `artifactsDir`：产物根目录，缺省 `<cwd>/.pulsar/artifacts`。
- `dev`：是否启用适配器热替换，缺省 `NODE_ENV !== 'production'`。

`PluginEntry`

- `name`：条目唯一名。
- `inject`：需要的 Cordis 服务名，缺失时 fiber 保持 PENDING。
- `module`：后端入口绝对路径。
- `apply`：插件主体，程序化装载时直接提供。
- `config`：传给 Cordis 的配置。
- `client`：Client 面入口，缺省表示没有浏览器侧。

`ClientSource`

- `entry`：Client 面入口绝对路径。
- `runtime`：执行环境适配器名，缺省 `dom`。

`PluginState`

- `name`：条目名。
- `enabled`：是否持有 fiber。**PENDING 状态的 fiber 也算持有**。
- `client`：是否已产出 Client 描述。
- `rpc`：该条目装载期间新增的 RPC 方法名。

`HostState`

- `plugins`：全部条目的状态快照。
- `panels`：宿主侧面板贡献列表。

`BootGraph` 与 `BootPlugin` 由宿主传输能力统一定义，本契约只规定其在 `PluginHost` 上的产出时机，不重复定义字段。

## 错误

当前实现不定义稳定错误标识，错误以 `Error` 的 message 表达：

- `host has not started`：在 `start()` 之前调用 `load()`。
- `plugin already registered: <name>`：`load()` 的 name 已存在。
- `unknown plugin: <name>`：`setEnabled()` 的 name 不在条目表。
- `runtime already registered: <name>`：`registerRuntime()` 重名。
- `runtime not registered: <name>`：条目声明了适配器但宿主未注册它。
- `plugin <name> has no apply export`：条目没有 `apply`，且适配器返回的 server 对象也没有可用的 `apply`。
- `import map conflict for "<name>": <a> vs <b>`：多适配器声明的共享依赖同键不同 URL。

## 整体保证

- `load` 失败后不留下半成品。条目表、已装载表与 RPC 归属表中都不存在该 name。
- 同一 name 在宿主内最多对应一个 fiber。`setEnabled(false)` 后再 `setEnabled(true)` 会得到新的 fiber。
- 卸载不删除条目。`unload` 与 `setEnabled(false)` 都保留 `entries` 中的记录。
- RPC 归属按装载增量计算。每次挂载只记录本次新增的方法名，不包含此前已有的方法。
- 条目名不允许重复装载。重复 `load` 在任何阶段都不会覆盖已有 fiber。
- 启动与停止各只生效一次。`start` 重复调用返回同一地址；`stop` 后地址被清除，再次 `start` 会重新监听。
- 适配器激活顺序固定：先 `node`，后 Client 面适配器。两者都产出时，`node` 的结果先被采用。

## 按需扩展

适配器清理项的生命周期：

- 激活阶段收集的清理项由 `mountFiber` 包装进插件的 `apply`，通过 `ctx.effect` 登记到 fiber。
- 因此这些清理项只有在包装后的 `apply` 真正执行时才会挂到 fiber；`apply` 未执行时它们处于未托管状态。
- 激活本身失败时，清理项会被立即逆序执行，这是另一条独立的回滚路径。

已知现状：

- PENDING 与已投递并存。当 `inject` 未满足时 fiber 处于 PENDING，包装后的 `apply` 不执行；但适配器在激活阶段已完成编译、路由注册与 Client 描述产出，`bootGraph()` 仍会下发该条目。
- `state().enabled` 把 PENDING 报为 `true`。调用方无法从该字段区分「已就绪」与「等待依赖」。
- PENDING 条目的适配器清理项没有挂到 fiber，因此 `unload` 或 `setEnabled(false)` 不会回收这些清理项对应的路由。
- 重新启用会再次执行适配器激活。若前一次的路由未被回收，同名路由会触发路由重复注册。

取证范围：

- `packages/pulsar-host/src/host.ts`
- `packages/pulsar-host/src/index.ts`

待决策项：

- 是否引入「已就绪」状态，或让 `enabled` 只表示 ACTIVE。
- PENDING 条目的适配器清理项应由哪一方托管。
- 重新启用时如何处理可能残留的路由与产物路由。
