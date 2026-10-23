# Technical Plan / 技术方案: 插件前端基建现代化

## Requirement Baseline / 需求基线

- 对应需求文档：`docs/sdd-lab/2026-10-23_14-22_plugin-client-infra/requirements.md`
- 需求确认状态：已确认（2026-10-23 14:22 四条 + 2026-10-23 讨论五条，见需求文档 Requirement Decisions）
- 本方案覆盖范围：P0 ~ P3。P4（RPC 类型生成、Playwright 验收）不在本期。

## Terminology / 术语

| 术语 | 含义 |
|---|---|
| 执行环境适配器（`PluginRuntime`） | 一种源码的「编译 → 存放 → 投递 → 执行」方式的完整封装。宿主只调用它，不关心其内部 |
| `Activation` | 适配器一次激活的产物：可选的宿主内插件实例、可选的浏览器执行描述 |
| `BrowserRuntime` | 浏览器适配器的公共流水线：查盘、原子落盘、静态投递、路由回滚、boot graph 交付。**不含编译** |
| `Compiler` | 编译策略接口。每个框架各自实现，`BrowserRuntime` 只调用它，不认识任何打包器 |
| 浏览器契约包 | `@pulsar/plugin-client`，插件在浏览器里使用的 Slots / RPC / Events / Locale / Styles 与类型 |
| 宿主内核 | `pulsar-plugin-host` 收缩后的部分：装配、生命周期、RPC/Events 总线、HTTP 与路由表、HTML 渲染 |
| 产物目录（Artifact Store） | 宿主约定的磁盘目录，适配器把编译结果写进去；产物与插件生命周期解耦，卸载插件不删除 |

## Current Project Facts / 当前项目事实

- 已读取文件/模块：`packages/pulsar-plugin-host/src/host.ts`、`src/profile.ts`、`src/types.ts`、`src/router.ts`、`src/rpc.ts`、`src/events.ts`、`src/verify.ts`、`client/runtime.js`、`examples/hello-plugin/client.ts`、`profile/profile.yml`、`pnpm-workspace.yaml`。

| 位置 | 当前实现事实 | 造成的问题 |
|---|---|---|
| `client/runtime.js:33-52` | `render()` 直接调用组件，组件返回 `void` | 没有卸载钩子，清理只能靠组件自觉 |
| `client/runtime.js:113-122` | shell 组件用 `replaceChildren()` 重建 `header/main` | 插件重启用后内容重复挂载 |
| `client/runtime.js:135-161` | 手写 boot 流程，只有 `slots/api/events/locale` | 没有执行环境概念，无法接框架 |
| `client/runtime.js:4` | 依赖全局 `#app` | 运行时与 DOM 结构强耦合 |
| `src/host.ts` `bundleClient(file: string)` | 签名里没有 runtime 参数 | 框架差异连传进来的通道都没有；宿主被绑死为唯一的构建方 |
| `src/host.ts` `bundleClient` 内 `result.outputFiles[0]?.text` | 只取第一个产物 | 含 CSS 的入口会静默丢掉第二个文件；Vue SFC scoped style 直接坏 |
| `src/host.ts:351-362` | esbuild `bundle: true, write: false`，无 `external`、无 `sourcemap` | 共用依赖被重复内联；浏览器报错无法映射源码 |
| `src/host.ts:255-281` | `clientBundles` 缓存 key 只有插件 id | runtime 变更不失效；切换框架会拿到旧产物 |
| `src/host.ts` `handleClientBundle` | 产物路由与缓存都写在宿主里 | 宿主承担了它不该承担的职责，且无法被第三方扩展 |
| `src/host.ts:162-169` | boot graph 只有 `id + clientUrl` | 无法声明执行环境，也无法声明共享依赖 |
| `src/host.ts:311-333` | HTML 内联，只有 `<script type="module" src="/client/runtime.js">` | 没有 import map 注入位；多框架时必然出现重复实例 |
| `src/host.ts` `load()` | 直接 `await import(entry)` | 后端执行方式硬编码在宿主，无法替换为其他执行环境 |
| `src/profile.ts:37-41` | `client?: string` 解析为单一绝对路径 | 无法携带执行环境等元数据 |
| `src/types.ts:18-32` | `PatchEntry.client?: string` | 同上 |
| `examples/hello-plugin/client.ts:30-41` | 手写 `ClientContext` / `SlotSpec` / `SlotView` | 与宿主实现漂移，插件作者重复劳动 |
| `src/verify.ts:23-25` | 只断言 bundle 文本包含 `Hello plugin` | 未验证真实渲染与卸载 |

**包管理事实**：根 `pnpm-workspace.yaml` 只包含 `packages/*`；`packages/pulsar-plugin-host/pnpm-workspace.yaml` 是一份局部 workspace（指向 `../pulsar-kernel/vendor/*`）并自带 `pnpm-lock.yaml`。新建包会落入根 workspace，但 plugin-host 的局部安装是否能解析到它需要执行前确认（见 Q4）。

## Architecture / 总体架构

### 边界一：宿主内核不知道代码怎么来、怎么跑

宿主收缩到四件事，其余全部交出：

```text
┌──────────────────────────────────────────────────────────┐
│ 宿主内核                                                 │
│  profile / bundle / patch 装配                           │
│  fiber 生命周期（ctx.effect 回滚）                        │
│  RPC 总线 + SSE 事件总线                                  │
│  HTTP + 路由表 + HTML 渲染                                │
└───────────────────────────┬──────────────────────────────┘
                            │ 只交付 { id, entry, runtime, options }
                            ▼
┌──────────────────────────────────────────────────────────┐
│ 执行环境适配器 PluginRuntime                              │
│ 编译 → 存放（落盘） → 投递 → 执行，全部自负               │
└───────────────────────────┬──────────────────────────────┘
                            │ 写盘 / 静态读取
                    ┌───────┴────────┐
                    ▼                │
        产物目录 <artifactsDir>/      │
        <runtime>/<plugin>/<hash>/    │
                    │                │
              ┌─────────────┬────────┘
              ▼             ▼
        Node 进程        浏览器
   （server 面实例）  （client 面执行）
```

五条设计原则：

1. **宿主零构建**：宿主 `dependencies` 里不再有 `esbuild`；`bundleClient`、产物路由、产物缓存全部迁出。
2. **适配器全权负责**：一个框架的接入面不再被切成「宿主里的编译配置 + 浏览器里的 mount」两半，而是完整属于同一个适配器。
3. **运行期资源挂 fiber，产物不挂**：适配器占用的 HTTP 路由、监听、watch 通过 `ctx.effect()` 注册，插件卸载即回滚；但已写盘的编译产物不随之删除——它属于可复用缓存，不属于运行期资源。
4. **产物先落盘，运行期直取**：适配器编译完成即把文件组写进宿主约定的产物目录，返回的 `client.url` 指向该目录的静态路径。重启、重新启用、热替换都先查盘，命中就不再编译。
5. **宿主只认两个产出**：有 `server` 实例就进 fiber，有 `client` 描述就原样写进 boot graph。宿主不判断「这是前端还是后端」，也不关心产物在磁盘上长什么样。

### 边界二：适配器之间的关系有四层

**编排复用 + 策略替换（不是继承特化）**：`dom / solid / vue / react` 共用同一个 `BrowserRuntime`，但共用的不是「编译」，而是**产物流水线**——查盘、原子写盘、manifest、静态投递、路由回滚、boot graph 交付。编译被抽成 `Compiler` 策略，由各框架独立实现。

为什么编译不能进基类：四个框架的编译根本不是同一件事。

| 适配器 | 编译本质 | 能否用一个 `build` 配置描述 |
|---|---|---|
| `dom` | 剥离 TS / 原样拷贝 | 可以 |
| `react` | esbuild JSX + `jsxImportSource: react` + 常量替换 | 可以 |
| `solid` | esbuild 的 JSX 只是近似，真正的细粒度优化在 `babel-preset-solid` | 勉强，且有损 |
| `vue` | `.vue` 单文件组件需 `@vue/compiler-sfc` 拆 script/template/style，产出 scoped CSS | 不能 |

若用继承切分，基类就必须长出 `loader`、`jsx`、`plugins`、`define` 等一堆字段，最终退化成 `build: any`；每接一个新框架都要回头改基类。因此改为组合：基类只依赖 `Compiler` 接口，编译实现由适配器注入。

**平行**：`node` 与 `BrowserRuntime` 没有共享代码，是两个并列的 `PluginRuntime` 实现。`node` 是该抽象的退化实现：编译等于恒等、存放等于不存放、投递等于不投递、执行等于 `import()`。

**共存耦合**：同一页面可以同时存在不同适配器的插件，它们互不通信，但被迫共享两个全局唯一物——`import map`（一页只能一张）与 `@pulsar/plugin-client`（slots 注册表 / RPC / 事件总线必须同实例）。这是全套架构最容易出事的地方。

**降级兼容**：`dom` 不是「朴素的框架适配器」，而是**旧插槽组件签名的落点**。旧签名 `(container, view)` 只在 `dom` 下合法；`dom` 随包内置、不可卸载。

### 唯一耦合点：为什么 import map 是例外

适配器能全权处理 HTTP 资源（产物文件、vendor 文件、路由），但 `import map` 不是资源，它是**必须写进 HTML `<head>` 的一小段 JSON**，而 HTML 由宿主渲染。所以适配器没法自己落笔，只能把「`solid-js` 应该指向哪个 URL」交给宿主。

约束是硬的：

- 一个文档只能有一张 import map，多张合并的支持在各浏览器并不完整。
- import map 必须出现在第一个 module script 之前，否则后续裸模块名解析失败。

因此它必须**结构化**而不能是原始片段：若每个适配器各自贡献一段 `<script type="importmap">`，两个适配器同页即失效；而要合并原始片段，宿主还得先解析 HTML 再拼 JSON，比直接收结构化数据更糟。

结论：适配器声明 `imports: Record<string, string>`，宿主只做三件事——按 URL 去重、同键不同值时抛错、合并成唯一一张表。宿主仍然不知道 `solid-js` 意味着什么。

## API Design / API 设计

### Contract Scope / 契约范围

- 变更类型：新增（`@pulsar/plugin-client`）+ 扩展（profile / boot graph）+ 兼容保留（旧 `client: string` 与 `(container, view)` 插槽组件签名）
- 消费方：插件作者、宿主实现、适配器作者、示例插件、验收脚本
- 真相源文件：`packages/pulsar-plugin-client/src/index.ts`（浏览器契约）、`packages/pulsar-plugin-client/src/runtime/index.ts`（适配器契约）

### `PluginRuntime`（执行环境适配器）

```ts
export interface PluginSource {
  /** 插件 id，适配器用它做产物目录命名空间 */
  id: string
  /** 绝对路径，由宿主解析 profile 得到；适用于本地源码类插件 */
  entry: string
  /** profile 中该插件声明的执行环境相关配置，原样透传 */
  options?: unknown
}

export interface RuntimeContext {
  /** Cordis Context 的受限视图，由宿主传入 */
  effect(execute: () => void | (() => void)): () => void
  route(path: string, handler: RouteHandler): () => void
  /** 产物目录根，由宿主约定；适配器在其下自建子目录，不得写到别处 */
  artifactsDir: string
  logger: Logger
}

export interface Activation {
  /** 有值则在宿主进程内作为插件实例挂进 fiber */
  server?: PluginObject
  /** 有值则写进 boot graph，交浏览器执行 */
  client?: ClientDescriptor
  /** 不依赖 ctx.effect 时的显式回滚，二者可并存 */
  dispose?: () => void | Promise<void>
}

export interface ClientDescriptor {
  url: string
  assets?: string[]
}

export interface PluginRuntime {
  readonly name: string
  /** 该适配器需要的页面级共享依赖；宿主合并去重，不解释内容 */
  readonly imports?: Record<string, string>
  /** 浏览器侧适配器产物 URL；纯服务端适配器不提供 */
  readonly clientEntry?: string
  activate(source: PluginSource, ctx: RuntimeContext): Promise<Activation>
}
```

宿主侧只做两件无需判断的事：

```ts
const act = await runtime.activate(source, ctx)
if (act.server) mountInFiber(act.server, ctx)
if (act.client) registry.addClient(source.id, act.client)
```

`PluginRuntime` 不承诺「一定是前端」——`side` 字段刻意不设，因为宿主看返回值即可，多一个字段就多一次判断。

### `BrowserRuntime`（编排基类）与 `Compiler`（编译策略）

编译是四家差异最大的部分，因此不进基类，抽为可替换策略：

```ts
export interface CompileInput {
  source: PluginSource
  /** 空目录，编译器把全部产物写在这里；rename 到 hash 目录由基类负责 */
  outDir: string
}

export type CompileResult =
  /** 本地编译产出，文件已写入 outDir；inputs 是含依赖的全部源文件内容 hash */
  | { kind: 'files'; entry: string; assets: string[]; inputs?: Record<string, string> }
  /** 外部构建（如 Vite dev server），产物不在本机，直接给 URL */
  | { kind: 'external'; url: string; assets?: string[] }

export interface Compiler {
  readonly name: string
  /** 该编译器需要的页面级共享依赖；宿主合并去重，不解释内容 */
  readonly imports: Record<string, string>
  /** 浏览器侧 runtime 入口；纯服务端适配器不提供 */
  readonly clientEntry?: string
  /** 构建配置指纹，参与命中判定 */
  fingerprint(source: PluginSource): Promise<string>
  compile(input: CompileInput): Promise<CompileResult>
  /** 开发模式可选：监听源码变更，返回停止函数 */
  watch?(input: CompileInput, onChange: () => void): Promise<() => void>
}
```

基类只剩与框架无关的编排：

```ts
export abstract class BrowserRuntime implements PluginRuntime {
  abstract readonly name: string
  protected abstract readonly compiler: Compiler

  get imports() { return this.compiler.imports }
  get clientEntry() { return this.compiler.clientEntry }

  /** 子类必须实现的渲染原语 */
  abstract mount(container: Element, component: SlotComponent, view: SlotView): RuntimeHandle
  abstract slot(container: Element, name: string, view: SlotView): unknown

  async activate(source, ctx) {
    const artifact = await ensureArtifact({ root: ctx.artifactsDir, runtime: this.name, source, compiler: this.compiler })
    if (artifact.kind === 'external') return { client: { url: artifact.url, assets: artifact.assets } }
    const base = `/rt/${this.name}/${source.id}/${artifact.hash}`
    ctx.effect(() => ctx.route(`${base}/*`, serveDir(artifact.dir)))
    return { client: { url: `${base}/${artifact.entry}`, assets: artifact.assets.map(a => `${base}/${a}`) } }
  }
}
```

`ensureArtifact()` 是 `artifacts.ts` 里的纯编排：算构建指纹 → 读 `latest.json` → 校验全部依赖 → 命中即返回 → 否则建 `.tmp-<pid>-<time>/` → 调 `compiler.compile()` → 用编译产出的依赖图算 hash → 原子 rename → 更新 `latest.json` → `prune()`。它不认识任何打包器。

esbuild 仍然被复用，但靠**组合**而非继承：

```ts
export function esbuildCompiler(options: {
  plugins?: () => EsbuildPlugin[]
  loader?: Record<string, Loader>
  jsx?: 'automatic' | 'preserve'
  jsxImportSource?: string
  external: string[]
  define?: Record<string, string>
  imports: Record<string, string>
  clientEntry: string
}): Compiler
```

- `dom` / `react` / `solid` 各传入不同 options 的 `esbuildCompiler`。
- `vue` 给 `@vue/compiler-sfc` 写独立 `Compiler`，或直接返回 `{ kind: 'external' }` 指向 Vite。
- 换编译后端（swc / rspack）只新增一个 `Compiler`，`BrowserRuntime` 与 `artifacts.ts` 一行不改。

**一个适配器 = 一个 `Compiler` + 一组 `mount/slot`**。`node` 适配器不继承 `BrowserRuntime`，它独立实现 `PluginRuntime`。

### 产物目录（Artifact Store）

宿主约定一个根目录（`artifactsDir`，默认 `<cwd>/.pulsar/artifacts`，可通过宿主配置覆盖），适配器在根目录下按 `runtime / plugin / hash` 三级自建子目录：

```text
<artifactsDir>/
└── solid/
    └── hello/
        ├── latest.json         # 最近一次成功编译的 manifest 副本，命中判定从这里开始
        ├── a1b2c3d4e5/         # 一次编译的完整产出，按内容 hash 命名
        │   ├── entry.js
        │   ├── style.css
        │   └── manifest.json
        └── f6a7b8c9d0/         # 源码或依赖变更后的新产出，旧目录保留
```

`manifest.json` 是单个版本的收据，`latest.json` 是命中入口：

```json
{
  "runtime": "solid",
  "hash": "a1b2c3d4e5",
  "entry": "entry.js",
  "assets": ["style.css"],
  "inputs": {
    "../examples/hello-plugin/client.tsx": "9f2c…",
    "../examples/hello-plugin/style.css": "41ab…"
  },
  "buildOptionsFingerprint": "b71d…",
  "createdAt": "2026-10-23T08:00:00.000Z"
}
```

命中判定必须逐条校验，全部通过才复用：

1. `latest.json` 存在且 `buildOptionsFingerprint` 与当前编译器一致；
2. `inputs` 中**每一个**源文件（含依赖，不只是入口）当前内容 hash 与记录一致；
3. `latest.json` 指向的 `<hash>/manifest.json` 仍然存在。

hash 在编译**之后**由「编译器名 + `buildOptionsFingerprint` + 全部 `inputs` 的 hash」算出，因此只要依赖发生变化，必然产生新目录与新的 URL，不会撞上浏览器强缓存。

五条硬约定：

1. **先写临时目录再原子 rename**：编译结果写到 `<pluginId>/.tmp-<pid>-<time>/`，成功后 `rename` 为 `<hash>/`，避免并发或崩溃留下半成品；hash 必须等编译产出依赖图后才能确定，所以临时目录不能以 hash 命名。
2. **命中即零编译**：只读 `latest.json` 与源文件 hash，不触发任何编译。
3. **卸载不删产物**：插件禁用、fiber 回滚、宿主退出都不删除已写盘的目录。删除只由保留策略触发（见下）。
4. **URL 与磁盘一一对应**：`/rt/<runtime>/<pluginId>/<hash>/<file>` 直接映射到 `<artifactsDir>/<runtime>/<pluginId>/<hash>/<file>`，排查时可直接在磁盘上对照。
5. **保留最近 3 个版本（Q7 = ①）**：每个 `<runtime>/<pluginId>/` 下最多保留 3 个 hash 目录（含当前正在服务的那个），按目录 `mtime` 从旧到新删除，且必须跳过本次激活刚返回的 hash。

保留策略发生在 `artifacts.ts` 的 `prune()`：只在**新产物成功 rename 之后**调用，不在卸载路径调用。这样「卸载不删」与「磁盘不无限增长」两条同时成立——回收只跟编译次数有关，与插件生死无关。

根目录默认 `<cwd>/.pulsar/artifacts`（可由宿主配置覆盖），必须写入仓库 `.gitignore`；CI 可缓存该目录以跳过重复编译。

宿主只提供 `artifactsDir` 与静态投递能力，不解释目录内部结构；适配器只使用这个根目录，不自行猜测工作路径。

### 浏览器契约（`@pulsar/plugin-client`）

包的 `exports` 必须让插件作者的默认入口是纯浏览器代码，Node 依赖只能从子路径取：

```json
{
  "exports": {
    ".": "./src/index.ts",
    "./runtime": "./src/runtime/index.ts",
    "./runtime/node": "./src/runtime/node.ts",
    "./runtime/dom": "./src/runtime/dom.ts",
    "./runtime/esbuild": "./src/runtime/compiler/esbuild.ts"
  }
}
```

`.` 导出 `defineClientPlugin`、Slots、RPC、Events、Locale、Styles 与全部浏览器类型，零 Node 依赖；`./runtime` 只导出 `PluginRuntime`、`BrowserRuntime`、`Compiler`、`CompileResult`、`PluginSource`、`Activation` 等接口与流水线，**其模块图不含 esbuild**；`./runtime/node` 是零打包器的服务端适配器；`./runtime/dom` 与 `./runtime/solid` 才经由 `./runtime/esbuild` 引入 esbuild。

esbuild 是本包的 `dependencies`，而不是宿主的：宿主 `dependencies` 中没有 esbuild，但注册 `dom` 适配器时由本包按需加载。这就是「宿主不 import 打包器」的准确边界——磁盘上有，模块图里只有适配器子路径才碰得到。

插槽组件签名同时保留两种：

```ts
export interface SlotView {
  scopeId?: string
  renderSlot(name: string, container: Element, options?: { scopeId?: string }): void
  onCleanup(fn: () => void): void
}

/** 旧签名，仅在 dom 适配器下合法 */
export type LegacySlotComponent = (container: Element, view: SlotView) => void | (() => void)
/** 新签名，返回框架节点 */
export type RenderSlotComponent<TNode> = (view: SlotView) => TNode
export type SlotComponent<TNode = unknown> = LegacySlotComponent | RenderSlotComponent<TNode>
```

兼容的是**插槽组件签名**，不是插件入口——插件入口当前已是 `apply(ctx)` 形态，无需改动。

```ts
export interface ClientPlugin {
  readonly kind: 'client'
  apply(ctx: ClientContext): void | (() => void) | Promise<void | (() => void)>
}

export function defineClientPlugin(
  setup: (ctx: ClientContext) => void | (() => void) | Promise<void | (() => void)>,
): ClientPlugin
```

### profile 契约

```yaml
plugins:
  - id: hello
    module: ../examples/hello-plugin/index.ts
    client:
      entry: ../examples/hello-plugin/client.ts
      runtime: dom
  - id: legacy
    module: ../examples/legacy/index.ts
    client: ../examples/legacy/client.ts   # 旧写法，归一化为 { entry, runtime: 'dom' }
```

`runtime` 缺省 `dom`；`module` 侧同样接受 `runtime`，缺省 `node`。

### Boot Graph 契约

```json
{
  "rev": "7",
  "runtimes": {
    "dom": "/rt/dom/client.js",
    "solid": "/rt/solid/client.js"
  },
  "imports": {
    "@pulsar/plugin-client": "/rt/vendor/plugin-client.js",
    "solid-js": "/rt/vendor/solid.js",
    "solid-js/web": "/rt/vendor/solid-web.js"
  },
  "plugins": [
    { "id": "hello", "runtime": "solid", "client": { "url": "/rt/solid/hello/a1b2c3/entry.js", "assets": ["/rt/solid/hello/a1b2c3/style.css"] } }
  ]
}
```

- `runtimes` 只收录提供了 `clientEntry` 的适配器
- `imports` 由宿主合并去重后写成唯一一张 import map，位于第一个 module script 之前
- `plugins[].client` 由适配器返回，宿主原样透传，不解释 `url` 的内部结构；该 URL 指向产物目录中的静态文件

### 宿主扩展点

```ts
class PluginHost {
  registerRuntime(runtime: PluginRuntime): () => void
}
```

`node` 与 `dom` 在 `start()` 时内置注册；框架适配器由 profile 顶层 `runtimes` 声明并加载。注册即贡献，注销即回滚，与其它插件贡献同构。

## Solution Options / 方案候选

三套方案共用同一基座，P0 / P1 完全一致。差异收敛到三点：`Compiler` 实现、`imports` 条目、`mount/slot` 实现。三者都不需要改动 `BrowserRuntime` 与 `artifacts.ts`。

### Option A / 方案 A：Solid

- 推荐：是（若团队无强绑定生态）
- `Compiler`：`esbuildCompiler({ plugins: () => [solid()], jsx: 'automatic', jsxImportSource: 'solid-js', external: ['solid-js', 'solid-js/web', 'solid-js/store'], imports: {...} })`（`solid()` 来自 `esbuild-plugin-solid`）
- `imports`：3 条
- `mount`：`createRoot(dispose => render(() => component(view), container))`，`unmount` 调 `dispose`

```tsx
import { createSignal } from 'solid-js'
import { defineClientPlugin } from '@pulsar/plugin-client'

function HelloPanel() {
  const [count, setCount] = createSignal(0)
  return <button onClick={() => setCount(count() + 1)}>{count()}</button>
}

export default defineClientPlugin((ctx) => {
  ctx.slots.register({ name: 'main', id: 'hello-panel' }, () => <HelloPanel />)
})
```

- 优点：无虚拟 DOM，挂载/卸载与容器模型天然契合，适配器最薄；运行时体积最小。
- 缺点：生态与现成组件库少于 Vue / React；TSX 编译需多一层 babel 预设。
- 风险：`createRoot` 所有权边界处理不当会导致卸载后 effect 存活，必须在适配器内统一 `dispose`。

### Option B / 方案 B：Vue 3

- 推荐：是（若后续插件需要大量现成组件库）
- `Compiler`：自定义 `sfcCompiler`（`@vue/compiler-sfc` 拆块 + esbuild 处理 script，产出 scoped CSS），`external: ['vue']`；或直接返回 `{ kind: 'external' }` 交给 Vite dev server
- `imports`：1 条
- `mount`：`createApp(component).mount(container)`，`unmount` 调 `app.unmount()`
- 额外约束：SFC 的 scoped style 会产生第二个产物文件，直接验证 `Activation` 的文件组模型是否成立

```ts
import { h } from 'vue'
import HelloPanel from './HelloPanel.vue'
import { defineClientPlugin } from '@pulsar/plugin-client'

export default defineClientPlugin((ctx) => {
  ctx.slots.register({ name: 'main', id: 'hello-panel' }, () => h(HelloPanel))
})
```

- 优点：生态最丰富，SFC 体验贴近常规 Vue 项目；`createApp` 自带 app 级作用域，样式隔离相对清晰。
- 缺点：必须引入 SFC 编译链，适配器包体积与启动成本上升；多插件各自 `createApp` 时全局配置需统一注入。
- 风险：SFC 编译链是唯一会引入「非 esbuild 原生能力」的地方，若编译失败必须只降级该插件。

### Option C / 方案 C：React

- 推荐：否（除非团队已深度绑定 React）
- `Compiler`：`esbuildCompiler({ jsx: 'automatic', jsxImportSource: 'react', external: ['react', 'react-dom', 'react-dom/client', 'react/jsx-runtime'], imports: {...} })`
- `imports`：4 条以上
- `mount`：`createRoot(container).render(component(view))`，`unmount` 调 `root.unmount()`

```tsx
import { useState } from 'react'
import { defineClientPlugin } from '@pulsar/plugin-client'

function HelloPanel() {
  const [count, setCount] = useState(0)
  return <button onClick={() => setCount((v) => v + 1)}>{count}</button>
}

export default defineClientPlugin((ctx) => {
  ctx.slots.register({ name: 'main', id: 'hello-panel' }, () => <HelloPanel />)
})
```

- 优点：生态最大，招人成本低；esbuild 原生 JSX，无需额外编译插件。
- 缺点：`imports` 条目最多，单例约束最强；跨插槽 context 需额外 Provider 桥接；运行时体积最大。
- 风险：漏配任一 `react/*` 子路径即出现两份实例，hooks 直接报错；必须提供启动期自检。

### 方案对比

| 维度 | A Solid | B Vue 3 | C React |
|---|---|---|---|
| `mount` 实现复杂度 | 低 | 中 | 中高 |
| `Compiler` 实现 | `esbuildCompiler` + solid 插件 | 自定义 SFC 编译链，或外置 Vite | `esbuildCompiler` 裸用 |
| `imports` 条目 | 3 | 1 | 4+ |
| 运行时体积 | 小 | 中 | 大 |
| 生态/组件库 | 小 | 大 | 最大 |
| 与容器模型契合度 | 高 | 中 | 中 |
| 主要风险 | 团队熟悉度 | SFC 编译链 | 单例与 context 边界 |

## Decision / 方案决策

- Selected / 选定方案：A Solid
- Why / 选择原因：用户选定。`mount` 最薄（`createRoot` + `dispose` 与容器模型天然契合）、`imports` 仅 3 条、运行时体积最小、无虚拟 DOM，跨插槽切换时不必处理调度器差异。
- Decision Owner / 决策人：用户
- Decision Time / 决策时间：2026-10-23 16:20
- Open Questions 状态：Q1 ~ Q7 已全部关闭
- 关联决策：
  - Q2 = 允许同页多适配器共存 → 保留 `imports` 合并去重与冲突抛错，不引入「每页单适配器」校验。
  - Q4 = 根 workspace 统一管理，`@pulsar/plugin-client` 以 `workspace:*` 被 `plugin-host` 引用。
  - Q5 = 后端同样统一走 `PluginRuntime`，`node` 作为退化实现（编译恒等、不落盘、不投递、执行即 `import()`）。
  - Q6 = P0 直接采用「文件组 + hash」产物模型，`Activation.client.assets` 从第一版就是数组。
  - Q7 = ① 固定 `<cwd>/.pulsar/artifacts`，每个插件保留最近 3 个 hash，超出自动回收。

## Open Questions / 开放问题

- [x] Q1 官方一等公民适配器选哪一个（A Solid / B Vue / C React）？
  - 影响范围：P2 的子类实现、编译链依赖、文档与示例
  - 候选处理：三选一为官方，其余保留接口但不在本期实现
  - 结论：A Solid。其余两套方案保留在 `Solution Options` 作为未来接入参考，本期不实现。
- [x] Q2 是否允许同一页面多适配器共存？
  - 影响范围：`imports` 合并策略、boot graph 结构、首屏体积
  - 候选处理：① 每页单适配器（体积小、需启动期校验）；② 多适配器共存（兼容性最好、体积翻倍）
  - 结论：② 允许共存。按「宿主合并去重 + 冲突抛错」实现。
- [x] Q3 若选 Vue，是否接受引入 SFC 编译链？
  - 结论：不适用。Q1 已选 Solid，本期不引入 `@vue/compiler-sfc`；若未来接 Vue，走 `Compiler` 策略或 `{ kind: 'external' }`，不改基类。
- [x] Q4 新包的包管理归属？
  - 候选处理：① 根 workspace 统一管理，plugin-host 用 `workspace:*` 引用；② 并入 plugin-host 局部 workspace
  - 结论：① 根 workspace。
- [x] Q5 后端是否统一走 `PluginRuntime`（`node` 作为退化实现）？
  - 触发来源：讨论中确认「执行环境适配器」思路对后端同样适用，但用户尚未表态是否本期落地
  - 影响范围：`load()` 是否改造、`PluginEntry` 是否增加 `runtime` 字段、后端是否多一层间接
  - 候选处理：① 统一（前后端同一心智模型，未来可接 WASM / 子进程 / 远程插件）；② 仅前端使用，后端保留直接 `import()`
  - 结论：① 统一。`load()` 只走 `runtime.activate()`，`node` 适配器内部仍是 `import()`。
- [x] Q6 产物模型「文件组 + hash」放在 P0 还是 P2？
  - 触发来源：`outputFiles[0]` 已经证明单文件模型不成立（含 CSS 的入口会丢产物）
  - 影响范围：产物路由、缓存 key、boot graph `client.assets`
  - 候选处理：① P0 直接上（P0 变大，但 P2 无需返工）；② P2 再改（P0 更快，但路由与缓存在 P2 重写一次）
  - 结论：① P0 直接上。`manifest.json`、`assets` 数组、原子 rename 全部进 P0。
- [x] Q7 产物目录的位置、提交策略与清理方式？
  - 触发来源：用户已确认「编译产出需要存储、不随插件销毁、运行阶段直接从该位置加载」，但目录位置与回收策略未定
  - 影响范围：默认路径、`.gitignore`、CI 缓存、磁盘增长
  - 候选处理：① 固定 `<cwd>/.pulsar/artifacts`，按插件保留最近 3 个 hash，超出自动回收；② 同位置但不自动回收，提供 `artifacts clean` 命令；③ 放 `node_modules/.cache/pulsar`，依赖重装即自然清空
  - 结论：① 固定路径 + 保留最近 3 个。回收由 `prune()` 在新产物落盘后触发，跳过当前服务的 hash。

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：Q1 ~ Q7 已关闭（2026-10-23 16:20，见 `Decision`）；`Decision` 已填写
- 若执行前需求、API、范围或交互规则变化：先回写本文件对应章节，再动代码

### Step 1.（P0）建立浏览器契约包

#### 文件：`packages/pulsar-plugin-client/package.json`

- 改动类型：新增
- 改动内容：`name: "@pulsar/plugin-client"`、`type: module`；`exports` 分五个入口——`.`（浏览器）、`./runtime`（契约与流水线，模块图无 esbuild）、`./runtime/node`、`./runtime/dom`、`./runtime/esbuild`；`esbuild` 作为本包 `dependencies`，不进宿主 `dependencies`
- 归属（Q4）：根 `pnpm-workspace.yaml` 的 `packages: packages/*` 已覆盖新包；当前根 workspace 不可安装，故同时登记到 `pulsar-plugin-host/pnpm-workspace.yaml`，由局部 workspace 完成安装
- 验收点：`pnpm --filter @pulsar/plugin-client typecheck` 通过；`./runtime` 的模块图不出现 esbuild；宿主 `package.json` 的 `dependencies` 不含 esbuild

#### 文件：`packages/pulsar-plugin-client/src/{slots,rpc,events,locale,styles}.ts`

- 改动类型：新增（自 `client/runtime.js` 迁移并类型化）
- 改动内容：每个模块提供 `create*()` 工厂，不依赖全局 `document`；`slots.register` 重复 id 抛错；`onCleanup` 按 LIFO 执行
- 验收点：单元级验证注册 → 渲染 → dispose 后容器为空

#### 文件：`packages/pulsar-plugin-client/src/index.ts`

- 改动类型：新增
- 改动内容：导出 `defineClientPlugin`、`SlotComponent`、`SlotView` 等全部浏览器类型与 API，组装默认 `ClientContext`
- 验收点：`import { defineClientPlugin } from '@pulsar/plugin-client'` 可用且不引入 Node 模块

#### 文件：`packages/pulsar-plugin-client/src/runtime/{types,browser}.ts`

- 改动类型：新增
- 改动内容：`PluginRuntime`、`PluginSource`、`Activation`、`RuntimeContext`；`BrowserRuntime` 基类只做编排——调 `ensureArtifact()`、注册静态路由、返回 `client` 描述；只有路由走 `ctx.effect` 回滚
- 设计约束：基类只认 `Compiler` 与 `mount/slot` 两个抽象，不 import 任何打包器，不出现任何框架名分支
- 验收点：`browser.ts` 中搜索不到 `esbuild` / `solid` / `vue` / `react` 字面量

#### 文件：`packages/pulsar-plugin-client/src/runtime/compiler/{types,esbuild}.ts`

- 改动类型：新增
- 改动内容：
  - `types.ts`：`Compiler`、`CompileInput`、`CompileResult`、`ensureArtifact()` 依赖的 `fingerprint` 约定
  - `esbuild.ts`：`esbuildCompiler(options)`，把 options 转成一次 esbuild `build`（`write: false` + `metafile: true`），把 `outputFiles` 完整落盘为文件组，从 metafile 提取全部依赖并算内容 hash 作为 `inputs`，并实现 `fingerprint` 与可选 `watch`
- 设计约束：`esbuild` 只允许在本文件 import；`artifacts.ts` 与 `browser.ts` 不得出现 esbuild 类型
- 验收点：`esbuildCompiler` 处理含 CSS 的入口时返回完整的 `{ entry, assets }`，不止第一个文件；`inputs` 同时包含入口与被 import 的 CSS

#### 文件：`packages/pulsar-plugin-client/src/runtime/artifacts.ts`

- 改动类型：新增
- 改动内容：产物目录的读写原语——`resolveDir()`、`readManifest()`、`readLatest()`、`writeAtomic()`（`rename` 临时目录）、`ensureArtifact()`（算指纹 → 读 latest → 校验全部依赖 → 命中返回 / 否则编译 → 后置算 hash → 原子发布 → 更新 latest → `prune()`）、`prune()`（每个 `<runtime>/<pluginId>/` 保留最近 3 个 hash，跳过当前 hash）、`serveDir()`
- 设计约束：只处理目录与文件，不引入任何打包器；`prune()` 只在新产物成功 rename 后调用，不在卸载路径调用（Q7 = ①）
- 验收点：并发两次 `writeAtomic()` 同一 hash 不产生半成品目录；未变源码第二次调用不重新编译；改被 import 的 CSS 会产生新 hash；同一插件连续编译 4 次后目录内只剩 3 个 hash 且最新指针未被删除

### Step 2.（P0）落地 `node` 与 `dom` 两个适配器

#### 文件：`packages/pulsar-plugin-client/src/runtime/node.ts`

- 改动类型：新增
- 改动内容：`activate()` = `import(pathToFileURL(entry))` + 解析插件导出，返回 `{ server }`；不编译、不缓存、不投递、不提供 `clientEntry`（Q5 = 统一，`node` 是退化实现）
- 验收点：与改造前 `load()` 行为逐项一致（同一插件、同一导出解析规则）；该适配器不创建任何产物目录

#### 文件：`packages/pulsar-plugin-client/src/runtime/dom.ts`

- 改动类型：新增
- 改动内容：`class DomRuntime extends BrowserRuntime`，`compiler = esbuildCompiler({ external: [], imports: {}, clientEntry: '/client/runtime.js' })`；`mount` 用「渲染前后子节点差集」追踪旧写法产物并返回 `RuntimeHandle`
- 设计约束：P0 不把 `@pulsar/plugin-client` 设为 external，插件 import 它会被内联；共享依赖切分（`imports` 非空 + import map）在 Step 3 一并接入
- 验收点：`hello-plugin` 不改一行即可运行；产物落在 `<artifactsDir>/dom/hello/<hash>/`，重启宿主后命中同一目录、不重新编译

### Step 3.（P0）宿主收缩

#### 文件：`packages/pulsar-plugin-host/src/types.ts`

- 改动类型：修改
- 改动内容：`PatchEntry.client` 扩展为 `string | { entry: string; runtime?: string }`；`PluginEntry` 增加 `module?: string`（后端入口）与可选 `apply`（由 `node` 适配器返回的 `server` 补齐）；`BootGraph` 增加 `runtimes` / `imports`，`BootPlugin` 的 `clientUrl` 替换为 `client: ClientDescriptor`
- 验收点：旧 profile 与旧类型调用方编译不报错

#### 文件：`packages/pulsar-plugin-host/src/profile.ts`

- 改动类型：修改
- 改动内容：归一化 `client` 的两种形态（字符串等价于 `{ entry, runtime: 'dom' }`）；`entry.runtime` 作为对象形态的兜底；`module` 一律由 `node` 适配器消费，不在 profile 层单独声明 runtime
- 验收点：旧 `profile.yml` 解析结果不变，`clientPath` 与 `clientRuntime` 正确

#### 文件：`packages/pulsar-plugin-host/src/host.ts`

- 改动类型：修改（净删除）
- 改动内容：
  - 新增 `registerRuntime()` 与 `runtimes` 注册表；**内建适配器不在 `host.ts` 注册**
  - `start()` 解析 `artifactsDir`（默认 `<cwd>/.pulsar/artifacts`，可由宿主配置覆盖），确保根目录存在，并注入 `RuntimeContext`
  - `load()` / `setEnabled()` 走同一条 `activate()`：调 `runtime.activate()`，把适配器注册的路由与监听收进 `cleanups`，失败时立即回滚
  - 删除 `bundleClient()`、`handleClientBundle()`、`clientBundles` 缓存及其路由
  - `bootGraph()` 汇总各适配器的 `clientEntry` 与 `imports`，同键不同 URL 抛错
  - `renderPage()` 在首个 module script 前注入唯一一张 import map
  - 宿主不删除、不遍历产物目录内部文件，只提供根路径与静态投递
- 设计约束：`host.ts` 内不得出现任何构建细节，也不得出现任何框架名；只 import `@pulsar/plugin-client/runtime` 契约
- 验收点：`host.ts` 中不再有 `esbuild`、`outputFiles`、`external`、`jsx` 等构建相关标识；也不 import 任何适配器实现

#### 文件：`packages/pulsar-plugin-host/src/index.ts`

- 改动类型：修改
- 改动内容：新增 `createHost()` 作为组装根——唯一允许同时 import 宿主与内建适配器（`NodeRuntime` / `DomRuntime`）的地方；适配器在此注入 `registerRuntime()`
- 设计约束：换框架或换内置适配器集合只改这一处，`host.ts` 不动
- 验收点：`host.ts` 的 import 列表中没有 `runtime/dom` / `runtime/node`

#### 文件：`packages/pulsar-plugin-host/client/runtime.js`

- 改动类型：修改（收敛为薄 bootstrap）
- 改动内容：只负责拉 boot graph、按 `plugin.client.assets` 先挂 CSS、`import()` 插件产物、apply；Slots/RPC/Events 逻辑的完整契约实现移入契约包，bootstrap 保留一份最小可用实现
- 验收点：能按 `client.url` 加载产物；CSS 产物在模块执行前已挂载

#### 文件：`packages/pulsar-plugin-host/src/verify.ts`

- 改动类型：修改
- 改动内容：新增断言——旧写法走 `dom`；`node` 适配器经 `activate()` 返回 `server`（Q5 = 统一）；boot graph 含唯一 import map 且覆盖全部 `imports`；插件禁用后适配器路由回滚但产物目录仍存在；重新启用命中同一 hash 目录、不重新编译、不重复渲染；同一插件连续编译 4 次后只保留 3 个 hash 目录
- 验收点：`pnpm verify` 全绿

### Step 4.（P1）生命周期与清理闭环

#### 文件：`packages/pulsar-plugin-client/src/slots.ts`

- 改动类型：修改
- 改动内容：`RenderRecord` 记住所属注册项；`register` 返回的 dispose 不只删注册表，还卸载该注册项在所有容器上的已渲染内容；`renderInto` 在挂载前清掉同容器同插槽的旧记录（因此嵌套 `renderSlot` 也幂等）；`SlotView.onCleanup` 收集的函数在卸载时 LIFO 执行
- 验收点：注册 → 渲染 → 重复渲染 → 注销后容器为空且无残留监听；重渲染触发旧实例 cleanups 一次

#### 文件：`packages/pulsar-plugin-client/src/legacy.ts`

- 改动类型：新增
- 改动内容：`legacy(component)` 显式包装旧 `(container, view)` 组件——渲染前 `replaceChildren()`，卸载时执行其返回的清理函数；不依赖 `component.length` 推断
- 验收点：包装后的组件在非空容器上渲染时不残留旧节点

#### 文件：`packages/pulsar-plugin-client/src/index.ts`

- 改动类型：修改
- 改动内容：导出 `legacy`；`createClientContext()` 允许复用外部 slots/api/events 实例
- 验收点：`import { legacy } from '@pulsar/plugin-client'` 可用

> 偏差（17:55）：插件级 dispose（`apply` 返回值）**推迟到 Step 6**实现。P1 阶段浏览器侧没有卸载入口，提前接线只会留下一段无人调用的代码；热替换路径需要它时再实现。

### Step 5.（P2）框架适配器与依赖共享

Q1 = A Solid 为官方实现；Vue / React 的方案保留在 `Solution Options` 作为接入参考，本期不实现。

#### 文件：`packages/pulsar-plugin-client/src/runtime/solid.ts`

- 改动类型：新增
- 改动内容：
  - `Compiler`：`esbuildCompiler({ plugins: () => [solidPlugin()], external: ['solid-js', 'solid-js/web', 'solid-js/store'], imports, clientEntry })`
  - `imports`：三条，指向适配器自己投递的 `/rt/vendor/solid/{solid,web,store}.js`
  - `activate()`：先 `ctx.effect` 挂 `/rt/vendor/solid/*` 路由（白名单三个文件，从 `solid-js` 包 dist 读），再 `super.activate()`
  - `mount/slot`：复用通用的 `mountNodes()`（Solid 组件编译后自己产出 DOM）
- 设计约束：框架实例只能来自 import map 提供的单例；新增适配器不得修改 `browser.ts` 与 `artifacts.ts`
- 依赖决策（18:10）：`solid-js` 与 `esbuild-plugin-solid` 作为本包 `dependencies` 直接 npm 安装，**不**收编进 `pulsar-kernel/vendor`；接受 `babel-preset-solid` 完整编译链
- 验收点：同一页面同时装载 `dom` 与 `solid` 两个插件；Solid 产物以 `from "solid-js"` 形式保留外部引用；`/rt/vendor/solid/solid.js` 可取；浏览器点击计数从 0 递增

#### 文件：`packages/pulsar-plugin-host/examples/solid-plugin/{client.tsx,index.ts}` + `profile/profile.yml`

- 改动类型：新增
- 改动内容：Solid 示例插件（`createRoot` + `insert` 挂载计数器），profile 中用对象形态声明 `client.runtime: solid`
- 验收点：`/api/boot` 同时包含 `hello`（dom）与 `solid`，`runtimes` 有两条，`imports` 含三条 solid 映射

#### 文件：`packages/pulsar-plugin-client/src/runtime/mount.ts`

- 改动类型：新增
- 改动内容：`mountNodes()`——执行组件、收集新增节点、卸载时移除；`dom` 与 `solid` 共用，避免复制
- 验收点：两个适配器的 `mount` 都不再各自实现节点记账

#### 文件：`packages/pulsar-plugin-host/src/verify.ts`

- 改动类型：修改
- 改动内容：新增 Solid 断言——产物 URL 形态、`runtimes.solid`、`imports` 三条映射、产物保留 `from "solid-js"`、vendor 白名单 404
- 验收点：`pnpm verify` 覆盖 dom + solid 双适配器共存

### Step 6.（P3）开发期热替换

#### 文件：`packages/pulsar-plugin-client/src/runtime/compiler/esbuild.ts`

- 改动类型：修改
- 改动内容：实现 `Compiler.watch()`——`esbuild.context()` + `watch()` + `onEnd` 回调；只报告「编译成功」，不落盘、不通知宿主、不管路由
- 设计约束：esbuild 的 context / watch 概念只能出现在本文件
- 验收点：本文件之外搜索不到 `esbuild.context` / `esbuild.watch`

#### 文件：`packages/pulsar-plugin-client/src/runtime/browser.ts`

- 改动类型：修改
- 改动内容：`activate()` 后若 `compiler.watch` 与 `ctx.notify` 同时可用，订阅 watch；变更时重走 `ensureArtifact()`，新 hash 挂新路由并 `ctx.notify({ type: 'plugin/changed', payload: { id, rev, url, assets } })`；旧路由与旧产物原样保留
- 设计约束：基类仍然只认 `compiler` 抽象；`watch` 为可选能力，缺失时降级为不支持热替换
- 验收点：`browser.ts` 中搜索不到 `esbuild`；改动插件前端后产物 hash 变化，其他插件产物不变

#### 文件：`packages/pulsar-plugin-client/src/runtime/types.ts`

- 改动类型：修改
- 改动内容：`RuntimeContext` 增加可选 `notify(change: RuntimeChange)`；新增 `RuntimeChange { type, payload? }`
- 验收点：不提供 `notify` 时适配器静默跳过热替换

#### 文件：`packages/pulsar-plugin-host/src/host.ts`

- 改动类型：修改
- 改动内容：新增 `HostOptions.dev`（默认 `NODE_ENV !== 'production'`）；仅开发模式注入 `notify`——先按 payload 刷新 boot graph 中的 `client.url`/`assets`（保证刷新页面也拿到最新产物），再转发 SSE `plugin/changed`
- 验收点：生产模式下不注入 `notify`，适配器不启动 watch

#### 文件：`packages/pulsar-plugin-host/client/runtime.js`

- 改动类型：修改
- 改动内容：按插件记录 `disposers` 与 `owned`（含 CSS link）；订阅 `plugin/changed` → `disposePlugin(id)` → 重新拉 boot graph → `import(newUrl)` → 重新 apply → 重渲染；CSS 随插件卸载一并移除
- 验收点：连续修改 3 次不产生重复 DOM、重复 CSS link 与重复 SSE 订阅

#### 文件：`docs/pulsar-plugin-host/plugin-guide.md`

- 改动类型：修改
- 改动内容：新增执行环境适配器概念、`defineClientPlugin`、生命周期、热替换、适配器接入说明
- 验收点：新插件作者按文档可跑通

### Step N. 检查与回写

#### 命令

- 运行：`pnpm --filter @pulsar/plugin-host typecheck`、`pnpm --filter @pulsar/plugin-client typecheck`、`pnpm --filter @pulsar/plugin-host verify`
- 修复：类型错误先改 `API Design` 再改代码

#### 文件：`docs/sdd-lab/2026-10-23_14-22_plugin-client-infra/lifecycle.md`

- 回写执行记录、实际改动摘要、验证结果、下一步状态

## Risk And Mitigation / 风险与缓解

- 风险：宿主收缩后仍残留隐性构建职责（例如 `renderPage` 里硬编码 vendor 路径）。
  - 缓解方式：Step 3 的验收点改为「搜索关键词为零」的机械检查：`esbuild`、`outputFiles`、`external`、`jsx`、`solid`、`vue`、`react` 在 `src/host.ts` 中均不出现。
- 风险：`Compiler` 接口被 esbuild 细节污染（例如把 `outputFiles`、`loader`、`jsx` 直接写进契约），导致换编译后端或接 Vue 时必须改接口。
  - 缓解方式：`Compiler` 只暴露 `fingerprint` / `compile` / 可选 `watch`，返回值统一收敛为 `files | external` 两种形态；`./runtime` 入口不依赖任何打包器，`esbuild` 只允许出现在 `compiler/esbuild.ts`；`vue` 走自定义 `Compiler` 或 `external` 即可验证接口未被污染。
- 风险：`imports` 漏配导致框架多实例（React hooks 报错最典型）。
  - 缓解方式：启动期自检 `imports` 覆盖全部已注册适配器的声明；verify 增加「实例唯一」反向断言。
- 风险：文件组产物模型在 P2 才引入，导致路由与产物布局重写。
  - 缓解方式：Q6 建议 P0 直接上，即便 P0 只有单文件产物，也走 `{ dir, hash, assets }` 结构。
- 风险：插件禁用时路由随 fiber 回收，但浏览器可能仍在请求旧 URL。
  - 缓解方式：产物文件不随插件删除，禁用只回收路由；重新启用时命中同一 hash 目录并恢复路由，无需重新编译。
- 风险：产物目录持续增长（每次源码变更新增一个 hash 目录）。
  - 缓解方式：Q7 = ① 已定，每个 `<runtime>/<pluginId>/` 保留最近 3 个 hash；`prune()` 只在新产物成功 rename 后触发并跳过当前 hash，不在卸载路径删除。`<cwd>/.pulsar/artifacts` 写入 `.gitignore`。
- 风险：多进程或并发激活同时编译同一插件，写盘竞争。
  - 缓解方式：`writeAtomic()` 先写 `<hash>.tmp/` 再 rename；同 hash 写入幂等，不同 hash 互不影响。
- 风险：多适配器共存时 `imports` 同键冲突（两个适配器声明不同版本的同一包）。
  - 缓解方式：冲突直接抛错并在启动期暴露，而不是静默以先注册者为准。
- 风险：旧签名判定（`component.length >= 2`）在压缩/包装后失真。
  - 缓解方式：P1 引入显式 `legacy()` 包装并写入迁移文档。
- 风险：根 workspace 与 plugin-host 局部 workspace 并存导致依赖解析不一致。
  - 缓解方式：Q4 = 根 workspace 已定；根 `pnpm-workspace.yaml` 的 `packages/*` 覆盖新包，`plugin-host` 以 `workspace:*` 引用，执行前用 `pnpm why esbuild` 确认宿主安装树无 esbuild。

## Execute Checkpoint / 执行检查点

- 当前理解：宿主收缩为「装配 + 生命周期 + 总线 + HTTP」的内核，不参与任何代码构建；`PluginRuntime` 作为执行环境适配器全权负责编译、存放、投递、执行；编译产物落盘到宿主约定的 `artifactsDir`，按 `runtime/plugin/hash` 隔离，不随插件销毁，运行期直接静态读取；`BrowserRuntime` 基类只做产物流水线（查盘、原子落盘、静态投递、路由回滚、boot graph 交付），编译由注入的 `Compiler` 策略完成，基类与 `artifacts.ts` 都不认识任何打包器。
- 核心目标：P0 建包与宿主收缩、P1 生命周期闭环、P2 框架适配器与依赖共享、P3 单插件热替换。
- 下一步动作：Q1 ~ Q7 已全部关闭，`Decision` 已填写。等待用户批准后把 `lifecycle.md` 转 `executing`，从 Step 1 开始。
- 已关闭的关键分歧：Q1 = A Solid；Q2 = 允许多适配器共存；Q4 = 根 workspace；Q5 = 后端统一走 `PluginRuntime`；Q6 = 文件组产物模型进 P0；Q7 = 固定路径 + 保留最近 3 个；Q3 因选 Solid 不适用。
