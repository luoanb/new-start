import { join } from 'node:path'
import type { SlotComponent, SlotView } from '../types.ts'
import { ensureArtifact, serveDir } from './artifacts.ts'
import type { Compiler } from './compiler/types.ts'
import type { Activation, PluginRuntime, PluginSource, RuntimeContext } from './types.ts'

/** 框架挂载句柄；卸载必须幂等。 */
export interface RuntimeMountHandle {
  unmount(): void
}

/**
 * 浏览器侧适配器的公共流水线：查盘、原子落盘、静态投递、路由回滚、boot graph 交付。
 *
 * 本类不含编译，也不认识任何打包器；编译由子类注入的 `Compiler` 完成。
 */
export abstract class BrowserRuntime implements PluginRuntime {
  abstract readonly name: string
  protected abstract readonly compiler: Compiler

  get imports(): Record<string, string> {
    return this.compiler.imports
  }

  get clientEntry(): string | undefined {
    return this.compiler.clientEntry
  }

  /** 渲染原语：把插件组件挂到容器，返回卸载句柄。 */
  abstract mount(container: Element, component: SlotComponent, view: SlotView): RuntimeMountHandle

  /** 子插槽渲染原语：由框架决定如何在其容器内继续渲染。 */
  abstract slot(container: Element, name: string, view: SlotView): unknown

  async activate(source: PluginSource, ctx: RuntimeContext): Promise<Activation> {
    const artifact = await ensureArtifact({
      root: ctx.artifactsDir,
      runtime: this.name,
      source,
      compiler: this.compiler,
    })

    if (artifact.kind === 'external') {
      return { client: { url: artifact.url, assets: artifact.assets } }
    }

    const base = `/rt/${this.name}/${source.id}/${artifact.hash}`
    ctx.effect(() => ctx.route(`${base}/*`, serveDir(artifact.dir, `${base}/`)))
    await this.startWatch(source, ctx, artifact.hash)

    return {
      client: {
        url: `${base}/${artifact.entry}`,
        assets: artifact.assets.map((asset) => `${base}/${asset}`),
      },
    }
  }

  /**
   * 开发期监听：编译出新 hash 后挂新路由并上报宿主。
   *
   * 只有 `Compiler.watch` 与 `ctx.notify` 同时可用时才启用；旧 hash 的目录与路由都保留。
   */
  private async startWatch(
    source: PluginSource,
    ctx: RuntimeContext,
    initialHash: string,
  ): Promise<void> {
    if (!this.compiler.watch || !ctx.notify) return

    const stop = await this.compiler.watch(
      // 监听不落盘，目录只是编译器 API 的必填项。
      { source, outDir: join(ctx.artifactsDir, this.name, source.id, '.tmp-watch') },
      () => {
        void this.rebuild(source, ctx, initialHash).catch((error) => {
          ctx.logger.error(`[${this.name}] ${source.id} 重编译失败: ${error}`)
        })
      },
    )

    ctx.effect(() => () => {
      stop()
    })
  }

  private async rebuild(
    source: PluginSource,
    ctx: RuntimeContext,
    initialHash: string,
  ): Promise<void> {
    const artifact = await ensureArtifact({
      root: ctx.artifactsDir,
      runtime: this.name,
      source,
      compiler: this.compiler,
    })
    if (artifact.kind !== 'files' || artifact.hash === initialHash) return

    const base = `/rt/${this.name}/${source.id}/${artifact.hash}`
    ctx.effect(() => ctx.route(`${base}/*`, serveDir(artifact.dir, `${base}/`)))
    ctx.notify?.({
      type: 'plugin/changed',
      payload: {
        id: source.id,
        rev: artifact.hash,
        url: `${base}/${artifact.entry}`,
        assets: artifact.assets.map((asset) => `${base}/${asset}`),
      },
    })
    ctx.logger.info(`[${this.name}] ${source.id} 重新编译为 ${artifact.hash}`)
  }
}
