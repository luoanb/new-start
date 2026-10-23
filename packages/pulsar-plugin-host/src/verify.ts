import { mkdtemp, rm, stat } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { DomRuntime } from '@pulsar/plugin-client/runtime/dom'
import { NodeRuntime } from '@pulsar/plugin-client/runtime/node'
import { SolidRuntime } from '@pulsar/plugin-client/runtime/solid'
import { PluginHost } from './host.ts'
import type { BootGraph, HostState } from './types.ts'

const profileDir = fileURLToPath(new URL('../profile', import.meta.url))
const artifactsDir = await mkdtemp(join(tmpdir(), 'pulsar-verify-'))
const host = new PluginHost()
host.registerRuntime(new NodeRuntime())
host.registerRuntime(new DomRuntime())
host.registerRuntime(new SolidRuntime())

let url = (await host.start({ port: 0, artifactsDir, dev: false })).url
let clientUrl = ''

try {
  await host.loadProfile(profileDir)

  const loaded = await state(url)
  const hello = loaded.plugins.find((plugin) => plugin.name === 'hello')
  assert(hello?.enabled, '插件未处于 enabled')
  assert(hello?.client, '插件未声明 Client 面')
  assert(hello?.rpc.includes('hello.greet'), '插件未注册 RPC')
  assert(loaded.panels.some((panel) => panel.id === 'hello'), '插件未贡献面板')

  const boot = await json<BootGraph>(`${url}/api/boot`)
  const entry = boot.plugins.find((plugin) => plugin.id === 'hello')
  assert(entry, 'boot graph 未包含 Client 面')
  clientUrl = entry.client.url
  assert(
    /^\/rt\/dom\/hello\/[0-9a-f]{10}\/client\.js$/.test(clientUrl),
    `Client 产物 URL 形态不正确: ${clientUrl}`,
  )
  assert(Array.isArray(entry.client.assets), 'client.assets 应是数组')
  assert(boot.runtimes.dom === '/client/runtime.js', 'boot graph 未声明 dom runtime 入口')
  assert(boot.imports && typeof boot.imports === 'object', 'boot graph 缺少 imports')

  // 官方框架适配器：同一页面里 dom 与 solid 共存
  const solid = boot.plugins.find((plugin) => plugin.id === 'solid')
  assert(solid, 'boot graph 未包含 Solid 插件')
  assert(
    /^\/rt\/solid\/solid\/[0-9a-f]{10}\/client\.js$/.test(solid.client.url),
    `Solid 产物 URL 形态不正确: ${solid.client.url}`,
  )
  assert(boot.runtimes.solid === '/client/runtime.js', 'boot graph 未声明 solid runtime 入口')
  assert(boot.imports['solid-js'] === '/rt/vendor/solid/solid.js', 'import map 未声明 solid-js')
  assert(boot.imports['solid-js/web'] === '/rt/vendor/solid/web.js', 'import map 未声明 solid-js/web')

  const solidBundle = await fetch(`${url}${solid.client.url}`)
  assert(solidBundle.ok, 'Solid 产物不可用')
  const solidCode = await solidBundle.text()
  assert(solidCode.includes('solid-panel'), 'Solid 产物缺少插槽注册')
  assert(
    solidCode.includes('from "solid-js"') || solidCode.includes("from 'solid-js'"),
    'Solid 产物应保留 solid-js 为外部依赖，否则会出现多份实例',
  )

  const vendor = await fetch(`${url}/rt/vendor/solid/solid.js`)
  assert(vendor.ok, 'Solid 共享运行时不可取')
  assert((await fetch(`${url}/rt/vendor/solid/../package.json`)).status === 404, 'vendor 只允许白名单文件')

  const bundle = await fetch(`${url}${clientUrl}`)
  assert(bundle.ok, 'Client 产物不可用')
  assert(bundle.headers.get('cache-control')?.includes('immutable'), '产物应带强缓存')
  assert((await bundle.text()).includes('Hello plugin'), 'Client 产物内容缺失')

  const page = await (await fetch(`${url}/`)).text()
  assert(count(page, 'type="importmap"') === 1, '页面必须恰好一张 import map')
  assert(
    page.indexOf('type="importmap"') < page.indexOf('type="module"'),
    'import map 必须先于首个 module script',
  )

  const greeted = await rpc(url, 'hello.greet', { name: 'verify' })
  assert(greeted.status === 200, 'RPC 调用失败')
  assert(greeted.body.result?.message === 'hello, verify', 'RPC 返回值不正确')

  const productDir = join(artifactsDir, 'dom', 'hello', clientUrl.split('/')[4]!)
  await stat(productDir)
  await stat(join(productDir, 'manifest.json'))

  await host.setEnabled('hello', false)
  const disabled = await state(url)
  assert(disabled.plugins.find((plugin) => plugin.name === 'hello')?.enabled === false, '禁用状态未记录')
  assert(!disabled.panels.some((panel) => panel.id === 'hello'), '禁用后面板仍有残留')
  assert(
    !(await json<BootGraph>(`${url}/api/boot`)).plugins.some((plugin) => plugin.id === 'hello'),
    '禁用后 boot graph 仍包含该插件',
  )
  assert((await fetch(`${url}${clientUrl}`)).status === 404, '禁用后产物路由应回滚')
  assert((await rpc(url, 'hello.greet', {})).status === 500, '禁用后 RPC 仍可调用')
  await stat(productDir)

  await host.setEnabled('hello', true)
  const enabledBoot = await json<BootGraph>(`${url}/api/boot`)
  const reEnabled = enabledBoot.plugins.find((plugin) => plugin.id === 'hello')
  assert(reEnabled, '重新启用后 boot graph 未恢复')
  assert(reEnabled.client.url === clientUrl, '重新启用应命中同一产物，不应重新编译')
  const again = await rpc(url, 'hello.greet', { name: 'again' })
  assert(again.body.result?.message === 'hello, again', '重新启用后 RPC 未恢复')

  console.log('PASS 装配 / Client 产物 / import map / RPC / 禁用回滚 / 重新启用命中')
} finally {
  await host.stop()
}

// 重启验证：同一 artifactsDir 下应命中已落盘产物。
url = (await host.start({ port: 0, artifactsDir, dev: false })).url
try {
  await host.loadProfile(profileDir)
  const restarted = await rpc(url, 'hello.greet', { name: 'restart' })
  assert(restarted.body.result?.message === 'hello, restart', '重启后 RPC 不可用')
  const boot = await json<BootGraph>(`${url}/api/boot`)
  const entry = boot.plugins.find((plugin) => plugin.id === 'hello')
  assert(entry?.client.url === clientUrl, '重启后应命中同一产物目录')
  console.log('PASS 停止后重新启动并命中产物')
} finally {
  await host.stop()
  await rm(artifactsDir, { recursive: true, force: true })
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

/** 统计子串出现次数，用于断言 import map 只有一张。 */
function count(source: string, needle: string): number {
  return source.split(needle).length - 1
}

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`FAIL ${message}`)
}
