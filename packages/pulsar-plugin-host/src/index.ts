import { resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { DomRuntime } from '@pulsar/plugin-client/runtime/dom'
import { NodeRuntime } from '@pulsar/plugin-client/runtime/node'
import { SolidRuntime } from '@pulsar/plugin-client/runtime/solid'
import { PluginHost } from './host.ts'

export { PluginHost } from './host.ts'
export type { HostOptions } from './host.ts'
export * from './types.ts'
export { loadProfile } from './profile.ts'
export { RpcService } from './rpc.ts'
export { ClientEvents } from './events.ts'

/**
 * 组装根：唯一允许同时看到宿主与内建适配器的地方。
 *
 * `host.ts` 本身只依赖 `./runtime` 契约，不认识任何打包器；
 * 适配器由这里注入，因此换框架只改这一行。
 */
export function createHost(): PluginHost {
  const host = new PluginHost()
  host.registerRuntime(new NodeRuntime())
  host.registerRuntime(new DomRuntime())
  host.registerRuntime(new SolidRuntime())
  return host
}

const entry = process.argv[1]
const isDirectRun = entry !== undefined && import.meta.url === pathToFileURL(entry).href

if (isDirectRun) {
  const host = createHost()
  const port = process.env.PORT ? Number(process.env.PORT) : 0
  const { url } = await host.start({ port })

  const profileDir = process.env.PROFILE_DIR
    ? resolve(process.env.PROFILE_DIR)
    : fileURLToPath(new URL('../profile', import.meta.url))

  await host.loadProfile(profileDir)
  console.log(`pulsar-plugin-host listening at ${url}`)
  console.log(`profile: ${profileDir}`)
}
