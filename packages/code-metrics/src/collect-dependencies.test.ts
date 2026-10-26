import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'

import { afterAll, describe, expect, it } from 'vitest'

import {
  groupFilesByTsConfig,
  mergeCruiseReports,
  nearestTsConfigPath,
  shallowRoots,
} from './collect-dependencies.ts'

const repoRoot = mkdtempSync(path.join(tmpdir(), 'code-metrics-depcruise-test-'))
mkdirSync(path.join(repoRoot, 'apps/a/src'), { recursive: true })
mkdirSync(path.join(repoRoot, 'apps/b/src'), { recursive: true })
mkdirSync(path.join(repoRoot, 'scripts'), { recursive: true })
mkdirSync(path.join(repoRoot, 'packages/c/src'), { recursive: true })
writeFileSync(path.join(repoRoot, 'tsconfig.json'), '{}')
writeFileSync(path.join(repoRoot, 'apps/a/tsconfig.json'), '{}')
writeFileSync(path.join(repoRoot, 'packages/c/tsconfig.json'), '{}')

afterAll(() => {
  // Windows 上 tmp 目录由系统清理；此处不做强制删除。
})

describe('nearestTsConfigPath', () => {
  it('找到所在目录的 tsconfig', () => {
    expect(nearestTsConfigPath(repoRoot, 'apps/a/src/index.ts')).toBe('apps/a/tsconfig.json')
  })

  it('逐级向上查找', () => {
    expect(nearestTsConfigPath(repoRoot, 'packages/c/src/nested/deep/x.ts')).toBe(
      'packages/c/tsconfig.json',
    )
  })

  it('无更近 tsconfig 时命中仓库根的 tsconfig', () => {
    expect(nearestTsConfigPath(repoRoot, 'apps/b/src/index.ts')).toBe('tsconfig.json')
  })
})

describe('shallowRoots', () => {
  it('取前两段作为目录根', () => {
    expect(
      shallowRoots([
        { path: 'apps/a/src/index.ts' },
        { path: 'apps/b/src/main.ts' },
        { path: 'scripts/count.mjs' },
      ]),
    ).toEqual(['apps/a', 'apps/b', 'scripts'])
  })
})

describe('groupFilesByTsConfig', () => {
  it('按最近 tsconfig 分组，缺省归回退组', () => {
    const groups = groupFilesByTsConfig(
      repoRoot,
      ['apps/a/src/index.ts', 'apps/b/src/main.ts', 'packages/c/src/x.ts'],
      'tsconfig.json',
    )
    expect(groups).toEqual([
      { tsConfig: 'apps/a/tsconfig.json', roots: ['apps/a'] },
      { tsConfig: 'packages/c/tsconfig.json', roots: ['packages/c'] },
      { tsConfig: 'tsconfig.json', roots: ['apps/b'] },
    ])
  })
})

describe('mergeCruiseReports', () => {
  it('权威组的模块记录覆盖其他组 follow 到的同名占位', () => {
    const reportA = {
      modules: [
        {
          source: 'apps/a/src/index.ts',
          dependencies: [{ module: './local', resolved: 'apps/a/src/local.ts' }],
        },
        {
          source: 'apps/b/src/main.ts',
          dependencies: [{ module: 'stale', couldNotResolve: true }],
        },
      ],
    }
    const reportFallback = {
      modules: [
        {
          source: 'apps/b/src/main.ts',
          dependencies: [{ module: './fresh', resolved: 'apps/b/src/fresh.ts' }],
        },
      ],
    }
    const merged = mergeCruiseReports(
      [
        { tsConfig: 'apps/a/tsconfig.json', report: reportA },
        { tsConfig: 'tsconfig.json', report: reportFallback },
      ],
      repoRoot,
    )
    const bySource = new Map((merged.modules ?? []).map((module) => [module.source, module]))
    expect(bySource.get('apps/a/src/index.ts')?.dependencies?.[0]?.module).toBe('./local')
    // apps/b 的权威组是回退组，必须覆盖 reportA 中被 follow 到的占位记录。
    expect(bySource.get('apps/b/src/main.ts')?.dependencies?.[0]?.module).toBe('./fresh')
  })
})
