import { couplingByFile, duplicationByFile, metricNumber } from './read-metrics.ts'
import type { FileFact, MetricsFacts, MetricsReport } from './types.ts'

export type EvaluationConfig = {
  enabled: boolean
  hotspotFormula: {
    terms: { metric: string; weight: number; normalization: 'none' | 'rank' }[]
    missing: 'skip' | 'zero'
  } | null
  candidateFilters: { id: string; metric: string; operator: '>=' | '>'; value: number }[]
}

export function evaluate(
  facts: MetricsFacts,
  config: EvaluationConfig,
): MetricsReport['evaluation'] {
  if (!config.enabled || !config.hotspotFormula) {
    return { enabled: false, formula: null, score: null, matches: [] }
  }
  if (config.hotspotFormula.missing !== 'skip' && config.hotspotFormula.missing !== 'zero') {
    throw new Error(`unknown evaluation missing strategy: ${config.hotspotFormula.missing}`)
  }
  const duplication = duplicationByFile(
    [...facts.duplication, ...facts.crossClassificationDuplication],
    facts.files,
  )
  const coupling = couplingByFile(facts)
  const valueOf = (file: FileFact, metric: string) =>
    metricNumber(file, metric, duplication, coupling)
  const files = facts.files
  const rankScores = new Map<string, Map<string, number>>()
  for (const term of config.hotspotFormula.terms) {
    if (term.normalization !== 'rank') continue
    const present = files
      .flatMap((file) => {
        const value = valueOf(file, term.metric)
        return value === null ? [] : [{ path: file.path, value }]
      })
      .sort((left, right) => right.value - left.value || left.path.localeCompare(right.path))
    const scores = new Map<string, number>()
    present.forEach((item, index) => {
      scores.set(item.path, present.length === 0 ? 0 : (present.length - index) / present.length)
    })
    rankScores.set(term.metric, scores)
  }
  const matches = files.flatMap((file) => {
    const passes = config.candidateFilters.every((filter) => {
      const value = valueOf(file, filter.metric)
      return (
        value !== null && (filter.operator === '>=' ? value >= filter.value : value > filter.value)
      )
    })
    if (!passes) return []
    let weighted = 0
    let weightSum = 0
    const values: Record<string, number | null> = {}
    for (const term of config.hotspotFormula?.terms ?? []) {
      const raw = valueOf(file, term.metric)
      values[term.metric] = raw
      if (raw === null) {
        if (config.hotspotFormula?.missing === 'zero') {
          weighted += term.weight * 0
          weightSum += term.weight
        }
        continue
      }
      const normalized =
        term.normalization === 'rank' ? (rankScores.get(term.metric)?.get(file.path) ?? 0) : raw
      weighted += term.weight * normalized
      weightSum += term.weight
    }
    for (const filter of config.candidateFilters) {
      if (!(filter.metric in values)) values[filter.metric] = valueOf(file, filter.metric)
    }
    return [
      {
        ruleId: config.candidateFilters.map((filter) => filter.id).join('+') || 'formula',
        path: file.path,
        values,
        score: weightSum === 0 ? null : weighted / weightSum,
      },
    ]
  })
  matches.sort(
    (left, right) =>
      (right.score ?? -1) - (left.score ?? -1) || left.path.localeCompare(right.path),
  )
  return { enabled: true, formula: config.hotspotFormula, score: null, matches }
}
