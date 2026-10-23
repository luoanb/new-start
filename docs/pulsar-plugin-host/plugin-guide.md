# pulsar-plugin-host 插件开发与运行攻略

面向**插件消费者**：你只想写一个 Host/Client 插件、装进宿主、跑起来，不需要了解宿主内部实现。

## 0. 你能得到什么

- 一个 Host 进程（Cordis 插件树 + HTTP 服务）
- 一个浏览器运行时（第二棵插件树 + Slots）
- 插件通过 profile 安装；禁用即整体回滚
- Host 与 Client 之间可用 RPC 调用、SSE 事件
- Client 代码由**执行环境适配器**（`dom` / `solid`）编译、落盘并静态投递，宿主不打包

当前**没有**：npm 仓库安装、桌面壳。安装 = 把插件目录放进仓库 + 写 profile。

---

## 1. 五分钟：只写 Host 面

### 1.1 目录

```text
plugins/my-plugin/
└── index.ts
```

### 1.2 最小插件

```ts
import type { Context } from '@deepseek-ai/cordis'

export const myPlugin = {
  inject: ['router'],
  apply(ctx: Context) {
    ctx.effect(() => ctx.router.get('/my-plugin/ping', (_req, res) => {
      res.writeHead(200, { 'content-type': 'application/json; charset=utf-8' })
      res.end(JSON.stringify({ ok: true }))
    }), 'my plugin route')
  },
}
```

规则：

- 模块默认导出，或任意具名导出中带 `apply` 的对象，宿主都能识别。
- 插件名由 profile 的 `id` 决定，不取代码里的变量名。
- **所有可撤销的注册都必须包在 `ctx.effect()` 里**；禁用插件时只会回滚 effect。
- `inject` 写宿主提供的服务名，缺一个插件就不会启动。

### 1.3 装进 profile

编辑 `packages/pulsar-plugin-host/profile/profile.yml`：

```yaml
plugins:
  - id: my-plugin
    module: ../plugins/my-plugin/index.ts
```

`module` 路径**相对于写它的 YAML 文件**。宿主默认读 `packages/pulsar-plugin-host/profile/`。

### 1.4 运行

```bash
cd packages/pulsar-plugin-host
pnpm install     # 首次
pnpm start
```

打开终端打印的地址，访问 `http://127.0.0.1:<port>/my-plugin/ping`。

---

## 2. 增加 Client 面

同一个插件可以再提供一个浏览器侧入口。

### 2.1 目录

```text
plugins/my-plugin/
├── index.ts     # Host 面
└── client.ts    # Client 面
```

### 2.2 Client 插件

```ts
export function apply(ctx: {
  slots: {
    register(
      spec: { name: string; id: string; order?: number; children?: string[] },
      component: (container: HTMLElement, view: {
        renderSlot(name: string, el: HTMLElement, owner?: unknown): void
      }) => void,
    ): () => void
  }
  api: { call(method: string, args?: unknown): Promise<any> }
  events: { on(type: string, handler: (payload: unknown) => void): () => void }
  locale: { t(key: string): string; messages: Record<string, string> }
}) {
  ctx.slots.register({ name: 'main', id: 'my-panel' }, (container) => {
    const section = document.createElement('article')
    const title = document.createElement('h2')
    title.textContent = 'My plugin'
    const button = document.createElement('button')
    button.textContent = 'Ping host'
    button.addEventListener('click', async () => {
      const result = await ctx.api.call('my.ping', {})
      title.textContent = result.message
    })
    section.append(title, button)
    container.append(section)
  })
}
```

### 2.3 在 profile 里声明 client

```yaml
plugins:
  - id: my-plugin
    module: ../plugins/my-plugin/index.ts
    client: ../plugins/my-plugin/client.ts
```

`client` 可选；没有它就只是纯 Host 插件。字符串形态等价于 `runtime: dom`。

需要框架时用对象形态，`runtime` 取执行环境适配器名：

```yaml
  - id: my-solid-plugin
    module: ../plugins/my-solid-plugin/index.ts
    client:
      entry: ../plugins/my-solid-plugin/client.tsx
      runtime: solid
```

| runtime | 适用 | 说明 |
|---|---|---|
| `dom`（默认） | 手写 DOM / `client.ts` | 旧 `(container, view)` 写法零改动 |
| `solid` | Solid 组件 / `client.tsx` | 官方一等公民，`solid-js` 由页面 import map 共享 |

### 2.4 编译与产物

- Client 面由**执行环境适配器**编译成文件组产物（入口 + CSS + 其他资源），不再由宿主打包。
- 产物落在 `<cwd>/.pulsar/artifacts/<runtime>/<pluginId>/<hash>/`，带 `manifest.json` 与 `latest.json`；**不随插件卸载删除**，每个插件保留最近 3 个版本。
- 同一份源码再次启动会命中已有目录，不重新编译。
- 不能用 Node API（`fs`、`process` 等）。
- 修改 Client 源码后：开发模式（默认）会自动重新编译并通过 SSE 热替换该插件；生产模式需重启宿主。

---

## 3. 槽位（Slots）怎么用

宿主页面只有一棵根组件，它声明了一个子槽：

```text
root
└─ main
```

### 3.1 注册到已有槽

```ts
ctx.slots.register({ name: 'main', id: 'my-panel' }, component)
```

- `id` 全局唯一，重复注册会直接报错。
- `order` 升序排列，默认 0。
- 注册阶段不校验槽是否存在；启动完成后如果该槽无人声明，组件不会渲染，页面会显示“注册到了无人声明的槽”错误卡片。

### 3.2 声明自己的子槽

如果你的组件需要留扩展点，在注册时声明 `children`，并在组件里渲染它：

```ts
ctx.slots.register(
  { name: 'main', id: 'my-panel', children: ['my-panel.toolbar'] },
  (container, view) => {
    const toolbar = document.createElement('div')
    container.append(toolbar)
    view.renderSlot('my-panel.toolbar', toolbar)
  },
)
```

其他插件随后可以注册到 `my-panel.toolbar`。

### 3.3 scope

| scope | 渲染条件 |
|---|---|
| `root`（默认） | 总是渲染 |
| `scoped-maybe` | 总是渲染，但可以读到可选 `scopeId` |
| `scoped` | 只有当前渲染位置带有 `scopeId` 时渲染 |

默认根渲染没有 `scopeId`。需要作用域时，由父组件显式传入（父注册需先声明 `children: ['my-panel.detail']`）：

```ts
view.renderSlot('my-panel.detail', el, { itemId: 'a' }, { scopeId: 'item-a' })
```

对应的注册：

```ts
ctx.slots.register({ name: 'my-panel.detail', id: 'detail', scope: 'scoped' }, component)
```

---

## 4. Host 能力速查

### 4.1 HTTP 路由

```ts
// 需要 inject: ['router']
ctx.effect(() => ctx.router.get('/path', (req, res) => { /* ... */ }), 'route')
```

- 精确路径：`router.get`
- 前缀路径：`router.prefix('/assets/', handler)`（同样用 `ctx.effect` 包住返回值）
- 处理器签名：`(req: IncomingMessage, res: ServerResponse)`

### 4.2 RPC

Host：

```ts
ctx.effect(() => ctx.rpc.method('my.ping', (args) => {
  return { message: `pong ${(args as any)?.name ?? ''}` }
}), 'rpc')
```

Client：

```ts
const result = await ctx.api.call('my.ping', { name: 'a' })
```

- 参数与返回值必须是 JSON 可序列化的。
- 方法名全局唯一。
- 未注册的方法返回 HTTP 500。
- 禁用插件后方法立即消失。

### 4.3 事件

Host：

```ts
ctx.clientEvents.emit('my/changed', { at: Date.now() })
```

Client：

```ts
const off = ctx.events.on('my/changed', (payload) => console.log(payload))
```

- 只下发，不重放；页面晚于事件到达就收不到。
- 返回的 `off()` 用于取消订阅。

### 4.4 面板（可选）

```ts
// 需要 inject: ['panels']
ctx.effect(() => ctx.panels.add({ id: 'my-panel', title: 'My', body: '...' }), 'panel')
```

面板只出现在 `/api/state` 的服务端枚举里，**不会自动渲染到页面**。要 UI 请用 Slots。

### 4.5 配置

```yaml
- id: my-plugin
  module: ../plugins/my-plugin/index.ts
  config:
    greeting: hi
```

```ts
export const myPlugin = {
  inject: ['router'],
  apply(ctx: Context, config?: { greeting?: string }) {
    // config.greeting
  },
}
```

---

## 5. 安装、启停与路径规则

### 5.1 安装

当前版本没有注册表安装，步骤是：

1. 把插件目录放到你想放的位置（推荐 `packages/pulsar-plugin-host/plugins/<name>/`）。
2. 在 profile 或 bundle 补丁里加一行。
3. 重启宿主。

### 5.2 分层

```yaml
# profile.yml
bundles:
  - ./bundles/core.patch.yml
plugins:
  - id: my-plugin
    module: ../plugins/my-plugin/index.ts
```

- bundle 按数组顺序应用，profile 自己的 `plugins` 最后应用。
- 同一 `id` 后出现的层**整条替换**前面的层，不是字段合并。
- `disabled: true` 的条目会被过滤。

### 5.3 运行时启停

目前只有程序化接口：

```ts
await host.setEnabled('my-plugin', false)
await host.setEnabled('my-plugin', true)
```

禁用会销毁插件 fiber，路由、RPC、面板与客户端启动图条目一起消失；条目仍保留在 profile 里。

---

## 6. 调试

| 目标 | 方法 |
|---|---|
| 看插件是否装载 | `GET /api/state`，检查 `enabled`、`client`、`rpc` |
| 看客户端启动图 | `GET /api/boot`（含 `client.url`、`client.assets`、`runtimes`、`imports`） |
| 看编译产物 | `GET /rt/<runtime>/<pluginId>/<hash>/client.js`，磁盘上同一路径 |
| 手动调 RPC | `curl -X POST -H 'content-type: application/json' -d '{"method":"my.ping","args":{}}' http://127.0.0.1:<port>/api/rpc` |
| 页面渲染问题 | 浏览器控制台 + 页面上的“插件加载失败”卡片 |
| 产物为何重新编译 | 对比 `<hash>/manifest.json` 的 `inputs` 与源码内容 hash |

自动化验收参考 `src/verify.ts`。

---

## 7. 排错表

| 现象 | 原因 | 处理 |
|---|---|---|
| 启动时报 `no plugin export found` | 模块没有默认导出，也没有带 `apply` 的具名导出 | 导出 `{ apply }` 对象 |
| 插件一直不生效 | `inject` 写了不存在的服务 | 只用 `router` / `panels` / `rpc` / `clientEvents` / `logger` |
| 往槽注册时报 `slot not declared` | 父组件没有在 `children` 里声明该槽 | 在父注册里补 `children` |
| 槽注册成功但页面不显示 | 用了 `scope: 'scoped'` 但父级没传 `scopeId` | 改 `root`/`scoped-maybe`，或由父级传 `{ scopeId }` |
| `/rt/<runtime>/<id>/<hash>/client.js` 404 | 插件被禁用，或 runtime 名写错 | 检查 `/api/boot` 与 `/api/state` |
| `runtime not registered: xxx` | `client.runtime` 写了宿主没注册的适配器 | 用 `dom` 或 `solid` |
| RPC 返回 500 `unknown rpc method` | 方法没注册，或插件被禁用 | 检查 `ctx.rpc.method` 与 `/api/state` 的 `rpc` 列表 |
| 改了 Client 代码页面没变 | 生产模式没开热替换 | 用默认开发模式启动，或重启宿主 |
| Solid 插件报重复实例 / hooks 失效 | 产物没有走 import map，自行打包了 `solid-js` | 不要把 `solid-js` 从 `external` 里去掉 |
| `ERR_PNPM_LOCKFILE_CONFIG_MISMATCH` | 改过 `pnpm-workspace.yaml` 的 overrides | `pnpm install --no-frozen-lockfile` |

---

## 8. 安全与边界

- 插件与宿主**同进程**，拥有宿主权限；只安装可信插件。
- Host 只监听 `127.0.0.1`，但本机其他进程仍可访问端口；生产桌面壳需要额外 token。
- 不要执行用户输入拼出来的命令；插件不是沙箱。
- 不要把业务逻辑写进宿主本身；业务只属于插件。

---

## 9. 从示例开始

最快路径：

```bash
cp -r packages/pulsar-plugin-host/examples/hello-plugin packages/pulsar-plugin-host/plugins/my-plugin
```

然后：

1. 改 `index.ts` 的 `apply` 内容（Host 面）；
2. 改 `client.ts` 的槽组件（Client 面）；
3. 在 `profile/profile.yml` 加 `my-plugin` 条目；
4. `pnpm start`。
