import { couplingByFile, duplicationByFile, metricNumber } from './read-metrics.ts'
import type { MetricsFacts, Ranking } from './types.ts'

const METRICS = [
  'totalLines',
  'nonEmptyLines',
  'bytes',
  'complexityMax',
  'complexitySum',
  'cognitiveMax',
  'duplicatedLines',
  'uniqueDuplicatedLines',
  'duplicatedLineDensity',
  'gitCommits90d',
  'gitChurn90d',
  'instability',
] as const

export function rank(facts: MetricsFacts): Ranking[] {
  const duplication = duplicationByFile(
    [...facts.duplication, ...facts.crossClassificationDuplication],
    facts.files,
  )
  const coupling = couplingByFile(facts)
  return METRICS.flatMap((metric) => {
    const values = facts.files
      .map((file) => ({ path: file.path, value: metricNumber(file, metric, duplication, coupling) }))
      .filter((item): item is { path: string; value: number } => item.value !== null)
      .sort((left, right) => right.value - left.value || left.path.localeCompare(right.path))
    return values.map((item, index) => ({
      metric,
      path: item.path,
      rank: index + 1,
      percentile: ((values.length - index) / values.length) * 100,
    }))
  })
}
