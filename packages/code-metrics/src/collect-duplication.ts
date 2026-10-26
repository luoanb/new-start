import { existsSync, mkdtempSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'

import type { DuplicationMatch, DuplicationStatistics, FileFact } from './types.ts'
import {
  commandFailure,
  normalizeReportPath,
  packageScript,
  type Progress,
  runPackageBin,
} from './util.ts'

export type DuplicationConfig = {
  minLines: number
  minTokens: number
  mode: string
}

const MAX_LINES = '100000'
const MAX_SIZE = '20mb'

type JscpdClone = {
  lines: number
  firstFile: { name: string; start: number; end: number }
  secondFile: { name: string; start: number; end: number }
}

type JscpdReport = {
  duplicates?: JscpdClone[]
  statistics?: {
    total?: {
      duplicatedLines?: number
      duplicatedTokens?: number
      percentage?: number
      percentageTokens?: number
    }
  }
}

export function uniqueCoveredLines(intervals: readonly { start: number; end: number }[]): number {
  const sorted = intervals
    .filter((interval) => interval.end >= interval.start)
    .sort((left, right) => left.start - right.start || left.end - right.end)
  let total = 0
  let current: { start: number; end: number } | null = null
  for (const interval of sorted) {
    if (!current || interval.start > current.end + 1) {
      if (current) total += current.end - current.start + 1
      current = { start: interval.start, end: interval.end }
    } else {
      current.end = Math.max(current.end, interval.end)
    }
  }
  if (current) total += current.end - current.start + 1
  return total
}

export function directoryRoots(files: readonly { path: string }[]): string[] {
  return [...new Set(files.map((file) => file.path.split('/')[0]).filter(Boolean))].sort()
}

export function splitDuplication(
  matches: readonly DuplicationMatch[],
  files: readonly FileFact[],
): {
  sameClassification: DuplicationMatch[]
  crossClassification: DuplicationMatch[]
} {
  const byPath = new Map(
    files
      .filter((file) => file.size.readable && !file.size.binary)
      .map((file) => [file.path, file]),
  )
  const sameClassification: DuplicationMatch[] = []
  const crossClassification: DuplicationMatch[] = []
  for (const match of matches) {
    const first = byPath.get(match.firstFile)
    const second = byPath.get(match.secondFile)
    if (!first || !second) continue
    if (first.classification.primary === second.classification.primary)
      sameClassification.push(match)
    else crossClassification.push(match)
  }
  return { sameClassification, crossClassification }
}

export function collectDuplication(
  repoRoot: string,
  files: readonly FileFact[],
  config: DuplicationConfig,
  onProgress?: Progress,
): {
  sameClassification: DuplicationMatch[]
  crossClassification: DuplicationMatch[]
  statistics: DuplicationStatistics | null
  parameters: string
} {
  const roots = directoryRoots(files).filter((root) => existsSync(path.join(repoRoot, root)))
  const parameters = `dirs=${roots.join(',')} minLines=${config.minLines} minTokens=${config.minTokens} mode=${config.mode} maxLines=${MAX_LINES} maxSize=${MAX_SIZE}`
  onProgress?.(`重复检测 开始 ${roots.length} 个目录`)
  if (roots.length === 0) {
    return { sameClassification: [], crossClassification: [], statistics: null, parameters }
  }
  const outputDir = mkdtempSync(path.join(tmpdir(), 'code-metrics-jscpd-'))
  const result = runPackageBin(
    repoRoot,
    packageScript(import.meta.url, '../node_modules/jscpd/bin/jscpd'),
    [
      ...roots,
      '--silent',
      '--reporters',
      'json',
      '--output',
      outputDir,
      '--min-lines',
      String(config.minLines),
      '--min-tokens',
      String(config.minTokens),
      '--mode',
      config.mode,
      '--max-lines',
      MAX_LINES,
      '--max-size',
      MAX_SIZE,
      '--gitignore',
      '--pattern',
      '**/*.{ts,svelte,js,mjs,cjs,css,html,json,md,yml,yaml,svg}',
      '--ignore',
      '**/node_modules/**',
      '--ignore',
      '**/dist/**',
      '--ignore',
      '**/.svelte-kit/**',
      '--ignore',
      '**/coverage/**',
    ],
  )
  const reportPath = path.join(outputDir, 'jscpd-report.json')
  if (!existsSync(reportPath)) throw commandFailure('jscpd', result)
  const report = JSON.parse(readFileSync(reportPath, 'utf8')) as JscpdReport
  const total = report.statistics?.total
  const statistics: DuplicationStatistics = {
    duplicatedLines: total?.duplicatedLines ?? null,
    duplicatedTokens: total?.duplicatedTokens ?? null,
    percentage: total?.percentage ?? null,
    percentageTokens: total?.percentageTokens ?? null,
  }
  const matches = (report.duplicates ?? []).map((duplicate) => ({
    firstFile: normalizeReportPath(repoRoot, duplicate.firstFile.name),
    firstStart: duplicate.firstFile.start,
    firstEnd: duplicate.firstFile.end,
    secondFile: normalizeReportPath(repoRoot, duplicate.secondFile.name),
    secondStart: duplicate.secondFile.start,
    secondEnd: duplicate.secondFile.end,
    lines: duplicate.lines,
  }))
  const split = splitDuplication(matches, files)
  onProgress?.('重复检测 完成')
  return { ...split, statistics, parameters }
}
