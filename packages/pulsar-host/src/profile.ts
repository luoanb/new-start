import { readFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { load as parseYaml } from 'js-yaml'
import type { ClientSource, PatchEntry, ResolvedEntry } from './types.ts'

interface ProfileFile {
  /** 依次叠加的 bundle 补丁文件，路径相对于 profile 目录。 */
  bundles?: string[]
  /** profile 自己的补丁层，优先级高于全部 bundle。 */
  plugins?: PatchEntry[]
}

/** Client 面缺省使用 dom 适配器，保持旧写法零改动。 */
const DEFAULT_CLIENT_RUNTIME = 'dom'

/**
 * 读取一个 profile 并合成插件条目。
 *
 * 层序与 dsh 一致：bundles 按声明顺序应用，profile 自己的 patch 最后应用；
 * 同一 id 后出现的层整条替换前面的层，`disabled: true` 的条目被过滤。
 */
export async function loadProfile(dir: string): Promise<ResolvedEntry[]> {
  const profileFile = resolve(dir, 'profile.yml')
  const profile = await parseFile<ProfileFile>(profileFile)
  if (!profile || typeof profile !== 'object') {
    throw new Error(`invalid profile: ${profileFile}`)
  }

  const merged = new Map<string, ResolvedEntry>()

  const applyLayer = (file: string, entries: PatchEntry[]): void => {
    for (const entry of entries) {
      if (!entry || typeof entry.id !== 'string') {
        throw new Error(`invalid patch entry in ${file}`)
      }
      if (typeof entry.module !== 'string') {
        throw new Error(`patch entry "${entry.id}" is missing module in ${file}`)
      }
      const base = dirname(file)
      const client = normalizeClient(entry.client, entry.runtime)
      merged.set(entry.id, {
        ...entry,
        modulePath: resolve(base, entry.module),
        clientPath: client ? resolve(base, client.entry) : undefined,
        clientRuntime: client?.runtime,
      })
    }
  }

  for (const bundle of profile.bundles ?? []) {
    const file = resolve(dir, bundle)
    applyLayer(file, await parseFile<PatchEntry[]>(file) ?? [])
  }
  applyLayer(profileFile, profile.plugins ?? [])

  return [...merged.values()].filter((entry) => !entry.disabled)
}

/** 归一化 client 的两种书写形态；对象形态优先于条目级 runtime。 */
function normalizeClient(
  client: PatchEntry['client'],
  runtime: string | undefined,
): ClientSource | undefined {
  if (client === undefined) return undefined
  if (typeof client === 'string') {
    return { entry: client, runtime: runtime ?? DEFAULT_CLIENT_RUNTIME }
  }
  if (typeof client !== 'object' || typeof client.entry !== 'string') {
    throw new Error('invalid client entry: expected string or { entry, runtime }')
  }
  return { entry: client.entry, runtime: client.runtime ?? runtime ?? DEFAULT_CLIENT_RUNTIME }
}

async function parseFile<T>(file: string): Promise<T> {
  try {
    return parseYaml(await readFile(file, 'utf8')) as T
  } catch (error) {
    throw new Error(`failed to read ${file}: ${error instanceof Error ? error.message : String(error)}`)
  }
}
