import { instability } from './collect-dependencies.ts'
import { uniqueCoveredLines } from './collect-duplication.ts'
import type { DuplicationDensity, DuplicationMatch, FileFact, MetricsFacts } from './types.ts'
import { roundRatio } from './util.ts'

export type FileDuplication = {
  duplicatedLines: number
  uniqueDuplicatedLines: number
  duplicatedLineDensity: number | null
}

export type FileCoupling = {
  incoming: number
  outgoing: number
  afferent: number
  efferent: number
  instability: number | null
}

export function cyclomaticOf(file: FileFact): { max: number; sum: number } | null {
  if (!file.complexity || file.complexity.status === 'failed') return null
  const values = file.complexity.functions.map((fn) => fn.cyclomatic)
  return {
    max: values.length === 0 ? 0 : Math.max(...values),
    sum: values.reduce((sum, value) => sum + value, 0),
  }
}

export function cognitiveOfFile(file: FileFact): { max: number; sum: number } | null {
  if (!file.complexity || file.complexity.status === 'failed') return null
  const values = [
    file.complexity.moduleCognitive,
    ...file.complexity.functions.map((fn) => fn.cognitive),
  ]
  return {
    max: Math.max(...values),
    sum: values.reduce((sum, value) => sum + value, 0),
  }
}

export function duplicationByFile(
  matches: readonly DuplicationMatch[],
  files: readonly FileFact[],
): Map<string, FileDuplication> {
  const intervals = new Map<string, { start: number; end: number }[]>()
  const duplicatedLines = new Map<string, number>()
  for (const match of matches) {
    const sides = [
      { file: match.firstFile, start: match.firstStart, end: match.firstEnd },
      { file: match.secondFile, start: match.secondStart, end: match.secondEnd },
    ]
    const counted = new Set<string>()
    for (const side of sides) {
      intervals.set(side.file, [...(intervals.get(side.file) ?? []), side])
      if (counted.has(side.file)) continue
      counted.add(side.file)
      duplicatedLines.set(side.file, (duplicatedLines.get(side.file) ?? 0) + match.lines)
    }
  }
  return new Map(
    files.map((file) => {
      const unique = uniqueCoveredLines(intervals.get(file.path) ?? [])
      const lines = file.size.totalLines
      return [
        file.path,
        {
          duplicatedLines: duplicatedLines.get(file.path) ?? 0,
          uniqueDuplicatedLines: unique,
          duplicatedLineDensity:
            lines !== null && lines > 0 ? roundRatio((unique / lines) * 100) : null,
        },
      ]
    }),
  )
}

export function couplingByFile(
  facts: Pick<MetricsFacts, 'files' | 'graph'>,
): Map<string, FileCoupling> {
  const incomingEdges = new Map<string, number>()
  const outgoingEdges = new Map<string, number>()
  const afferent = new Map<string, Set<string>>()
  const efferent = new Map<string, Set<string>>()
  for (const edge of facts.graph.edges) {
    outgoingEdges.set(edge.from, (outgoingEdges.get(edge.from) ?? 0) + 1)
    incomingEdges.set(edge.to, (incomingEdges.get(edge.to) ?? 0) + 1)
    const outgoing = efferent.get(edge.from) ?? new Set<string>()
    outgoing.add(edge.to)
    efferent.set(edge.from, outgoing)
    const incoming = afferent.get(edge.to) ?? new Set<string>()
    incoming.add(edge.from)
    afferent.set(edge.to, incoming)
  }
  return new Map(
    facts.files.map((file) => {
      const incoming = afferent.get(file.path)?.size ?? 0
      const outgoing = efferent.get(file.path)?.size ?? 0
      return [
        file.path,
        {
          incoming: incomingEdges.get(file.path) ?? 0,
          outgoing: outgoingEdges.get(file.path) ?? 0,
          afferent: incoming,
          efferent: outgoing,
          instability: instability(incoming, outgoing),
        },
      ]
    }),
  )
}

export function metricNumber(
  file: FileFact,
  metric: string,
  duplication: ReadonlyMap<string, FileDuplication>,
  coupling: ReadonlyMap<string, FileCoupling>,
): number | null {
  const duplicate = duplication.get(file.path)
  const linked = coupling.get(file.path)
  const values: Record<string, number | null> = {
    totalLines: file.size.totalLines,
    nonEmptyLines: file.size.nonEmptyLines,
    bytes: file.size.bytes,
    complexityMax: cyclomaticOf(file)?.max ?? null,
    complexitySum: cyclomaticOf(file)?.sum ?? null,
    cognitiveMax: cognitiveOfFile(file)?.max ?? null,
    duplicatedLines: duplicate?.duplicatedLines ?? 0,
    uniqueDuplicatedLines: duplicate?.uniqueDuplicatedLines ?? 0,
    duplicatedLineDensity: duplicate?.duplicatedLineDensity ?? null,
    gitCommits90d: file.git['90d']?.commits ?? null,
    gitChurn90d: file.git['90d']?.churn ?? null,
    instability: linked?.instability ?? null,
  }
  return values[metric] ?? null
}

export function summarizeDuplicationDensity(
  files: readonly FileFact[],
  matches: readonly DuplicationMatch[],
): DuplicationDensity {
  const byFile = duplicationByFile(matches, files)
  let uniqueDuplicatedLines = 0
  let analyzableFiles = 0
  let analyzableLines = 0
  for (const file of files) {
    uniqueDuplicatedLines += byFile.get(file.path)?.uniqueDuplicatedLines ?? 0
    if (file.size.totalLines !== null && file.size.readable && !file.size.binary) {
      analyzableFiles += 1
      analyzableLines += file.size.totalLines
    }
  }
  return {
    uniqueDuplicatedLines,
    analyzableFiles,
    analyzableLines,
    density:
      analyzableLines === 0 ? null : roundRatio((uniqueDuplicatedLines / analyzableLines) * 100),
  }
}
