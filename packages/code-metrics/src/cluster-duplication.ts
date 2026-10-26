import type { DuplicationCluster, DuplicationClusterMember, DuplicationMatch } from './types.ts'

export type DuplicationClusterConfig = {
  clusterRatioBase: 'shorter' | 'longer' | 'union'
  clusterThreshold: number
}

const RATIO_BASES = ['shorter', 'longer', 'union'] as const

type Interval = { file: string; start: number; end: number }

/**
 * 交叠率 = 交叠行数 / 分母。
 * 区间长度按 end - start 计（与归并算法基准一致，行号本身为闭区间）。
 * 分母口径：shorter 取较短区间长度；longer 取较长区间长度；union 取两区间的合并跨度。
 */
function overlapRatio(
  first: { start: number; end: number },
  second: { start: number; end: number },
  ratioBase: DuplicationClusterConfig['clusterRatioBase'],
): number {
  const overlap = Math.min(first.end, second.end) - Math.max(first.start, second.start)
  if (overlap <= 0) return 0
  const spanFirst = first.end - first.start
  const spanSecond = second.end - second.start
  const base =
    ratioBase === 'shorter'
      ? Math.min(spanFirst, spanSecond)
      : ratioBase === 'longer'
        ? Math.max(spanFirst, spanSecond)
        : spanFirst + spanSecond - overlap
  return base > 0 ? overlap / base : 0
}

export function clusterDuplication(
  matches: readonly DuplicationMatch[],
  config: DuplicationClusterConfig,
): DuplicationCluster[] {
  if (!RATIO_BASES.includes(config.clusterRatioBase)) {
    throw new Error(`unknown duplication clusterRatioBase: ${String(config.clusterRatioBase)}`)
  }
  if (matches.length === 0) return []

  const intervals: Interval[] = []
  const intervalIds = new Map<string, number>()
  const parent: number[] = []
  const intervalId = (interval: Interval): number => {
    const key = `${interval.file}#${interval.start}-${interval.end}`
    const existing = intervalIds.get(key)
    if (existing !== undefined) return existing
    const id = intervals.length
    intervalIds.set(key, id)
    intervals.push(interval)
    parent.push(id)
    return id
  }
  const find = (id: number): number => {
    let root = id
    while (parent[root] !== root) root = parent[root]
    while (parent[id] !== root) {
      const next = parent[id]
      parent[id] = root
      id = next
    }
    return root
  }
  const union = (left: number, right: number): void => {
    const leftRoot = find(left)
    const rightRoot = find(right)
    if (leftRoot !== rightRoot) parent[leftRoot] = rightRoot
  }

  const pairEnds: [number, number][] = []
  for (const match of matches) {
    const first = intervalId({
      file: match.firstFile,
      start: match.firstStart,
      end: match.firstEnd,
    })
    const second = intervalId({
      file: match.secondFile,
      start: match.secondStart,
      end: match.secondEnd,
    })
    union(first, second)
    pairEnds.push([first, second])
  }

  const indicesByFile = new Map<string, number[]>()
  for (let index = 0; index < intervals.length; index++) {
    const file = intervals[index].file
    indicesByFile.set(file, [...(indicesByFile.get(file) ?? []), index])
  }
  for (const indices of indicesByFile.values()) {
    const sorted = [...indices].sort(
      (left, right) =>
        intervals[left].start - intervals[right].start ||
        intervals[left].end - intervals[right].end,
    )
    let current: { end: number; members: number[] } | null = null
    const flush = (): void => {
      if (current) {
        for (let index = 1; index < current.members.length; index++) {
          union(current.members[0], current.members[index])
        }
      }
      current = null
    }
    for (const index of sorted) {
      const interval = intervals[index]
      if (
        current &&
        overlapRatio(
          { start: intervals[current.members[0]].start, end: current.end },
          interval,
          config.clusterRatioBase,
        ) >= config.clusterThreshold
      ) {
        current.end = Math.max(current.end, interval.end)
        current.members.push(index)
      } else {
        flush()
        current = { end: interval.end, members: [index] }
      }
    }
    flush()
  }

  const components = new Map<number, number[]>()
  for (let index = 0; index < intervals.length; index++) {
    const root = find(index)
    components.set(root, [...(components.get(root) ?? []), index])
  }
  const pairCountByRoot = new Map<number, number>()
  for (const [first] of pairEnds) {
    const root = find(first)
    pairCountByRoot.set(root, (pairCountByRoot.get(root) ?? 0) + 1)
  }

  const clusters: DuplicationCluster[] = [...components.entries()].map(([root, indices]) => {
    const members: DuplicationClusterMember[] = indices
      .map((index) => intervals[index])
      .sort(
        (left, right) =>
          left.file.localeCompare(right.file) || left.start - right.start || left.end - right.end,
      )
    return {
      instances: indices.length,
      fileCount: new Set(indices.map((index) => intervals[index].file)).size,
      maxLines: Math.max(
        ...indices.map((index) => intervals[index].end - intervals[index].start + 1),
      ),
      pairCount: pairCountByRoot.get(root) ?? 0,
      members,
    }
  })
  return clusters.sort(
    (left, right) =>
      right.instances * right.maxLines - left.instances * left.maxLines ||
      right.instances - left.instances ||
      right.maxLines - left.maxLines ||
      left.members[0].file.localeCompare(right.members[0].file) ||
      left.members[0].start - right.members[0].start,
  )
}
