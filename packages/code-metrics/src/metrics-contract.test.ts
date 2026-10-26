import { describe, expect, it } from 'vitest'

import { measureSource } from './collect-complexity.ts'
import { dependencyComponents, instability, workspaceCoupling } from './collect-dependencies.ts'
import { uniqueCoveredLines } from './collect-duplication.ts'
import {
  canonicalIdentity,
  coverageDays,
  gitLogArgs,
  normalizeNumstatPath,
  temporalCoupling,
} from './collect-git.ts'
import { evaluate, type EvaluationConfig } from './evaluate.ts'
import { reportExitCode, validateReport } from './schema.ts'
import type { FileFact, MetricsFacts, MetricsReport } from './types.ts'
import { formatProgress } from './util.ts'

const complexityConfig = {
  decisionNodeKinds: [
    'IfStatement',
    'ForStatement',
    'ForInStatement',
    'ForOfStatement',
    'WhileStatement',
    'DoStatement',
    'CatchClause',
    'ConditionalExpression',
    'SwitchCase',
  ],
  logicalOperators: ['&&', '||', '??'],
}

function sourceFile(path: string, lines: number): FileFact {
  return {
    path,
    extension: '.ts',
    workspace: null,
    classification: { primary: 'production', tags: ['production'] },
    tracked: true,
    size: {
      bytes: lines,
      totalLines: lines,
      nonEmptyLines: lines,
      lineEnding: 'lf',
      binary: false,
      readable: true,
    },
    git: {},
    complexity: null,
  }
}

describe('first-priority structural metrics', () => {
  it('calculates the documented cognitive-complexity subset', () => {
    expect(
      measureSource('a.ts', 'function f(a, b) { if (a) { if (b) return 1 } }', complexityConfig)
        .functions[0]?.cognitive,
    ).toBe(3)
    expect(
      measureSource('a.ts', 'function f(a) { if (a) return 1; else return 2 }', complexityConfig)
        .functions[0]?.cognitive,
    ).toBe(2)
    expect(
      measureSource('a.ts', 'function f(a, b) { if (a && b) return 1 }', complexityConfig)
        .functions[0]?.cognitive,
    ).toBe(2)
    const nested = measureSource(
      'a.ts',
      'function outer(a) { if (a) { function inner(b) { if (b) return 1 } } }',
      complexityConfig,
    )
    expect(nested.functions.map((item) => item.cognitive).sort()).toEqual([1, 1])
    expect(nested.moduleCognitive + nested.functions.reduce((sum, item) => sum + item.cognitive, 0)).toBe(2)
    const switched = measureSource(
      'a.ts',
      'function f(x) { switch (x) { case 1: return 1; case 2: return 2 } }',
      complexityConfig,
    )
    expect(switched.functions[0]?.cognitive).toBe(1)
    expect(switched.functions[0]?.cyclomatic).toBe(3)
    expect(
      measureSource('a.ts', 'function f(n) { return n ? f(n - 1) : 0 }', complexityConfig)
        .functions[0]?.cognitive,
    ).toBe(2)
  })

  it('maps svelte script lines back to the original file', () => {
    const fact = measureSource(
      'a.svelte',
      '<div>\n</div>\n<script>\nfunction f() {\n  if (a) return 1\n}\n</script>\n',
      complexityConfig,
    )
    expect(fact.functions[0]).toMatchObject({ name: 'f', startLine: 4, cognitive: 1 })
  })

  it('counts overlapping duplicate intervals once', () => {
    expect(
      uniqueCoveredLines([
        { start: 1, end: 5 },
        { start: 3, end: 7 },
        { start: 10, end: 11 },
      ]),
    ).toBe(9)
  })

  it('reports only cyclic strongly connected components and instability', () => {
    const components = dependencyComponents([
      { from: 'apps/a/a.ts', to: 'apps/a/b.ts', kind: 'import' },
      { from: 'apps/a/b.ts', to: 'apps/a/a.ts', kind: 'import' },
      { from: 'apps/a/a.ts', to: 'packages/b/c.ts', kind: 'import' },
    ])
    expect(components).toEqual([
      {
        files: ['apps/a/a.ts', 'apps/a/b.ts'],
        cyclic: true,
        internalEdges: 2,
        incomingEdges: 0,
        outgoingEdges: 1,
      },
    ])
    expect(instability(1, 1)).toBe(0.5)
    expect(instability(0, 0)).toBeNull()
    expect(
      workspaceCoupling([{ from: 'apps/a/a.ts', to: 'packages/b/c.ts', kind: 'import' }]),
    ).toEqual([
      { scope: 'workspace', key: 'apps/a', afferent: 0, efferent: 1, instability: 1 },
      { scope: 'workspace', key: 'packages/b', afferent: 1, efferent: 0, instability: 0 },
    ])
  })

  it('keeps git windows relative to HEAD and summarizes temporal coupling', () => {
    expect(coverageDays('2026-01-15T00:00:00.000Z', '2026-01-25T00:00:00.000Z')).toBe(10)
    expect(
      gitLogArgs(
        { id: '90d', days: 90 },
        {
          excludeMerges: true,
          followRenames: true,
          referenceAt: '2026-04-15T00:00:00.000Z',
        },
      ),
    ).toContain('--since=2026-01-15T00:00:00.000Z')
    expect(normalizeNumstatPath('apps/{old => new}/file.ts')).toBe('apps/new/file.ts')
    expect(canonicalIdentity('A <a@x>', 'B <b@x>', { 'A <a@x>': 'Canonical <c@x>' })).toBe(
      'Canonical <c@x>',
    )
    expect(
      temporalCoupling(
        [{ files: ['a.ts', 'b.ts'] }, { files: ['a.ts', 'b.ts'] }, { files: ['a.ts'] }],
        2,
      ),
    ).toEqual([
      {
        firstFile: 'a.ts',
        secondFile: 'b.ts',
        cochangeCount: 2,
        firstCommits: 3,
        secondCommits: 2,
        confidenceFirstToSecond: 0.666667,
        confidenceSecondToFirst: 1,
        jaccard: 0.666667,
      },
    ])
  })
})

describe('promised metrics contract', () => {
  it('scores configured hotspot formulas per file', () => {
    const low = sourceFile('apps/a.ts', 10)
    const high = sourceFile('apps/b.ts', 30)
    const config: EvaluationConfig = {
      enabled: true,
      hotspotFormula: {
        terms: [{ metric: 'totalLines', weight: 1, normalization: 'rank' }],
        missing: 'skip',
      },
      candidateFilters: [],
    }
    const facts: MetricsFacts = {
      files: [low, high],
      duplication: [],
      crossClassificationDuplication: [],
      duplicationStatistics: null,
      graph: {
        nodes: [],
        edges: [],
        unresolved: [],
        cycles: [],
        components: [],
        coverage: null,
        externalEdgeCount: null,
      },
    }
    const result = evaluate(facts, config)
    expect(result.score).toBeNull()
    expect(result.matches.map((match) => [match.path, match.score])).toEqual([
      ['apps/b.ts', 1],
      ['apps/a.ts', 0.5],
    ])
  })

  it('rejects an invalid report and exposes the partial-failure exit code', () => {
    expect(() => validateReport({} as MetricsReport)).toThrow(/invalid metrics report/)
    expect(reportExitCode({ diagnostics: [{ severity: 'error' }] } as MetricsReport)).toBe(2)
    expect(reportExitCode({ diagnostics: [{ severity: 'info' }] } as MetricsReport)).toBe(0)
  })

  it('formats elapsed progress without affecting report data', () => {
    expect(formatProgress(1_000, 3_500, '复杂度 250/1748')).toBe(
      '[code-metrics] 2.5s  复杂度 250/1748\n',
    )
  })
})
