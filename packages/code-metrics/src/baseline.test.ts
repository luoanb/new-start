import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'

import { describe, expect, it } from 'vitest'

import { aggregate } from './aggregate.ts'
import { classify } from './classify.ts'
import { filterDependencies } from './collect-dependencies.ts'
import { splitDuplication } from './collect-duplication.ts'
import { inspectFile } from './inspect-file.ts'
import type { FileFact } from './types.ts'
import { commandFailure, nearestRankPercentile, normalizeReportPath } from './util.ts'

function file(filePath: string, primary: string): FileFact {
  return {
    path: filePath,
    extension: '.ts',
    workspace: null,
    classification: { primary, tags: [primary] },
    tracked: true,
    size: {
      bytes: 1,
      totalLines: 1,
      nonEmptyLines: 1,
      lineEnding: 'lf',
      binary: false,
      readable: true,
    },
    git: {},
    complexity: null,
  }
}

const rules = [
  { id: 'test', pattern: '\\.test\\.' },
  { id: 'production', pattern: '\\.ts$' },
]

describe('code metrics baseline', () => {
  it('uses configured classification priority', () => {
    expect(classify('apps/pixmax/src/foo.test.ts', rules).primary).toBe('test')
    expect(classify('apps/pixmax/src/foo.ts', rules).primary).toBe('production')
  })

  it('uses nearest-rank percentiles', () => {
    expect(nearestRankPercentile([1, 2, 3, 4], 90)).toBe(4)
    expect(nearestRankPercentile([], 50)).toBeNull()
  })

  it('counts CRLF and trailing newline consistently', () => {
    const root = mkdtempSync(path.join(tmpdir(), 'metrics-'))
    mkdirSync(path.join(root, 'apps'), { recursive: true })
    writeFileSync(path.join(root, 'apps/a.ts'), 'a\r\nb\r\n')
    const fact = inspectFile(root, { path: 'apps/a.ts', tracked: true }, rules, [])
    expect(fact.size).toMatchObject({ totalLines: 2, nonEmptyLines: 2, lineEnding: 'crlf' })
    rmSync(root, { recursive: true, force: true })
  })

  it('keeps the spawn failure reason', () => {
    const error = commandFailure('jscpd', {
      error: Object.assign(new Error('spawn ENAMETOOLONG'), { name: 'Error' }),
      status: null,
      signal: null,
      stdout: '',
      stderr: '',
    } as never)
    expect(error.message).toContain('spawn ENAMETOOLONG')
    expect(error.message).toContain('exit null')
  })

  it('normalizes absolute report paths and splits duplication by classification', () => {
    expect(normalizeReportPath('C:/repo', 'C:\\repo\\apps\\a.ts')).toBe('apps/a.ts')
    const match = {
      firstFile: 'apps/a.ts',
      firstStart: 1,
      firstEnd: 5,
      secondFile: 'packages/b.ts',
      secondStart: 1,
      secondEnd: 5,
      lines: 5,
    }
    const split = splitDuplication(
      [match, { ...match, secondFile: 'docs/readme.md' }],
      [
        file('apps/a.ts', 'production'),
        file('packages/b.ts', 'production'),
        file('docs/readme.md', 'docs'),
      ],
    )
    expect(split.sameClassification.map((item) => item.secondFile)).toEqual(['packages/b.ts'])
    expect(split.crossClassification.map((item) => item.secondFile)).toEqual(['docs/readme.md'])
  })

  it('keeps dependency edges inside the collected file set', () => {
    const files = [file('apps/a.ts', 'production'), file('apps/b.ts', 'production')]
    const filtered = filterDependencies(
      {
        modules: [
          {
            source: 'apps/a.ts',
            dependencies: [
              { module: './b', resolved: 'apps/b.ts', dependencyTypes: ['local'] },
              { module: 'vite', resolved: 'node_modules/vite/index.js', dependencyTypes: ['npm'] },
              { module: './missing', couldNotResolve: true },
            ],
          },
        ],
      },
      files,
      'C:/repo',
    )
    expect(filtered.edges).toEqual([{ from: 'apps/a.ts', to: 'apps/b.ts', kind: 'local' }])
    expect(filtered.externalEdgeCount).toBe(1)
    expect(filtered.unresolved).toEqual([
      { from: 'apps/a.ts', request: './missing', reason: 'unresolved' },
    ])
    expect(filtered.coverage).toBe(0.5)
  })

  it('aggregates an empty line population as null', () => {
    const file = {
      path: 'a.bin',
      extension: '.bin',
      workspace: null,
      classification: { primary: 'asset', tags: ['asset'] },
      size: { totalLines: null, nonEmptyLines: null, bytes: 4 },
    } as FileFact
    expect(aggregate([file], 1, [50])[0]?.sumTotalLines).toBeNull()
  })
})
