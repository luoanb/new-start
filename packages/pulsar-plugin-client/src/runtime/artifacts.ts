import { createHash } from 'node:crypto'
import { mkdir, readFile, readdir, rename, rm, stat, writeFile } from 'node:fs/promises'
import { extname, join, relative, resolve, sep } from 'node:path'
import type { Compiler, CompileResult } from './compiler/types.ts'
import type { PluginSource, RouteHandler } from './types.ts'

/** 一次编译产出的磁盘收据，是命中判定的唯一依据。 */
export interface ArtifactManifest {
  runtime: string
  hash: string
  entry: string
  assets: string[]
  /** 源文件相对路径 -> 内容 hash；包含全部编译依赖，不只是入口。 */
  inputs: Record<string, string>
  /** 构建配置指纹，由编译器提供。 */
  buildOptionsFingerprint: string
  createdAt: string
}

/** 解析后的产物位置：可能是本地目录，也可能在外部服务上。 */
export type Artifact =
  | { kind: 'files'; hash: string; dir: string; entry: string; assets: string[] }
  | { kind: 'external'; url: string; assets: string[] }

export interface EnsureArtifactOptions {
  /** 宿主约定的产物根目录。 */
  root: string
  /** 适配器名，作为一级命名空间。 */
  runtime: string
  source: PluginSource
  compiler: Compiler
  /** 每个插件保留的版本数，默认 3。 */
  keep?: number
}

/** 产物目录：`<root>/<runtime>/<pluginId>/<hash>`；hash 缺省时返回插件目录。 */
export function resolveDir(root: string, runtime: string, pluginId: string, hash?: string): string {
  const base = join(root, segment(runtime), segment(pluginId))
  return hash ? join(base, segment(hash)) : base
}

/** 读取某个版本的 manifest；不存在或损坏时返回 undefined。 */
export async function readManifest(dir: string): Promise<ArtifactManifest | undefined> {
  return readJson<ArtifactManifest>(join(dir, 'manifest.json'))
}

/** 读取插件目录下的最新版本指针；没有编译记录时返回 undefined。 */
export async function readLatest(pluginDir: string): Promise<ArtifactManifest | undefined> {
  return readJson<ArtifactManifest>(join(pluginDir, 'latest.json'))
}

/**
 * 原子发布：把临时目录 rename 成目标目录。
 *
 * 目标已存在（并发编译同一 hash）时丢弃本次结果，保留先到者。
 */
export async function writeAtomic(tmpDir: string, targetDir: string): Promise<void> {
  try {
    await rename(tmpDir, targetDir)
  } catch (error) {
    if (await readManifest(targetDir)) {
      await rm(tmpDir, { recursive: true, force: true })
      return
    }
    await rm(tmpDir, { recursive: true, force: true })
    throw error
  }
}

/**
 * 保证产物存在：读最新指针 → 校验全部依赖 → 命中即返回；否则编译到临时目录，
 * 由编译结果算出内容 hash，再原子发布并更新指针。
 *
 * 不认识任何打包器；编译完全交给传入的 `Compiler`。
 */
export async function ensureArtifact(options: EnsureArtifactOptions): Promise<Artifact> {
  const { root, runtime, source, compiler, keep = 3 } = options
  const pluginDir = resolveDir(root, runtime, source.id)
  const buildOptionsFingerprint = await compiler.fingerprint(source)

  const latest = await readLatest(pluginDir)
  if (latest && await isFresh(latest, buildOptionsFingerprint)) {
    const dir = join(pluginDir, latest.hash)
    if (await readManifest(dir)) {
      return { kind: 'files', hash: latest.hash, dir, entry: latest.entry, assets: latest.assets }
    }
  }

  const tmpDir = join(pluginDir, `.tmp-${process.pid}-${Date.now()}`)
  await mkdir(tmpDir, { recursive: true })

  let result: CompileResult
  try {
    result = await compiler.compile({ source, outDir: tmpDir })
  } catch (error) {
    await rm(tmpDir, { recursive: true, force: true })
    throw error
  }

  if (result.kind === 'external') {
    await rm(tmpDir, { recursive: true, force: true })
    return { kind: 'external', url: result.url, assets: result.assets ?? [] }
  }

  const inputs = result.inputs ?? await hashEntry(source)
  const hash = createHash('sha256')
    .update(`${compiler.name}\0${buildOptionsFingerprint}\0${JSON.stringify(sorted(inputs))}`)
    .digest('hex')
    .slice(0, 10)
  const dir = join(pluginDir, hash)

  const manifest: ArtifactManifest = {
    runtime,
    hash,
    entry: result.entry,
    assets: result.assets,
    inputs,
    buildOptionsFingerprint,
    createdAt: new Date().toISOString(),
  }

  if (await readManifest(dir)) {
    // 同内容产物已存在：丢弃本次编译结果。
    await rm(tmpDir, { recursive: true, force: true })
  } else {
    await writeFile(join(tmpDir, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`)
    await writeAtomic(tmpDir, dir)
  }

  await writeFile(join(pluginDir, 'latest.json'), `${JSON.stringify(manifest, null, 2)}\n`)
  await prune(pluginDir, keep, hash)

  return { kind: 'files', hash, dir, entry: result.entry, assets: result.assets }
}

/**
 * 保留最近 `keep` 个版本，跳过当前正在服务的 hash。
 *
 * 只在新产物成功发布后调用；卸载路径不调用，因此「卸载不删产物」仍成立。
 */
export async function prune(pluginDir: string, keep = 3, keepHash?: string): Promise<void> {
  let entries
  try {
    entries = await readdir(pluginDir, { withFileTypes: true })
  } catch {
    return
  }
  const versions = await Promise.all(
    entries
      .filter((entry) => entry.isDirectory() && !entry.name.startsWith('.tmp-'))
      .map(async (entry) => ({
        name: entry.name,
        mtime: (await stat(join(pluginDir, entry.name))).mtimeMs,
      })),
  )
  versions.sort((a, b) => b.mtime - a.mtime)
  const stale = versions.slice(keep).filter((version) => version.name !== keepHash)
  await Promise.all(
    stale.map((version) => rm(join(pluginDir, version.name), { recursive: true, force: true })),
  )
}

/** 静态投递：`<basePath>/<file>` 直接映射到 `<dir>/<file>`，带强缓存。 */
export function serveDir(dir: string, basePath: string): RouteHandler {
  const root = resolve(dir)
  return async (req, res) => {
    const pathname = new URL(req.url ?? '/', 'http://localhost').pathname
    if (!pathname.startsWith(basePath)) return notFound(res)

    const relativePath = decodeURIComponent(pathname.slice(basePath.length)).replace(/^\/+/, '')
    if (!relativePath) return notFound(res)

    const file = resolve(root, relativePath)
    if (file !== root && !file.startsWith(`${root}${sep}`)) return forbidden(res)

    try {
      const content = await readFile(file)
      res.writeHead(200, {
        'content-type': contentType(file),
        'cache-control': 'public, max-age=31536000, immutable',
      })
      res.end(content)
    } catch {
      notFound(res)
    }
  }
}

/** manifest 里的每个源文件是否仍与磁盘一致。 */
async function isFresh(
  manifest: ArtifactManifest,
  buildOptionsFingerprint: string,
): Promise<boolean> {
  if (manifest.buildOptionsFingerprint !== buildOptionsFingerprint) return false
  const entries = Object.entries(manifest.inputs ?? {})
  if (entries.length === 0) return false
  for (const [file, hash] of entries) {
    try {
      const content = await readFile(resolve(file))
      if (createHash('sha256').update(content).digest('hex') !== hash) return false
    } catch {
      return false
    }
  }
  return true
}

/** 编译器未返回依赖图时的兜底：只记录入口。 */
async function hashEntry(source: PluginSource): Promise<Record<string, string>> {
  const content = await readFile(source.entry)
  return {
    [relative(process.cwd(), source.entry).split(sep).join('/')]:
      createHash('sha256').update(content).digest('hex'),
  }
}

function sorted(inputs: Record<string, string>): Record<string, string> {
  return Object.fromEntries(Object.entries(inputs).sort(([a], [b]) => a.localeCompare(b)))
}

async function readJson<T>(file: string): Promise<T | undefined> {
  try {
    return JSON.parse(await readFile(file, 'utf8')) as T
  } catch {
    return undefined
  }
}

/** 产物路径片段只允许安全字符，避免 id 逃出命名空间。 */
function segment(value: string): string {
  if (!/^[A-Za-z0-9._-]+$/.test(value) || value === '.' || value === '..') {
    throw new Error(`invalid artifact path segment: ${value}`)
  }
  return value
}

function contentType(file: string): string {
  switch (extname(file).toLowerCase()) {
    case '.js':
    case '.mjs':
      return 'text/javascript; charset=utf-8'
    case '.css':
      return 'text/css; charset=utf-8'
    case '.json':
    case '.map':
      return 'application/json; charset=utf-8'
    case '.html':
      return 'text/html; charset=utf-8'
    case '.svg':
      return 'image/svg+xml'
    case '.png':
      return 'image/png'
    case '.jpg':
    case '.jpeg':
      return 'image/jpeg'
    case '.webp':
      return 'image/webp'
    case '.woff2':
      return 'font/woff2'
    case '.wasm':
      return 'application/wasm'
    default:
      return 'application/octet-stream'
  }
}

function notFound(res: Parameters<RouteHandler>[1]): void {
  res.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' })
  res.end('not found')
}

function forbidden(res: Parameters<RouteHandler>[1]): void {
  res.writeHead(403, { 'content-type': 'text/plain; charset=utf-8' })
  res.end('forbidden')
}
