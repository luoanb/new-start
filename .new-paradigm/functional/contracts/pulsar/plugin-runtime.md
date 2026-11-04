# plugin-runtime 代码契约

> 文档状态：待确认
> 契约标识：CTR-PSR-006
> 所属领域：pulsar（Pulsar 插件宿主与前端插件体系；`PSR` 代号由 CTR-PSR-001 声明）
> 上位依据：《代码契约规则》
> 适用范围：执行环境适配器接口、浏览器侧产物流水线与编译策略
> 下位产物：实现代码、契约一致性检查结果

## 职责边界

负责：

- 定义宿主与适配器之间的唯一协作接口：适配器接收条目、完成编译与存放、产出宿主进程内对象或浏览器侧描述。
- 定义适配器可使用的受限上下文：effect 登记、路由注册、产物目录、变更通知、日志。
- 提供浏览器侧适配器的公共流水线：查盘命中、编译、原子发布、静态投递、开发期监听。
- 定义编译策略接口，把打包器差异隔离在单一实现之后。
- 定义产物在磁盘上的布局、清单字段与保留策略。

不负责：

- 宿主的条目状态与 fiber 生命周期。
- profile 的解析。
- 浏览器侧的插件上下文与渲染。
- 具体业务插件的编译内容。

## 对外操作

`runtime.activate(source: PluginSource, ctx: RuntimeContext) -> Promise<Activation>`

- 输入条件：`source.entry` 为绝对路径；`ctx.artifactsDir` 存在且可写。
- 输出语义：返回 `{ server?, client?, dispose? }`，三者可同时存在。`server` 交给宿主挂进 fiber，`client` 写入启动图，`dispose` 由宿主回收。
- 操作级错误：入口不存在、编译失败、产物冲突时抛错。

`compiler.fingerprint(source: PluginSource) -> Promise<string>`

- 输出语义：返回构建配置指纹。指纹不包含源码内容，只反映影响产物形态的配置。
- 操作级错误：无。

`compiler.compile(input: CompileInput) -> Promise<CompileResult>`

- 输入条件：`input.outDir` 为空目录。
- 输出语义：产物已写入 `outDir`，返回入口文件名、资源列表与参与编译的源文件内容散列。
- 操作级错误：无产物时抛错。

`compiler.watch?(input: CompileInput, onChange: () => void) -> Promise<() => void>`

- 输出语义：监听源码变更，每次成功编译后回调一次；返回停止监听的函数。
- 操作级错误：无。

`ensureArtifact(options: EnsureArtifactOptions) -> Promise<Artifact>`

- 输出语义：命中已有产物时直接返回其位置；未命中时编译、发布并返回新位置。外部托管产物不落盘。
- 操作级错误：路径片段非法、编译失败时抛错。

`resolveDir(root, runtime, pluginId, hash?) -> string`

- 输出语义：返回 `<root>/<runtime>/<pluginId>/<hash>`；`hash` 缺省时返回插件目录。
- 操作级错误：任一片段含非法字符时抛错。

`readManifest(dir) -> Promise<ArtifactManifest | undefined>`

- 输出语义：读取 `manifest.json`；缺失或损坏时返回 `undefined`。

`readLatest(pluginDir) -> Promise<ArtifactManifest | undefined>`

- 输出语义：读取 `latest.json`；无编译记录时返回 `undefined`。

`writeAtomic(tmpDir, targetDir) -> Promise<void>`

- 输出语义：把临时目录改名为目标目录；目标已存在且含清单时丢弃本次结果。
- 操作级错误：改名失败且目标无清单时清理临时目录后抛错。

`prune(pluginDir, keep?, keepHash?) -> Promise<void>`

- 输出语义：按目录修改时间保留最近 `keep` 个版本，跳过 `keepHash`；默认 `keep = 3`。

`serveDir(dir, basePath) -> RouteHandler`

- 输出语义：把 `<basePath>/<file>` 映射到 `<dir>/<file>` 并返回文件内容，附强缓存头。
- 操作级错误：越出根目录返回 403；文件不存在返回 404。

## 公共类型

`PluginSource`

- `id`：插件 id，用于产物目录命名空间。
- `entry`：入口绝对路径。
- `options`：配置，原样透传。

`Activation`

- `server`：宿主进程内对象，由宿主决定如何挂载。
- `client`：浏览器侧描述。
- `dispose`：不依赖 `ctx.effect` 时的显式回滚。

`ClientDescriptor`

- `url`：模块可被 import 的 URL。
- `assets`：非 JS 产物的可访问 URL 列表。

`RuntimeContext`

- `effect(execute)`：立即执行，收集返回的清理函数。
- `route(path, handler)`：注册 HTTP 路由，立即生效。
- `artifactsDir`：产物根目录。
- `notify?(change)`：开发期变更通知，缺省表示不支持热替换。
- `logger`：`info` / `warn` / `error`。

`RuntimeChange`

- `type`：变更类别。
- `payload`：变更载荷。

`PluginRuntime`

- `name`：适配器名，宿主按名查找。
- `imports`：页面级共享依赖声明。
- `clientEntry`：浏览器侧入口 URL，纯服务端适配器不提供。
- `activate`：见上。

`Compiler`

- `name`：编译策略名。
- `imports`：该策略需要的页面级共享依赖。
- `clientEntry`：浏览器侧入口 URL。
- `fingerprint` / `compile` / `watch`：见上。

`CompileResult`

- `{ kind: 'files', entry, assets, inputs? }`：本地产物。
- `{ kind: 'external', url, assets? }`：外部托管产物。

`ArtifactManifest`

- `runtime`、`hash`、`entry`、`assets`、`inputs`、`buildOptionsFingerprint`、`createdAt`。

`Artifact`

- `{ kind: 'files', hash, dir, entry, assets }` 或 `{ kind: 'external', url, assets }`。

## 错误

当前实现不定义稳定错误标识，错误以 `Error` 的 message 表达：

- `invalid artifact path segment: <value>`：产物路径片段不匹配允许字符集，或为 `.` / `..`。
- `no plugin export found in <entry>`：服务端入口模块没有可识别的插件导出。
- `esbuild produced no output for <entry>`：编译没有产出任何文件。
- 编译器与文件系统的原生错误直接透传。

## 整体保证

- 产物位置只由「适配器名、插件 id、内容散列」决定。同一内容在不同时间编译得到同一目录。
- 散列只由编译策略名、构建配置指纹与排序后的源文件散列决定，与编译时刻无关。
- 命中判定要求同时满足：存在 `latest.json`、构建配置指纹一致、清单内每个源文件与磁盘内容一致、目标目录含清单。任一不满足即重新编译。
- 发布是原子的。编译先落在以 `.tmp-` 开头的临时目录，成功后整体改名；并发编译同一内容时保留先到者，后来者丢弃结果。
- 发布成功后才更新 `latest.json`，随后才执行版本清理。
- 卸载不删除产物。清理只在新产物成功发布后触发，且跳过当前正在服务的散列。
- 路径片段只能由字母、数字、点、下划线、连字符组成，且不能是 `.` 或 `..`。
- 静态投递只暴露根目录内的文件。解析后的绝对路径必须等于根目录或以「根目录 + 分隔符」开头，否则拒绝。
- 服务端适配器不编译、不落盘、不投递。它只导入入口模块并返回插件对象。

## 按需扩展

插件导出选择：

- 服务端入口优先取默认导出；默认导出不可用时，按导出顺序取第一个「对象且 `apply` 为函数」的导出。
- 对象形态的 `apply` 与 `inject` 会被宿主合并进条目，仅在条目自身未声明时生效。

三个内建适配器的差异：

- `node`：不提供 `imports` 与 `clientEntry`，只返回 `server`。
- `dom`：不把任何依赖声明为外部，`imports` 为空；插件若导入共享契约包会被内联进产物。
- `solid`：把 `solid-js`、`solid-js/web`、`solid-js/store` 声明为外部，并在激活时注册 `/rt/vendor/solid/*` 路由投递三份 ESM 文件，白名单之外返回 404。

开发期监听：

- 仅在编译策略提供 `watch` 且上下文提供 `notify` 时启用。
- 新散列编译成功后追加一条新路由并上报变更，旧散列的路由与目录都保留。
- 监听目录使用 `.tmp-watch`，不参与版本保留统计。

已知现状：

- 每次开发期重编译都会新增一条静态路由，且没有对应的撤销。旧散列路由持续存在，长期运行会累积。
- 外部托管产物没有磁盘清单，无法参与命中判定，也不进入版本保留。
- 命中判定逐文件读取源文件并计算散列，源文件数量多时开销随文件数线性增长。

取证范围：

- `packages/pulsar-plugin-client/src/runtime/types.ts`
- `packages/pulsar-plugin-client/src/runtime/browser.ts`
- `packages/pulsar-plugin-client/src/runtime/artifacts.ts`
- `packages/pulsar-plugin-client/src/runtime/compiler/types.ts`
- `packages/pulsar-plugin-client/src/runtime/compiler/esbuild.ts`
- `packages/pulsar-plugin-client/src/runtime/node.ts`
- `packages/pulsar-plugin-client/src/runtime/dom.ts`
- `packages/pulsar-plugin-client/src/runtime/solid.ts`

待决策项：

- 开发期重编译的旧路由如何回收。
- 外部托管产物是否需要清单与命中判定。
- 版本保留策略是否应可配置，而不是固定 3。
