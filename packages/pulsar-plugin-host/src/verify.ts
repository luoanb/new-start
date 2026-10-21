import { fileURLToPath } from 'node:url'
import { PluginHost } from './host.ts'
import type { BootGraph, HostState } from './types.ts'

const profileDir = fileURLToPath(new URL('../profile', import.meta.url))
const host = new PluginHost()

let url = (await host.start({ port: 0 })).url

try {
  await host.loadProfile(profileDir)

  const loaded = await state(url)
  const hello = loaded.plugins.find((plugin) => plugin.name === 'hello')
  assert(hello?.enabled, '插件未处于 enabled')
  assert(hello?.client, '插件未声明 Client 面')
  assert(hello?.rpc.includes('hello.greet'), '插件未注册 RPC')
  assert(loaded.panels.some((panel) => panel.id === 'hello'), '插件未贡献面板')

  const boot = await json<BootGraph>(`${url}/api/boot`)
  assert(boot.plugins.some((plugin) => plugin.id === 'hello'), 'boot graph 未包含 Client 面')

  const bundle = await fetch(`${url}/plugins/hello/client.js`)
  assert(bundle.ok, 'Client bundle 不可用')
  assert((await bundle.text()).includes('Hello plugin'), 'Client bundle 内容缺失')

  const greeted = await rpc(url, 'hello.greet', { name: 'verify' })
  assert(greeted.status === 200, 'RPC 调用失败')
  assert(greeted.body.result?.message === 'hello, verify', 'RPC 返回值不正确')

  await host.setEnabled('hello', false)
  const disabled = await state(url)
  assert(disabled.plugins.find((plugin) => plugin.name === 'hello')?.enabled === false, '禁用状态未记录')
  assert(!disabled.panels.some((panel) => panel.id === 'hello'), '禁用后面板仍有残留')
  assert((await json<BootGraph>(`${url}/api/boot`)).plugins.length === 0, '禁用后 boot graph 仍有条目')
  assert((await fetch(`${url}/plugins/hello/client.js`)).status === 404, '禁用后 Client bundle 仍可访问')
  assert((await rpc(url, 'hello.greet', {})).status === 500, '禁用后 RPC 仍可调用')

  await host.setEnabled('hello', true)
  assert((await json<BootGraph>(`${url}/api/boot`)).plugins.some((plugin) => plugin.id === 'hello'), '重新启用后 boot graph 未恢复')
  const again = await rpc(url, 'hello.greet', { name: 'again' })
  assert(again.body.result?.message === 'hello, again', '重新启用后 RPC 未恢复')

  console.log('PASS 装配 / Client 装载 / RPC / 禁用卸载 / 重新启用')
} finally {
  await host.stop()
}

// 重启验证：stop 之后重新 start 并重新装配 profile。
url = (await host.start({ port: 0 })).url
try {
  await host.loadProfile(profileDir)
  const restarted = await rpc(url, 'hello.greet', { name: 'restart' })
  assert(restarted.body.result?.message === 'hello, restart', '重启后 RPC 不可用')
  assert((await json<BootGraph>(`${url}/api/boot`)).plugins.some((plugin) => plugin.id === 'hello'), '重启后 boot graph 为空')
  console.log('PASS 停止后重新启动并重新装配')
} finally {
  await host.stop()
}

async function state(base: string): Promise<HostState> {
  return await json<HostState>(`${base}/api/state`)
}

async function json<T>(target: string): Promise<T> {
  const response = await fetch(target)
  assert(response.ok, `GET ${target} 失败`)
  return await response.json() as T
}

async function rpc(base: string, method: string, args: unknown): Promise<{
  status: number
  body: { result?: { message?: string }; error?: string }
}> {
  const response = await fetch(`${base}/api/rpc`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ method, args }),
  })
  return { status: response.status, body: await response.json() }
}

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`FAIL ${message}`)
}
