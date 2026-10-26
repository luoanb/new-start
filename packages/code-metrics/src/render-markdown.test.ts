import { describe, expect, it } from 'vitest'

import { renderMarkdown } from './render-markdown.ts'
import type { FileFact, MetricsFacts, MetricsReport } from './types.ts'

function file(path: string, lines: number): FileFact {
  return {
    path,
    extension: '.ts',
    workspace: 'apps/demo',
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
    git: {
      '90d': {
        commits: 1,
        added: 2,
        deleted: 1,
        churn: 3,
        lastModifiedAt: null,
        rawContributors: [],
        canonicalContributors: [],
      },
    },
    complexity: {
      functions: [],
      functionCount: 0,
      moduleCognitive: 0,
      templateBranches: null,
      status: 'ok',
      reason: null,
    },
  }
}

function sample(): { report: MetricsReport; facts: MetricsFacts } {
  const files = [file('apps/a.ts', 10), file('apps/b.ts', 30), file('apps/c.ts', 20)]
  files[1].complexity?.functions.push({
    name: 'doWork',
    startLine: 3,
    endLine: 9,
    cyclomatic: 30,
    cognitive: 30,
    decisions: [],
  })
  const facts: MetricsFacts = {
    files,
    duplication: [
      {
        firstFile: 'apps/a.ts',
        firstStart: 1,
        firstEnd: 5,
        secondFile: 'apps/b.ts',
        secondStart: 1,
        secondEnd: 8,
        lines: 8,
      },
      {
        firstFile: 'apps/a.ts',
        firstStart: 2,
        firstEnd: 3,
        secondFile: 'apps/c.ts',
        secondStart: 2,
        secondEnd: 3,
        lines: 2,
      },
      {
        firstFile: 'apps/b.ts',
        firstStart: 1,
        firstEnd: 4,
        secondFile: 'apps/c.ts',
        secondStart: 1,
        secondEnd: 4,
        lines: 4,
      },
    ],
    crossClassificationDuplication: [],
    duplicationStatistics: null,
    graph: {
      nodes: ['apps/a.ts'],
      edges: [],
      unresolved: [
        { from: 'apps/a.ts', request: 'missing', reason: 'unresolved' },
        { from: 'apps/b.ts', request: 'missing', reason: 'unresolved' },
        { from: 'apps/c.ts', request: 'other', reason: 'unresolved' },
      ],
      cycles: [{ path: ['apps/a.ts', 'apps/b.ts', 'apps/a.ts'] }],
      components: [
        {
          files: ['apps/a.ts', 'apps/b.ts'],
          cyclic: true,
          internalEdges: 2,
          incomingEdges: 0,
          outgoingEdges: 0,
        },
      ],
      coverage: 1,
      externalEdgeCount: 0,
    },
  }
  const report: MetricsReport = {
    schemaVersion: '1.3.0',
    meta: {
      generatedAt: '2026-10-19T00:00:00.000Z',
      durationMs: 1,
      commit: 'abc',
      dirty: false,
      historyStart: '2026-01-15T00:00:00.000Z',
      historyCoverageDays: 277,
      node: 'v22',
      pnpm: '11',
      platform: 'win32',
      configHash: 'hash',
      command: ['all'],
      factsFile: 'facts.json',
      readingLimit: 2,
      duplicationCluster: { ratioBase: 'shorter', threshold: 0.6 },
      tools: { typescript: '5.9.3', jscpd: '4.0.8', dependencyCruiser: '17.3.10' },
      git: {
        referenceAt: '2026-10-26T00:00:00.000Z',
        excludeMerges: true,
        followRenames: true,
        temporalWindow: '90d',
        temporalMinCount: 3,
      },
    },
    scope: {
      source: 'tracked',
      include: ['apps'],
      extensions: ['.ts'],
      excludedDirectories: [],
      classification: [{ id: 'production', pattern: '\\.ts$' }],
    },
    derived: {
      aggregates: [
        {
          dimension: 'repository',
          key: 'repository',
          sampleCount: 3,
          sumBytes: 60,
          sumTotalLines: 60,
          sumNonEmptyLines: 60,
          min: 10,
          max: 30,
          mean: 20,
          percentiles: { p90: 30 },
        },
      ],
      rankings: [
        ...files.map((item, index) => ({
          metric: 'totalLines',
          path: item.path,
          rank: index + 1,
          percentile: 1,
        })),
        ...files.map((item, index) => ({
          metric: 'complexityMax',
          path: item.path,
          rank: index + 1,
          percentile: 1,
        })),
        { metric: 'cognitiveMax', path: 'apps/b.ts', rank: 1, percentile: 100 },
      ],
      duplicationDensity: {
        uniqueDuplicatedLines: 0,
        analyzableFiles: 3,
        analyzableLines: 60,
        density: 0,
      },
      coupling: [],
      temporalCoupling: [],
      duplicationClusters: [
        {
          instances: 3,
          fileCount: 3,
          maxLines: 8,
          pairCount: 3,
          members: [
            { file: 'apps/a.ts', start: 1, end: 5 },
            { file: 'apps/b.ts', start: 1, end: 8 },
            { file: 'apps/c.ts', start: 1, end: 4 },
          ],
        },
      ],
    },
    evaluation: { enabled: false, formula: null, score: null, matches: [] },
    diagnostics: [{ collector: 'dependency', severity: 'info', path: null, message: 'edges=0' }],
  }
  return { report, facts }
}

describe('renderMarkdown', () => {
  it('limits each reading table and keeps the values from the report', () => {
    const { report, facts } = sample()
    const markdown = renderMarkdown(report, facts)
    expect(markdown).toContain('展示前 2 条，完整记录共 3 条')
    expect(markdown).not.toContain('apps/c.ts | production | 20 |')
    expect(markdown).toContain('apps/a.ts -> apps/b.ts -> apps/a.ts')
    expect(markdown).toContain('| missing | 2 |')
    expect(markdown).not.toContain('### extension')
    expect(markdown).toContain('## 指标定义')
    expect(markdown).toContain('圈复杂度 = 1 + 决策节点数')
    expect(markdown).toContain('新增行数加删除行数')
    expect(markdown).toContain('重复次数可以大于文件数')
    expect(markdown).toContain('## 重复簇')
    expect(markdown).toContain('| 3 | 3 | 8 | 3 | apps/a.ts:1-5、apps/b.ts:1-8、apps/c.ts:1-4 |')
    expect(markdown).not.toContain('应该重构')
  })

  it('reads complexity numbers from function facts instead of a copied value', () => {
    const { report, facts } = sample()
    const markdown = renderMarkdown(report, facts)
    expect(markdown).toContain('| 最大复杂度函数 |')
    expect(markdown).toContain('doWork (3-9)')
    expect(markdown).toContain('### 认知复杂度')
    expect(markdown).toContain('| 1 | apps/b.ts | production | 30 | 30 | 30 |')
  })
})
