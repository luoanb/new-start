import { Service } from '@deepseek-ai/cordis'
import type { Context } from '@deepseek-ai/cordis'
import type { PanelContribution } from './types.ts'

/**
 * 面板注册表：Phase 1 的 UI 扩展点占位。
 *
 * Phase 2 引入 Slots 后由 `ctx.slots` 取代；此服务只负责证明
 * "插件贡献可枚举、可随 fiber 回滚"。
 */
export class PanelService extends Service {
  private readonly panels = new Map<string, PanelContribution>()

  constructor(ctx: Context) {
    super(ctx, 'panels')
  }

  /** 注册一个面板，返回撤销函数；重复 id 立即失败。 */
  add(panel: PanelContribution): () => void {
    if (this.panels.has(panel.id)) {
      throw new Error(`panel already registered: ${panel.id}`)
    }
    this.panels.set(panel.id, panel)
    return () => {
      this.panels.delete(panel.id)
    }
  }

  /** 当前全部面板，按注册顺序返回。 */
  list(): PanelContribution[] {
    return [...this.panels.values()]
  }
}

declare module '@deepseek-ai/cordis' {
  interface Context {
    panels: PanelService
  }
}
