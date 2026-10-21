import { resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { PluginHost } from './host.ts'

export { PluginHost } from './host.ts'
export type { HostOptions } from './host.ts'
export * from './types.ts'
export { loadProfile } from './profile.ts'
export { RpcService } from './rpc.ts'
export { ClientEvents } from './events.ts'

const entry = process.argv[1]
const isDirectRun = entry !== undefined && import.meta.url === pathToFileURL(entry).href

if (isDirectRun) {
  const host = new PluginHost()
  const port = process.env.PORT ? Number(process.env.PORT) : 0
  const { url } = await host.start({ port })

  const profileDir = process.env.PROFILE_DIR
    ? resolve(process.env.PROFILE_DIR)
    : fileURLToPath(new URL('../profile', import.meta.url))

  await host.loadProfile(profileDir)
  console.log(`pulsar-plugin-host listening at ${url}`)
  console.log(`profile: ${profileDir}`)
}
