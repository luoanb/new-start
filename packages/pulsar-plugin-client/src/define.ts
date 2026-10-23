import type { ClientPlugin } from './types.ts'

/**
 * 声明一个浏览器侧插件。当前只做类型收窄与形态校验；
 * 插件级 dispose 在生命周期步骤（P1）接入。
 */
export function defineClientPlugin(plugin: ClientPlugin): ClientPlugin {
  if (typeof plugin !== 'function') {
    throw new TypeError('client plugin must be a function')
  }
  return plugin
}
