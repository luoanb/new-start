import type { Aggregate, FileFact } from './types.ts'
import { nearestRankPercentile } from './util.ts'

function summarize(
  dimension: string,
  key: string,
  files: readonly FileFact[],
  percentiles: readonly number[],
): Aggregate {
  const lineValues = files.flatMap((file) =>
    file.size.totalLines === null ? [] : [file.size.totalLines],
  )
  const sumTotalLines =
    lineValues.length === 0 ? null : lineValues.reduce((sum, value) => sum + value, 0)
  const nonEmptyValues = files.flatMap((file) =>
    file.size.nonEmptyLines === null ? [] : [file.size.nonEmptyLines],
  )
  return {
    dimension,
    key,
    sampleCount: files.length,
    sumBytes: files.reduce((sum, file) => sum + file.size.bytes, 0),
    sumTotalLines,
    sumNonEmptyLines:
      nonEmptyValues.length === 0 ? null : nonEmptyValues.reduce((sum, value) => sum + value, 0),
    min: lineValues.length === 0 ? null : Math.min(...lineValues),
    max: lineValues.length === 0 ? null : Math.max(...lineValues),
    mean: sumTotalLines === null ? null : sumTotalLines / lineValues.length,
    percentiles: Object.fromEntries(
      percentiles.map((percentile) => [
        `p${percentile}`,
        nearestRankPercentile(lineValues, percentile),
      ]),
    ),
  }
}

function group(
  files: readonly FileFact[],
  keyOf: (file: FileFact) => string | null,
): Map<string, FileFact[]> {
  const groups = new Map<string, FileFact[]>()
  for (const file of files) {
    const key = keyOf(file)
    if (key === null) continue
    groups.set(key, [...(groups.get(key) ?? []), file])
  }
  return groups
}

export function aggregate(
  files: readonly FileFact[],
  directoryDepth: number,
  percentiles: readonly number[],
): Aggregate[] {
  const dimensions: [string, (file: FileFact) => string | null][] = [
    ['repository', () => 'repository'],
    ['classification', (file) => file.classification.primary],
    ['workspace', (file) => file.workspace],
    ['extension', (file) => file.extension || '(none)'],
    ['directory', (file) => file.path.split('/').slice(0, directoryDepth).join('/')],
  ]
  return dimensions.flatMap(([dimension, keyOf]) =>
    [...group(files, keyOf).entries()]
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, grouped]) => summarize(dimension, key, grouped, percentiles)),
  )
}
