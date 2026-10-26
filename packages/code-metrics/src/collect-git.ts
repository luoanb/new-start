import type { FileFact, GitWindowFact, TemporalCoupling } from './types.ts'
import { roundRatio, runGit } from './util.ts'

export type GitWindow = {
  id: string
  days: number | null
}

export type GitCollectionOptions = {
  excludeMerges: boolean
  followRenames: boolean
  identityMap: Readonly<Record<string, string>>
  referenceAt: string | null
  temporalWindow: string
  temporalMinCount: number
  onProgress?: (message: string) => void
}

type RawGitFact = {
  commits: Set<string>
  added: number
  deleted: number
  lastModifiedAt: string | null
  rawContributors: Set<string>
  canonicalContributors: Set<string>
}

function empty(): RawGitFact {
  return {
    commits: new Set(),
    added: 0,
    deleted: 0,
    lastModifiedAt: null,
    rawContributors: new Set(),
    canonicalContributors: new Set(),
  }
}

export function canonicalIdentity(
  raw: string,
  mailmap: string,
  identityMap: Readonly<Record<string, string>>,
): string {
  return identityMap[raw] ?? identityMap[mailmap] ?? mailmap
}

export function coverageDays(start: string, head: string): number {
  return Math.floor((Date.parse(head) - Date.parse(start)) / 86_400_000)
}

export function gitLogArgs(
  window: GitWindow,
  options: Pick<GitCollectionOptions, 'excludeMerges' | 'followRenames' | 'referenceAt'>,
): string[] {
  const args = ['log', '--numstat', '--format=%H%x09%cI%x09%an <%ae>%x09%aN <%aE>']
  if (options.excludeMerges) args.push('--no-merges')
  if (options.followRenames) args.push('--find-renames')
  if (window.days !== null) {
    if (!options.referenceAt) throw new Error('git reference time is missing')
    const since = new Date(Date.parse(options.referenceAt) - window.days * 86_400_000).toISOString()
    args.push(`--since=${since}`)
  }
  return args
}

export function normalizeNumstatPath(raw: string): string {
  const filePath = raw.trim()
  const brace = /^(.*)\{(.*?) => (.*?)\}(.*)$/.exec(filePath)
  if (brace) return `${brace[1]}${brace[3]}${brace[4]}`.replaceAll('//', '/')
  if (filePath.includes(' => ')) return filePath.split(' => ').at(-1)?.trim() ?? filePath
  return filePath
}

export function parseCommitHeader(
  line: string,
): { commit: string; modifiedAt: string; rawAuthor: string; mailmapAuthor: string } | null {
  const [commit, modifiedAt, rawAuthor, mailmapAuthor] = line.split('\t')
  if (!commit || !modifiedAt || !rawAuthor || !mailmapAuthor) return null
  if (!/^[0-9a-f]{7,}$/i.test(commit) || !modifiedAt.includes('T')) return null
  return { commit, modifiedAt, rawAuthor, mailmapAuthor }
}

export function temporalCoupling(
  commits: readonly { files: readonly string[] }[],
  minCount: number,
): TemporalCoupling[] {
  const pairCounts = new Map<string, number>()
  const fileCounts = new Map<string, number>()
  for (const commit of commits) {
    const files = [...new Set(commit.files)].sort()
    for (const file of files) fileCounts.set(file, (fileCounts.get(file) ?? 0) + 1)
    for (let left = 0; left < files.length; left += 1) {
      for (let right = left + 1; right < files.length; right += 1) {
        const key = `${files[left]}\0${files[right]}`
        pairCounts.set(key, (pairCounts.get(key) ?? 0) + 1)
      }
    }
  }
  const pairs: TemporalCoupling[] = []
  for (const [key, cochangeCount] of pairCounts) {
    if (cochangeCount < minCount) continue
    const [firstFile, secondFile] = key.split('\0')
    const firstCommits = fileCounts.get(firstFile) ?? 0
    const secondCommits = fileCounts.get(secondFile) ?? 0
    const union = firstCommits + secondCommits - cochangeCount
    pairs.push({
      firstFile,
      secondFile,
      cochangeCount,
      firstCommits,
      secondCommits,
      confidenceFirstToSecond: firstCommits === 0 ? 0 : roundRatio(cochangeCount / firstCommits),
      confidenceSecondToFirst: secondCommits === 0 ? 0 : roundRatio(cochangeCount / secondCommits),
      jaccard: union === 0 ? 0 : roundRatio(cochangeCount / union),
    })
  }
  return pairs.sort(
    (left, right) =>
      right.cochangeCount - left.cochangeCount ||
      right.jaccard - left.jaccard ||
      left.firstFile.localeCompare(right.firstFile) ||
      left.secondFile.localeCompare(right.secondFile),
  )
}

export function historyBounds(repoRoot: string): {
  start: string | null
  coverageDays: number | null
  head: string | null
} {
  const head = runGit(repoRoot, ['log', '-1', '--format=%cI', 'HEAD']).trim() || null
  const roots = runGit(repoRoot, ['rev-list', '--max-parents=0', 'HEAD'])
    .trim()
    .split(/\r?\n/)
    .filter(Boolean)
  const start =
    roots
      .map((commit) => runGit(repoRoot, ['log', '-1', '--format=%cI', commit]).trim())
      .filter(Boolean)
      .sort()[0] ?? null
  if (!start || !head) return { start, coverageDays: null, head }
  return { start, coverageDays: coverageDays(start, head), head }
}

export function collectGit(
  repoRoot: string,
  files: readonly FileFact[],
  windows: readonly GitWindow[],
  options: GitCollectionOptions,
): TemporalCoupling[] {
  const byPath = new Map(files.map((file) => [file.path, file]))
  const raw = new Map<string, Map<string, RawGitFact>>()
  const temporalCommits: { files: string[] }[] = []
  windows.forEach((window, index) => {
    options.onProgress?.(`Git ${index + 1}/${windows.length} ${window.id}`)
    const output = runGit(repoRoot, gitLogArgs(window, options))
    let commit = ''
    let modifiedAt = ''
    let rawAuthor = ''
    let mailmapAuthor = ''
    const filesInCommit = new Set<string>()
    const flush = () => {
      if (window.id === options.temporalWindow && commit && filesInCommit.size > 0) {
        temporalCommits.push({ files: [...filesInCommit] })
      }
      filesInCommit.clear()
    }
    for (const line of output.split(/\r?\n/)) {
      if (!line) continue
      const header = parseCommitHeader(line)
      if (header || line.includes('\t')) {
        if (header) {
          flush()
          commit = header.commit
          modifiedAt = header.modifiedAt
          rawAuthor = header.rawAuthor
          mailmapAuthor = header.mailmapAuthor
          continue
        }
      }
      if (!header && line.split('\t').length >= 4 && line.includes('T')) {
        commit = ''
        continue
      }
      const [added, deleted, filePath] = line.split('\t')
      const normalized = filePath ? normalizeNumstatPath(filePath) : ''
      if (!commit || !normalized || !byPath.has(normalized)) continue
      const windowFacts = raw.get(normalized) ?? new Map<string, RawGitFact>()
      const fact = windowFacts.get(window.id) ?? empty()
      fact.commits.add(commit)
      fact.lastModifiedAt ??= modifiedAt
      fact.rawContributors.add(rawAuthor)
      fact.canonicalContributors.add(
        canonicalIdentity(rawAuthor, mailmapAuthor, options.identityMap),
      )
      if (added !== '-' && deleted !== '-') {
        fact.added += Number(added)
        fact.deleted += Number(deleted)
      }
      windowFacts.set(window.id, fact)
      raw.set(normalized, windowFacts)
      if (window.id === options.temporalWindow) filesInCommit.add(normalized)
    }
    flush()
  })

  for (const file of files) {
    const windowFacts = raw.get(file.path) ?? new Map<string, RawGitFact>()
    file.git = Object.fromEntries(
      windows.map((window) => {
        const fact = windowFacts.get(window.id)
        const value: GitWindowFact = fact
          ? {
              commits: fact.commits.size,
              added: fact.added,
              deleted: fact.deleted,
              churn: fact.added + fact.deleted,
              lastModifiedAt: fact.lastModifiedAt,
              rawContributors: [...fact.rawContributors].sort(),
              canonicalContributors: [...fact.canonicalContributors].sort(),
            }
          : {
              commits: 0,
              added: 0,
              deleted: 0,
              churn: 0,
              lastModifiedAt: null,
              rawContributors: [],
              canonicalContributors: [],
            }
        return [window.id, value]
      }),
    )
  }
  return temporalCoupling(temporalCommits, options.temporalMinCount)
}
