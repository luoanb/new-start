import { pathToFileURL } from 'node:url'
import type { Activation, PluginObject, PluginRuntime, PluginSource, RuntimeContext } from './types.ts'

/**
 * 服务端适配器：退化实现——不编译、不落盘、不投递，执行即 `import()`。
 *
 * 宿主只消费返回的 `server`，与浏览器适配器走同一条 `activate()` 路径。
 */
export class NodeRuntime implements PluginRuntime {
  readonly name = 'node'

  async activate(source: PluginSource, _ctx: RuntimeContext): Promise<Activation> {
    const module = await import(pathToFileURL(source.entry).href) as Record<string, unknown>
    const plugin = resolvePluginExport(module)
    if (!plugin) {
      throw new Error(`no plugin export found in ${source.entry}`)
    }
    return { server: plugin }
  }
}

/** 从模块导出中选出插件对象：默认导出优先，其次任一具名导出。 */
export function resolvePluginExport(module: Record<string, unknown>): PluginObject | undefined {
  for (const candidate of [module.default, ...Object.values(module)]) {
    if (
      candidate
      && typeof candidate === 'object'
      && typeof (candidate as { apply?: unknown }).apply === 'function'
    ) {
      return candidate as PluginObject
    }
  }
  return undefined
}
