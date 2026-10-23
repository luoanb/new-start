# pulsar-host

插件宿主平台：**只有机制，没有业务**。Host 与 Browser 各一棵插件树，
由 profile/bundle/patch 装配、通过 RPC 与事件流连接。

## 定位

| | 内容 |
|---|---|
| 框架源 | `packages/pulsar-kernel/vendor`（workspace 直接引用，**不复制**） |
| 本包 | 插件装配、Host 生命周期、Client 装载、Slots、RPC/事件 |
| 不包含 | `core/*`、`llm/*`、`session`、`tools`、任何业务插件 |

## 机制

1. **装配**：`profile.yml` → 有序 bundle 补丁 → profile 自身补丁；同一 id 后层整条替换，`disabled: true` 过滤。
2. **Host 插件树**：Cordis fiber，贡献必须走 `ctx.effect`，禁用即整体回滚。
3. **Client 装载**：`/api/boot` 下发启动图；Host 用 esbuild 按需把插件 Client 面打成单文件 ESM；浏览器逐个 `import()` 后执行 `apply`。
4. **Slots**：`root / scoped-maybe / scoped` 三种 scope；父级用 `children` 声明子槽，未声明直接报错；组件不能跨包引用。
5. **RPC 与事件**：`POST /api/rpc` 调用 Host 方法；`GET /api/events` 以 SSE 下发 Host 事件。

## 结构

```text
pulsar-host/
├── src/
│   ├── host.ts        # PluginHost：Cordis 树 + HTTP + 装配 + 浏览器启动图
│   ├── profile.ts     # profile / bundle / patch 分层装配
│   ├── router.ts      # HTTP 路由注册表（精确 + 前缀）
│   ├── rpc.ts         # RPC 方法注册表
│   ├── events.ts      # Host → Browser SSE 事件流
│   ├── panels.ts      # 服务端侧贡献枚举（Phase 1 遗留，仍用于 /api/state）
│   ├── types.ts       # PluginEntry / PatchEntry / BootGraph
│   ├── index.ts       # 入口：读 profile 并起宿主
│   └── verify.ts      # 自动化验收
├── client/
│   └── runtime.js     # 浏览器运行时：boot graph、Slots、RPC、事件
├── examples/hello-plugin/
│   ├── index.ts       # Host 面：路由、面板、RPC
│   └── client.ts      # Client 面：注册 main 槽并调用 RPC
└── profile/
    ├── profile.yml
    └── bundles/core.patch.yml
```

## 命令

```sh
pnpm install
pnpm typecheck
pnpm start            # 读 profile/，终端打印监听地址
pnpm verify           # 非交互验收
```

`PORT=8080 pnpm start` 固定端口；`PROFILE_DIR=/path/to/profile pnpm start` 使用其他 profile。

## 插件契约

```ts
// Host 面
export interface PluginEntry {
  name: string
  inject?: string[]
  apply: (ctx: Context) => void | Promise<void>
  config?: any
  client?: string          // Client 面入口，由 profile 解析
}

// Client 面
export function apply(ctx: {
  slots: { register(spec, component): () => void }
  api: { call(method, args): Promise<unknown> }
  events: { on(type, handler): () => void }
  locale: { t(key): string; messages: Record<string, string> }
})
```

- Host 注册：`ctx.effect(() => ctx.rpc.method(...))`、`ctx.effect(() => ctx.router.get(...))`
- Client 注册：`ctx.slots.register({ name, id, scope?, children? }, component)`
- 禁用插件：Host fiber 销毁，路由/RPC/面板同时回滚；`/api/boot` 不再下发 Client 面

## 文档

- 插件开发 / 安装 / 运行攻略（消费者视角）：`docs/pulsar-host/plugin-guide.md`

## 阶段

- Phase 1：Host 薄层 + hello 插件 + 装卸验收（完成）
- Phase 2：Client 装载 + Slots 泛化 + RPC/事件（完成）
- Phase 3：profile/bundle/patch 装配（完成，机制等价实现）

未搬项（有意不包含）：Typert 代码生成、React 渲染器、客户端 HMR、pnpm 插件安装、Electron 桌面壳。

## 许可

框架源码来自 `packages/pulsar-kernel/vendor`（上游 MIT），本包不复制、不修改它。
