import { readFile } from 'node:fs/promises'
import type { IncomingMessage, ServerResponse } from 'node:http'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'
import type { SlotComponent, SlotView } from '../types.ts'
import { BrowserRuntime, type RuntimeMountHandle } from './browser.ts'
import { esbuildCompiler } from './compiler/esbuild.ts'
import type { Compiler } from './compiler/types.ts'
import { mountNodes } from './mount.ts'
import type { Activation, PluginSource, RuntimeContext } from './types.ts'

const require = createRequire(import.meta.url)
const SOLID_ROOT = dirname(require.resolve('solid-js/package.json'))

/** 适配器自己投递的 Solid ESM；插件不各自携带一份，保证页面里只有一份实例。 */
const VENDOR_PREFIX = '/rt/vendor/solid'
const VENDOR_FILES: Record<string, string> = {
  'solid.js': join(SOLID_ROOT, 'dist/solid.js'),
  'web.js': join(SOLID_ROOT, 'web/dist/web.js'),
  'store.js': join(SOLID_ROOT, 'store/dist/store.js'),
}

const IMPORTS: Record<string, string> = {
  'solid-js': `${VENDOR_PREFIX}/solid.js`,
  'solid-js/web': `${VENDOR_PREFIX}/web.js`,
  'solid-js/store': `${VENDOR_PREFIX}/store.js`,
}

/**
 * Solid 适配器：官方一等公民实现。
 *
 * 与 `dom` 的差别只有三处——`Compiler` 多一层 `babel-preset-solid`、
 * `imports` 声明三条共享依赖、`mount/slot` 换成 Solid 的节点挂载。
 */
export class SolidRuntime extends BrowserRuntime {
  readonly name = 'solid'

  protected readonly compiler: Compiler = esbuildCompiler({
    // esbuild 的 JSX 对 Solid 只是近似，真正的细粒度优化在 babel-preset-solid。
    plugins: () => [solidPlugin()],
    external: Object.keys(IMPORTS),
    imports: IMPORTS,
    clientEntry: '/client/runtime.js',
  })

  async activate(source: PluginSource, ctx: RuntimeContext): Promise<Activation> {
    ctx.effect(() => ctx.route(`${VENDOR_PREFIX}/*`, serveVendor()))
    return await super.activate(source, ctx)
  }

  mount(container: Element, component: SlotComponent, view: SlotView): RuntimeMountHandle {
    return mountNodes(container, component, view)
  }

  slot(container: Element, name: string, view: SlotView): void {
    view.renderSlot(name, container)
  }
}

/** 延迟加载编译插件：babel 链只在真的编译 Solid 插件时付出成本。 */
function solidPlugin() {
  const loaded = require('esbuild-plugin-solid') as typeof import('esbuild-plugin-solid')
  return loaded.solidPlugin()
}

/** 投递 Solid 运行时的三个 ESM 文件；白名单之外一律 404。 */
function serveVendor() {
  return async (req: IncomingMessage, res: ServerResponse) => {
    const pathname = new URL(req.url ?? '/', 'http://localhost').pathname
    const file = pathname.slice(VENDOR_PREFIX.length).replace(/^\/+/, '')
    const target = VENDOR_FILES[file]
    if (!target) {
      res.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' })
      res.end('not found')
      return
    }
    const content = await readFile(target)
    res.writeHead(200, {
      'content-type': 'text/javascript; charset=utf-8',
      'cache-control': 'public, max-age=31536000, immutable',
    })
    res.end(content)
  }
}
