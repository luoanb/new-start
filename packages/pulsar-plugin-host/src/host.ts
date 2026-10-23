import { mkdir, readFile } from 'node:fs/promises'
import { createServer, type IncomingMessage, type Server, type ServerResponse } from 'node:http'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { Context, type Fiber } from '@deepseek-ai/cordis'
import ConsoleLogger from '@deepseek-ai/cordis-plugin-logger-console'
import Timer from '@deepseek-ai/cordis-plugin-timer'
import type {
  Activation,
  ClientDescriptor,
  Logger,
  PluginObject,
  PluginRuntime,
  RuntimeChange,
  RuntimeContext,
} from '@pulsar/plugin-client/runtime'
import { ClientEvents } from './events.ts'
import { PanelService } from './panels.ts'
import { loadProfile } from './profile.ts'
import { RouterService } from './router.ts'
import { RpcService } from './rpc.ts'
import type { BootGraph, BootPlugin, HostState, PluginEntry } from './types.ts'

export interface HostOptions {
  /** 监听地址，默认 127.0.0.1 */
  host?: string
  /** 监听端口，0 表示由系统分配 */
  port?: number
  /** 编译产物根目录，默认 `<cwd>/.pulsar/artifacts` */
  artifactsDir?: string
  /** 开发模式：启用适配器热替换；默认 `NODE_ENV !== 'production'` */
  dev?: boolean
}

const RUNTIME_PATH = fileURLToPath(new URL('../client/runtime.js', import.meta.url))

/** 一次装载后保留的状态；重新启用时复用同一份描述。 */
interface LoadedEntry {
  source: PluginEntry
  client?: ClientDescriptor
  /** 适配器注册的路由与监听，随插件 fiber 回滚。 */
  cleanups: Array<() => void>
}

/**
 * 插件宿主：一棵 Cordis 插件树 + 一个 HTTP 服务 + 浏览器侧的插件装载图。
 *
 * Host 只负责装配、生命周期与 RPC/事件；编译、存放与投递交给
 * `PluginRuntime`（执行环境适配器），宿主不 import 任何打包器。
 */
export class PluginHost {
  readonly root = new Context()

  private readonly entries = new Map<string, PluginEntry>()
  private readonly loaded = new Map<string, LoadedEntry>()
  private readonly fibers = new Map<string, Fiber>()
  private readonly runtimes = new Map<string, PluginRuntime>()
  private readonly pluginRpc = new Map<string, string[]>()

  private artifactsDir = resolve(process.cwd(), '.pulsar/artifacts')
  private dev = true
  private server: Server | undefined
  private url: string | undefined
  private booted = false
  private bootRev = 0

  /** 注册一个执行环境适配器；同名覆盖视为错误。 */
  registerRuntime(runtime: PluginRuntime): void {
    if (this.runtimes.has(runtime.name)) {
      throw new Error(`runtime already registered: ${runtime.name}`)
    }
    this.runtimes.set(runtime.name, runtime)
  }

  /** 启动宿主并注册内建端点；重复调用返回同一地址。 */
  async start(options: HostOptions = {}): Promise<{ url: string }> {
    if (this.url) return { url: this.url }

    this.artifactsDir = options.artifactsDir ?? resolve(process.cwd(), '.pulsar/artifacts')
    this.dev = options.dev ?? process.env.NODE_ENV !== 'production'
    await mkdir(this.artifactsDir, { recursive: true })

    await this.root.plugin(ConsoleLogger)
    await this.root.plugin(Timer)
    await this.root.plugin(RouterService)
    await this.root.plugin(PanelService)
    await this.root.plugin(RpcService)
    await this.root.plugin(ClientEvents)

    const host = options.host ?? '127.0.0.1'
    this.registerBuiltinRoutes()

    const server = createServer((req, res) => {
      void this.handle(req, res)
    })
    await new Promise<void>((resolve, reject) => {
      server.once('error', reject)
      server.listen(options.port ?? 0, host, () => resolve())
    })

    const address = server.address()
    const port = typeof address === 'object' && address ? address.port : 0
    this.server = server
    this.url = `http://${host}:${port}`
    this.booted = true
    return { url: this.url }
  }

  /** 按 profile 的 bundle/patch 层装配插件。 */
  async loadProfile(dir: string): Promise<void> {
    for (const entry of await loadProfile(dir)) {
      await this.load({
        name: entry.id,
        module: entry.modulePath,
        config: entry.config,
        client: entry.clientPath
          ? { entry: entry.clientPath, runtime: entry.clientRuntime }
          : undefined,
      })
    }
  }

  /** 装载一个插件条目；插件名重复、宿主未启动或加载失败时抛错。 */
  async load(entry: PluginEntry): Promise<void> {
    if (!this.booted) throw new Error('host has not started')
    if (this.entries.has(entry.name)) {
      throw new Error(`plugin already registered: ${entry.name}`)
    }

    this.entries.set(entry.name, entry)
    try {
      const loaded = await this.activate(entry)
      this.loaded.set(entry.name, loaded)
      await this.mountFiber(entry, loaded)
      this.bump()
    } catch (error) {
      this.entries.delete(entry.name)
      this.loaded.delete(entry.name)
      this.pluginRpc.delete(entry.name)
      throw error
    }
  }

  /** 卸载插件贡献；条目保留，便于再次启用。 */
  async unload(name: string): Promise<void> {
    const fiber = this.fibers.get(name)
    if (!fiber) return
    this.fibers.delete(name)
    this.pluginRpc.delete(name)
    this.loaded.delete(name)
    await fiber.dispose()
    this.bump()
  }

  /** 运行时启停一个已注册条目。 */
  async setEnabled(name: string, enabled: boolean): Promise<void> {
    const entry = this.entries.get(name)
    if (!entry) throw new Error(`unknown plugin: ${name}`)
    if (enabled === this.fibers.has(name)) return

    if (enabled) {
      const loaded = await this.activate(entry)
      this.loaded.set(name, loaded)
      await this.mountFiber(entry, loaded)
      this.bump()
    } else {
      await this.unload(name)
    }
  }

  /** 当前插件与贡献快照。 */
  state(): HostState {
    return {
      plugins: [...this.entries.keys()].map((name) => ({
        name,
        enabled: this.fibers.has(name),
        client: this.loaded.get(name)?.client !== undefined,
        rpc: this.pluginRpc.get(name) ?? [],
      })),
      panels: this.root.panels.list(),
    }
  }

  /** 浏览器启动图：只包含当前已启用且声明了 Client 面的插件。 */
  bootGraph(): BootGraph {
    const plugins: BootPlugin[] = []
    const runtimes: Record<string, string> = {}
    const imports: Record<string, string> = {}

    for (const [id, loaded] of this.loaded) {
      if (!this.fibers.has(id) || !loaded.client) continue
      plugins.push({ id, client: loaded.client })

      const runtimeName = loaded.source.client?.runtime ?? 'dom'
      const runtime = this.runtimes.get(runtimeName)
      if (runtime?.clientEntry) runtimes[runtimeName] = runtime.clientEntry
      this.mergeImports(imports, runtime?.imports)
    }

    return { rev: String(this.bootRev), plugins, runtimes, imports }
  }

  /** 关闭 HTTP 服务并销毁整棵插件树。 */
  async stop(): Promise<void> {
    if (this.server) {
      const server = this.server
      this.server = undefined
      const closed = new Promise<void>((resolve) => server.close(() => resolve()))
      // keep-alive 连接会阻止 close 回调，需要主动断开。
      server.closeAllConnections()
      await closed
    }
    if (this.booted) {
      this.booted = false
      await this.root.fiber.dispose()
    }
    this.fibers.clear()
    this.entries.clear()
    this.loaded.clear()
    this.pluginRpc.clear()
    this.url = undefined
  }

  /** 调用适配器完成「编译 → 存放 → 投递」，收集需随 fiber 回滚的清理项。 */
  private async activate(entry: PluginEntry): Promise<LoadedEntry> {
    const cleanups: Array<() => void> = []
    const ctx = this.runtimeContext(entry.name, cleanups)
    try {
      return await this.activateWith(entry, ctx, cleanups)
    } catch (error) {
      // 激活失败时适配器可能已注册路由，必须立即回滚。
      for (let index = cleanups.length - 1; index >= 0; index -= 1) cleanups[index]!()
      throw error
    }
  }

  private async activateWith(
    entry: PluginEntry,
    ctx: RuntimeContext,
    cleanups: Array<() => void>,
  ): Promise<LoadedEntry> {
    if (entry.module) {
      const runtime = this.runtimes.get('node')
      if (!runtime) throw new Error('runtime not registered: node')
      const activation = await runtime.activate(
        { id: entry.name, entry: entry.module, options: entry.config },
        ctx,
      )
      if (activation.server) adoptServer(entry, activation.server)
      collect(cleanups, activation)
    }

    if (entry.client) {
      const runtimeName = entry.client.runtime ?? 'dom'
      const runtime = this.runtimes.get(runtimeName)
      if (!runtime) throw new Error(`runtime not registered: ${runtimeName}`)
      const activation = await runtime.activate(
        { id: entry.name, entry: entry.client.entry, options: entry.config },
        ctx,
      )
      collect(cleanups, activation)
      return { source: entry, client: activation.client, cleanups }
    }

    return { source: entry, cleanups }
  }

  /** 把适配器注册的清理项交给插件 fiber，并装载后端插件。 */
  private async mountFiber(entry: PluginEntry, loaded: LoadedEntry): Promise<void> {
    if (!entry.apply) throw new Error(`plugin ${entry.name} has no apply export`)

    const before = new Set(this.root.rpc.names())
    const plugin = {
      ...entry,
      apply: async (ctx: Context, config?: any) => {
        for (const cleanup of loaded.cleanups) ctx.effect(() => cleanup)
        await entry.apply!(ctx, config)
      },
    }
    const fiber = await this.root.plugin(plugin, entry.config)
    this.fibers.set(entry.name, fiber)
    this.pluginRpc.set(
      entry.name,
      this.root.rpc.names().filter((name) => !before.has(name)),
    )
  }

  /** 适配器看到的受限上下文：路由与监听先收集，等进入 fiber 后统一挂载。 */
  private runtimeContext(entryName: string, cleanups: Array<() => void>): RuntimeContext {
    return {
      artifactsDir: this.artifactsDir,
      logger: createLogger(),
      effect: (execute) => {
        const dispose = execute()
        if (typeof dispose === 'function') cleanups.push(dispose)
        return () => {}
      },
      route: (pattern, handler) => this.root.router.prefix(prefixOf(pattern), handler),
      // 开发期热替换：宿主只更新描述并转发事件，不解释变更原因。
      ...(this.dev
        ? {
          notify: (change: RuntimeChange) => {
            if (change.type === 'plugin/changed') this.applyClientChange(entryName, change.payload)
            this.root.clientEvents.emit(change.type, change.payload)
          },
        }
        : {}),
    }
  }

  /** 用适配器上报的新 URL 刷新 boot graph，让页面刷新也能拿到最新产物。 */
  private applyClientChange(id: string, payload: unknown): void {
    const loaded = this.loaded.get(id)
    const next = payload as { url?: unknown; assets?: unknown } | undefined
    if (!loaded?.client || typeof next?.url !== 'string') return
    loaded.client = {
      url: next.url,
      assets: Array.isArray(next.assets) ? next.assets as string[] : loaded.client.assets,
    }
    this.bump()
  }

  /** 合并适配器声明的共享依赖；同键不同 URL 直接失败。 */
  private mergeImports(target: Record<string, string>, source?: Record<string, string>): void {
    for (const [name, url] of Object.entries(source ?? {})) {
      const existing = target[name]
      if (existing && existing !== url) {
        throw new Error(`import map conflict for "${name}": ${existing} vs ${url}`)
      }
      target[name] = url
    }
  }

  private bump(): void {
    this.bootRev += 1
  }

  private registerBuiltinRoutes(): void {
    this.root.router.get('/health', (_req, res) => {
      res.writeHead(200, { 'content-type': 'text/plain; charset=utf-8' })
      res.end('ok')
    })

    this.root.router.get('/api/state', (_req, res) => {
      this.sendJson(res, this.state())
    })

    this.root.router.get('/api/boot', (_req, res) => {
      this.sendJson(res, this.bootGraph())
    })

    this.root.router.get('/api/events', (req, res) => {
      res.writeHead(200, {
        'content-type': 'text/event-stream; charset=utf-8',
        'cache-control': 'no-cache',
        connection: 'keep-alive',
      })
      res.write(': connected\n\n')
      this.root.clientEvents.subscribe(res)
      req.on('close', () => {
        res.end()
      })
    })

    this.root.router.get('/client/runtime.js', async (_req, res) => {
      const source = await readFile(RUNTIME_PATH, 'utf8')
      res.writeHead(200, { 'content-type': 'application/javascript; charset=utf-8' })
      res.end(source)
    })

    this.root.router.get('/api/rpc', async (req, res) => {
      if (req.method !== 'POST') {
        this.sendJson(res, { error: 'method not allowed' }, 405)
        return
      }
      const body = await this.readJson(req) as { method?: unknown; args?: unknown } | undefined
      if (!body || typeof body.method !== 'string') {
        this.sendJson(res, { error: 'missing method' }, 400)
        return
      }
      const result = await this.root.rpc.invoke(body.method, body.args)
      this.sendJson(res, { result })
    })

    this.root.router.get('/', (_req, res) => {
      res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' })
      res.end(this.renderPage())
    })
  }

  private async handle(req: IncomingMessage, res: ServerResponse): Promise<void> {
    try {
      if (await this.root.router.dispatch(req, res)) return
      this.sendJson(res, { error: 'not found' }, 404)
    } catch (error) {
      // 处理器已写出响应头时不能再写 JSON，否则会在 catch 里二次抛错。
      if (res.headersSent) {
        res.end()
        return
      }
      this.sendJson(res, {
        error: error instanceof Error ? error.message : String(error),
      }, 500)
    }
  }

  private async readJson(req: IncomingMessage): Promise<unknown> {
    const chunks: Buffer[] = []
    for await (const chunk of req) chunks.push(chunk as Buffer)
    const text = Buffer.concat(chunks).toString('utf8')
    return text ? JSON.parse(text) : undefined
  }

  private sendJson(res: ServerResponse, body: unknown, status = 200): void {
    res.writeHead(status, { 'content-type': 'application/json; charset=utf-8' })
    res.end(JSON.stringify(body))
  }

  /** 页面只做一件事：先输出唯一 import map，再加载薄 bootstrap。 */
  private renderPage(): string {
    const importMap = JSON.stringify({ imports: this.bootGraph().imports })
    return `<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>pulsar-plugin-host</title>
<script type="importmap">${importMap}</script>
<style>
  :root { color-scheme: light dark; }
  body { font: 14px/1.6 system-ui, sans-serif; margin: 0; }
  header { padding: 16px 24px; border-bottom: 1px solid #8884; font-weight: 600; }
  main { padding: 24px; display: grid; gap: 16px; }
  article { border: 1px solid #8884; border-radius: 8px; padding: 16px; }
  article h2 { margin: 0 0 8px; font-size: 16px; }
  .muted { opacity: .6; }
  button { font: inherit; padding: 4px 12px; cursor: pointer; }
</style>
</head>
<body>
<div id="app"><p class="muted" style="padding:24px">正在启动插件树…</p></div>
<script type="module" src="/client/runtime.js"></script>
</body>
</html>`
  }
}

/** 把适配器返回的 `server` 上的插件声明合并进条目，交给 Cordis 装载。 */
function adoptServer(entry: PluginEntry, server: PluginObject): void {
  const source = server as { apply?: unknown; inject?: unknown }
  entry.apply ??= typeof source.apply === 'function'
    ? source.apply as PluginEntry['apply']
    : undefined
  entry.inject ??= Array.isArray(source.inject) ? source.inject as string[] : undefined
}

function collect(cleanups: Array<() => void>, activation: Activation): void {
  const dispose = activation.dispose
  if (typeof dispose === 'function') cleanups.push(() => void dispose())
}

/** 把适配器的 glob 形态（`/rt/.../*`）转成 RouterService 的前缀。 */
function prefixOf(pattern: string): string {
  return pattern.endsWith('*') ? pattern.slice(0, -1) : pattern
}

function createLogger(): Logger {
  return {
    info: (message) => console.info(message),
    warn: (message) => console.warn(message),
    error: (message) => console.error(message),
  }
}
