import { spawnSync, type SpawnSyncReturns } from 'node:child_process'
import { createHash } from 'node:crypto'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)

export function toPosix(filePath: string): string {
  return filePath.replaceAll('\\', '/')
}

export function sha256(value: unknown): string {
  return createHash('sha256').update(JSON.stringify(value)).digest('hex')
}

export function stableSort<T>(items: readonly T[], compare: (left: T, right: T) => number): T[] {
  return [...items].sort(compare)
}

export function roundRatio(value: number): number {
  return Math.round(value * 1_000_000) / 1_000_000
}

export type Progress = (message: string) => void

export function formatProgress(startedAt: number, now: number, message: string): string {
  const elapsed = ((now - startedAt) / 1000).toFixed(1)
  return `[code-metrics] ${elapsed}s  ${message}\n`
}

export function createProgress(startedAt: number): Progress {
  return (message: string) => {
    process.stderr.write(formatProgress(startedAt, Date.now(), message))
  }
}

export function dependencyVersion(name: string): string {
  try {
    const version = (require(`${name}/package.json`) as { version?: unknown }).version
    return typeof version === 'string' ? version : 'unknown'
  } catch {
    return 'unknown'
  }
}

export function nearestRankPercentile(
  values: readonly number[],
  percentile: number,
): number | null {
  if (values.length === 0) return null
  const sorted = [...values].sort((left, right) => left - right)
  const rank = Math.max(1, Math.ceil((percentile / 100) * sorted.length))
  return sorted[rank - 1] ?? null
}

export function normalizeReportPath(repoRoot: string, filePath: string): string {
  const normalized = filePath.replaceAll('\\', '/').replace(/^\.\//, '')
  const root = repoRoot.replaceAll('\\', '/').replace(/\/$/, '')
  const prefix = `${root}/`
  return normalized.toLowerCase().startsWith(prefix.toLowerCase())
    ? normalized.slice(prefix.length)
    : normalized
}

export function runPackageBin(
  repoRoot: string,
  scriptPath: string,
  args: readonly string[],
): SpawnSyncReturns<string> {
  return spawnSync(process.execPath, [scriptPath, ...args], {
    cwd: repoRoot,
    encoding: 'utf8',
    maxBuffer: 256 * 1024 * 1024,
  })
}

export function packageScript(importMetaUrl: string, relativePath: string): string {
  return fileURLToPath(new URL(relativePath, importMetaUrl))
}

export function commandFailure(command: string, result: SpawnSyncReturns<string>): Error {
  const details = [
    result.error?.message,
    result.signal ? `signal ${result.signal}` : null,
    `exit ${result.status ?? 'null'}`,
    result.stderr?.trim(),
    result.stdout?.trim(),
  ].filter((item): item is string => Boolean(item))
  return new Error(details.join('\n').slice(0, 4000) || `${command} failed without output`)
}

export function runGit(repoRoot: string, args: readonly string[]): string {
  const result = spawnSync('git', ['-C', repoRoot, ...args], {
    encoding: 'utf8',
    maxBuffer: 256 * 1024 * 1024,
  })
  if (result.status !== 0) throw commandFailure(`git ${args.join(' ')}`, result)
  return result.stdout
}
