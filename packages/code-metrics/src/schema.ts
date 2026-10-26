import type { MetricsReport } from './types.ts'

export const metricsReportSchema = {
  $schema: 'https://json-schema.org/draft/2020-12/schema',
  $id: 'https://physalis.local/code-metrics/report.schema.json',
  title: 'MetricsReport',
  type: 'object',
    required: ['schemaVersion', 'meta', 'scope', 'derived', 'evaluation', 'diagnostics'],
  additionalProperties: true,
  properties: {
    schemaVersion: { type: 'string' },
    meta: {
      type: 'object',
      required: ['tools', 'git', 'configHash'],
      properties: {
        tools: {
          type: 'object',
          required: ['typescript', 'jscpd', 'dependencyCruiser'],
        },
        git: {
          type: 'object',
          required: ['excludeMerges', 'followRenames', 'temporalWindow', 'temporalMinCount'],
        },
      },
    },
    scope: {
      type: 'object',
      required: ['source', 'include', 'extensions', 'excludedDirectories', 'classification'],
    },
    facts: {
      type: 'object',
      required: [
        'files',
        'duplication',
        'crossClassificationDuplication',
        'duplicationStatistics',
        'graph',
      ],
    },
    derived: {
      type: 'object',
      required: [
        'aggregates',
        'rankings',
        'duplicationClusters',
        'duplicationDensity',
        'coupling',
        'temporalCoupling',
      ],
    },
    evaluation: {
      type: 'object',
      required: ['enabled', 'formula', 'score', 'matches'],
    },
    diagnostics: { type: 'array' },
  },
} as const

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function requireRecord(
  value: Record<string, unknown>,
  key: string,
  errors: string[],
  prefix = '',
): Record<string, unknown> | null {
  const candidate = value[key]
  if (!isRecord(candidate)) {
    errors.push(`${prefix}${key}`)
    return null
  }
  return candidate
}

function requireArray(
  value: Record<string, unknown>,
  key: string,
  errors: string[],
  prefix = '',
): unknown[] | null {
  const candidate = value[key]
  if (!Array.isArray(candidate)) {
    errors.push(`${prefix}${key}`)
    return null
  }
  return candidate
}

export function validateReport(report: MetricsReport): void {
  const errors: string[] = []
  const value = report as unknown
  if (!isRecord(value)) throw new Error('invalid metrics report: report')
  if (typeof value.schemaVersion !== 'string') errors.push('schemaVersion')
  const meta = requireRecord(value, 'meta', errors)
  const tools = meta ? requireRecord(meta, 'tools', errors, 'meta.') : null
  if (tools && typeof tools.typescript !== 'string') errors.push('meta.tools.typescript')
  const git = meta ? requireRecord(meta, 'git', errors, 'meta.') : null
  if (git && typeof git.excludeMerges !== 'boolean') errors.push('meta.git.excludeMerges')
  const scope = requireRecord(value, 'scope', errors)
  if (scope && !Array.isArray(scope.classification)) errors.push('scope.classification')
  if ('facts' in value) errors.push('facts')
  const derived = requireRecord(value, 'derived', errors)
  const density = derived ? requireRecord(derived, 'duplicationDensity', errors, 'derived.') : null
  if (density && typeof density.uniqueDuplicatedLines !== 'number') {
    errors.push('derived.duplicationDensity.uniqueDuplicatedLines')
  }
  if (derived && !Array.isArray(derived.temporalCoupling)) errors.push('derived.temporalCoupling')
  if (derived && !Array.isArray(derived.coupling)) errors.push('derived.coupling')
  const evaluation = requireRecord(value, 'evaluation', errors)
  const matches = evaluation ? requireArray(evaluation, 'matches', errors, 'evaluation.') : null
  matches?.forEach((match, index) => {
    if (!isRecord(match) || !('score' in match)) errors.push(`evaluation.matches[${index}].score`)
  })
  if (!Array.isArray(value.diagnostics)) errors.push('diagnostics')
  if (errors.length > 0) throw new Error(`invalid metrics report: ${errors.join(', ')}`)
}

export function reportExitCode(report: MetricsReport): number {
  return report.diagnostics.some((diagnostic) => diagnostic.severity === 'error') ? 2 : 0
}
