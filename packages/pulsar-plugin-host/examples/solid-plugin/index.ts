import type { Context } from '@deepseek-ai/cordis'
import type { PluginEntry } from '../../src/types.ts'

/** 示例插件：后端无贡献，只演示 Solid 前端的接入。 */
export const solidPlugin: PluginEntry = {
  name: 'solid-plugin',
  apply(_ctx: Context) {},
}
