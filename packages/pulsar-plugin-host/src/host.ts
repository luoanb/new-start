import { readFile } from 'node:fs/promises'
import { createServer, type IncomingMessage, type Server, type ServerResponse } from 'node:http'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { Context, type Fiber } from '@deepseek-ai/cordis'
import ConsoleLogger from '@deepseek-ai/cordis-plugin-logger-console'
import Timer from '@deepseek-ai/cordis-plugin-timer'
import { build } from 'esbuild'
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
}

const RUNTIME_PATH = fileURLToPath(new URL('../client/runtime.js', import.meta.url))

/**
 * 插件宿主：一棵 Cordis 插件树 + 一个 HTTP 服务 + 浏览器侧的插件装载图。
 *
 * Host 面负责装配、生命周期与 RPC/事件；Client 面由 `/api/boot` 描述的
 * 插件列表驱动，浏览器运行时逐个加载 ES module 并注册 Slots。
 */
export class PluginHost {
  readonly root = new Context()

  private readonly entries = new Map<string, PluginEntry>()
  private readonly fibers = new Map<string, Fiber>()
  private readonly clientPaths = new Map<string, string>()
  private readonly clientBundles = new Map<string, string>()
  private readonly pluginRpc = new Map<string, string[]>()

  private server: Server | undefined
  private url: string | undefined
  private booted = false
  private bootRev = 0

  /** 启动宿主并注册内建端点；重复调用返回同一地址。 */
  async start(options: HostOptions = {}): Promise<{ url: string }> {
    if (this.url) return { url: this.url }

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
      const module = await import(pathToFileURL(entry.modulePath).href)
      const plugin = resolvePluginExport(module)
      if (!plugin) {
        throw new Error(`no plugin export found in ${entry.modulePath}`)
      }
      await this.load({
        ...plugin,
        name: entry.id,
        config: entry.config,
        client: entry.clientPath,
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
    const before = new Set(this.root.rpc.names())
    try {
      const fiber = await this.root.plugin(entry, entry.config)
      this.fibers.set(entry.name, fiber)
      this.pluginRpc.set(
        entry.name,
        this.root.rpc.names().filter((name) => !before.has(name)),
      )
      if (entry.client) this.clientPaths.set(entry.name, entry.client)
      this.bump()
    } catch (error) {
      this.entries.delete(entry.name)
      this.clientPaths.delete(entry.name)
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
    this.clientBundles.delete(name)
    await fiber.dispose()
    this.bump()
  }

  /** 运行时启停一个已注册条目。 */
  async setEnabled(name: string, enabled: boolean): Promise<void> {
    const entry = this.entries.get(name)
    if (!entry) throw new Error(`unknown plugin: ${name}`)
    if (enabled === this.fibers.has(name)) return

    if (enabled) {
      const before = new Set(this.root.rpc.names())
      const fiber = await this.root.plugin(entry, entry.config)
      this.fibers.set(name, fiber)
      this.pluginRpc.set(
        name,
        this.root.rpc.names().filter((method) => !before.has(method)),
      )
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
        client: this.clientPaths.has(name),
        rpc: this.pluginRpc.get(name) ?? [],
      })),
      panels: this.root.panels.list(),
    }
  }

  /** 浏览器启动图：只包含当前已启用且声明了 Client 面的插件。 */
  bootGraph(): BootGraph {
    const plugins: BootPlugin[] = []
    for (const id of this.clientPaths.keys()) {
      if (!this.fibers.has(id)) continue
      plugins.push({ id, clientUrl: `/plugins/${encodeURIComponent(id)}/client.js` })
    }
    return { rev: String(this.bootRev), plugins }
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
    this.clientPaths.clear()
    this.pluginRpc.clear()
    this.clientBundles.clear()
    this.url = undefined
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

    this.root.router.prefix('/plugins/', async (req, res) => {
      await this.handleClientBundle(req, res)
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

  /** `GET /plugins/<id>/client.js`：按需打包并缓存 Client 面。 */
  private async handleClientBundle(req: IncomingMessage, res: ServerResponse): Promise<void> {
    const path = new URL(req.url ?? '/', 'http://localhost').pathname
    const match = /^\/plugins\/([^/]+)\/client\.js$/.exec(path)
    if (!match) {
      this.sendJson(res, { error: 'not found' }, 404)
      return
    }

    const id = decodeURIComponent(match[1]!)
    const file = this.clientPaths.get(id)
    if (!file || !this.fibers.has(id)) {
      this.sendJson(res, { error: `unknown client plugin: ${id}` }, 404)
      return
    }

    let code = this.clientBundles.get(id)
    if (!code) {
      code = await bundleClient(file)
      this.clientBundles.set(id, code)
    }

    res.writeHead(200, {
      'content-type': 'application/javascript; charset=utf-8',
      'cache-control': 'no-store',
    })
    res.end(code)
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

  private renderPage(): string {
    return `<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>pulsar-plugin-host</title>
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

/** 从模块导出中选出插件对象：默认导出优先，其次任一具名导出。 */
function resolvePluginExport(module: Record<string, unknown>): PluginEntry | undefined {
  for (const candidate of [module.default, ...Object.values(module)]) {
    if (
      candidate
      && typeof candidate === 'object'
      && typeof (candidate as { apply?: unknown }).apply === 'function'
    ) {
      return candidate as PluginEntry
    }
  }
  return undefined
}

/** 把插件的 Client 面打成单文件 ESM。 */
async function bundleClient(file: string): Promise<string> {
  const result = await build({
    entryPoints: [file],
    bundle: true,
    write: false,
    format: 'esm',
    platform: 'browser',
    target: 'es2022',
    logLevel: 'silent',
  })
  return result.outputFiles[0]?.text ?? ''
}
