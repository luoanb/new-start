import type { Context } from '@deepseek-ai/cordis'
import type { PluginEntry } from '../../src/types.ts'

/**
 * 示例插件：只使用宿主提供的机制，不含任何业务能力。
 *
 * - Host 面：HTTP 路由、面板贡献、RPC 方法；
 * - Client 面：见 `client.ts`，注册一个 `main` 槽组件并调用 RPC。
 */
export const helloPlugin: PluginEntry = {
  name: 'hello-plugin',
  inject: ['router', 'panels', 'rpc', 'clientEvents'],
  apply(ctx: Context) {
    ctx.effect(() => ctx.router.get('/hello', (_req, res) => {
      res.writeHead(200, { 'content-type': 'application/json; charset=utf-8' })
      res.end(JSON.stringify({ message: 'hello from pulsar-plugin-host' }))
    }), 'hello route')

    ctx.effect(() => ctx.panels.add({
      id: 'hello',
      title: 'Hello panel',
      body: 'Contributed by hello-plugin. Disable it to remove this panel and /hello.',
    }), 'hello panel')

    ctx.effect(() => ctx.rpc.method('hello.greet', (args) => {
      const name = typeof (args as { name?: unknown } | undefined)?.name === 'string'
        ? (args as { name: string }).name
        : 'world'
      ctx.clientEvents.emit('hello/greeted', { name })
      return { message: `hello, ${name}` }
    }), 'hello rpc')
  },
}
