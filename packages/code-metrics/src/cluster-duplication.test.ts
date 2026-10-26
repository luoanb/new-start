import { describe, expect, it } from 'vitest'

import { clusterDuplication } from './cluster-duplication.ts'
import type { DuplicationMatch } from './types.ts'

const DEFAULT_CONFIG = { clusterRatioBase: 'shorter', clusterThreshold: 0.6 } as const

function match(
  firstFile: string,
  firstStart: number,
  firstEnd: number,
  secondFile: string,
  secondStart: number,
  secondEnd: number,
): DuplicationMatch {
  return {
    firstFile,
    firstStart,
    firstEnd,
    secondFile,
    secondStart,
    secondEnd,
    lines: Math.min(firstEnd - firstStart + 1, secondEnd - secondStart + 1),
  }
}

describe('clusterDuplication', () => {
  it('returns empty for empty input', () => {
    expect(clusterDuplication([], DEFAULT_CONFIG)).toEqual([])
  })

  it('connects star-shaped pairs into one cluster', () => {
    const clusters = clusterDuplication(
      [
        match('a.ts', 1, 50, 'b.ts', 1, 50),
        match('a.ts', 1, 50, 'c.ts', 1, 50),
        match('a.ts', 1, 50, 'd.ts', 1, 50),
      ],
      DEFAULT_CONFIG,
    )
    expect(clusters).toHaveLength(1)
    expect(clusters[0]).toEqual({
      instances: 4,
      fileCount: 4,
      maxLines: 50,
      pairCount: 3,
      members: [
        { file: 'a.ts', start: 1, end: 50 },
        { file: 'b.ts', start: 1, end: 50 },
        { file: 'c.ts', start: 1, end: 50 },
        { file: 'd.ts', start: 1, end: 50 },
      ],
    })
  })

  it('merges a nested sub-clone into its parent cluster under the shorter base', () => {
    const matches = [match('f.ts', 1, 100, 'g.ts', 1, 100), match('f.ts', 10, 39, 'h.ts', 1, 30)]
    const shorter = clusterDuplication(matches, DEFAULT_CONFIG)
    expect(shorter).toHaveLength(1)
    expect(shorter[0].instances).toBe(4)
    expect(shorter[0].fileCount).toBe(3)

    const longer = clusterDuplication(matches, {
      clusterRatioBase: 'longer',
      clusterThreshold: 0.6,
    })
    expect(longer).toHaveLength(2)
    expect(longer.map((cluster) => cluster.instances).sort((left, right) => left - right)).toEqual([
      2, 2,
    ])
  })

  it('counts same-file self duplication as two instances in one file', () => {
    const clusters = clusterDuplication([match('f.ts', 10, 50, 'f.ts', 60, 100)], DEFAULT_CONFIG)
    expect(clusters).toHaveLength(1)
    expect(clusters[0].instances).toBe(2)
    expect(clusters[0].fileCount).toBe(1)
  })

  it('merges transitively when adjacent overlaps pass the threshold', () => {
    const clusters = clusterDuplication(
      [
        match('f.ts', 1, 100, 'g.ts', 1, 100),
        match('f.ts', 40, 140, 'h.ts', 1, 100),
        match('f.ts', 80, 180, 'i.ts', 1, 100),
      ],
      DEFAULT_CONFIG,
    )
    expect(clusters).toHaveLength(1)
    expect(clusters[0].instances).toBe(6)
    expect(clusters[0].fileCount).toBe(4)
  })

  it('keeps adjacent overlapping clusters apart below the threshold', () => {
    const clusters = clusterDuplication(
      [match('f.ts', 1, 100, 'g.ts', 1, 100), match('f.ts', 90, 190, 'h.ts', 1, 100)],
      DEFAULT_CONFIG,
    )
    expect(clusters).toHaveLength(2)
  })

  it('rejects unknown ratio base instead of silently merging', () => {
    expect(() =>
      clusterDuplication([match('a.ts', 1, 10, 'b.ts', 1, 10)], {
        // @ts-expect-error 故意传入未知口径，验证运行时拒绝
        clusterRatioBase: 'unknown',
        clusterThreshold: 0.6,
      }),
    ).toThrow('unknown duplication clusterRatioBase')
  })

  it('sorts clusters by duplicated volume (instances × maxLines)', () => {
    const clusters = clusterDuplication(
      [
        match('z.ts', 1, 20, 'y.ts', 1, 20),
        match('a.ts', 1, 10, 'b.ts', 1, 10),
        match('a.ts', 1, 10, 'c.ts', 1, 10),
      ],
      DEFAULT_CONFIG,
    )
    // 体量：z-y 为 2×20=40，a-b-c 为 3×10=30，按体量降序而非次数。
    expect(clusters.map((cluster) => cluster.instances)).toEqual([2, 3])
    expect(clusters[0].members.map((member) => member.file)).toEqual(['y.ts', 'z.ts'])
    expect(clusters[1].members.map((member) => member.file)).toEqual(['a.ts', 'b.ts', 'c.ts'])
  })
})
