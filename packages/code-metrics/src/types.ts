export type Nullable<T> = T | null

export type Classification = {
  primary: string
  tags: string[]
}

export type SizeFact = {
  bytes: number
  totalLines: Nullable<number>
  nonEmptyLines: Nullable<number>
  lineEnding: 'lf' | 'crlf' | 'mixed' | 'none'
  binary: boolean
  readable: boolean
}

export type GitWindowFact = {
  commits: Nullable<number>
  added: Nullable<number>
  deleted: Nullable<number>
  churn: Nullable<number>
  lastModifiedAt: Nullable<string>
  rawContributors: string[]
  canonicalContributors: string[]
}

export type ComplexityFunctionFact = {
  name: string
  startLine: number
  endLine: number
  cyclomatic: number
  cognitive: number
  decisions: string[]
}

export type ComplexityFact = {
  functions: ComplexityFunctionFact[]
  functionCount: number
  moduleCognitive: number
  templateBranches: Nullable<number>
  status: 'ok' | 'unsupported' | 'failed'
  reason: Nullable<string>
}

export type DuplicationMatch = {
  firstFile: string
  firstStart: number
  firstEnd: number
  secondFile: string
  secondStart: number
  secondEnd: number
  lines: number
}

export type DuplicationClusterMember = {
  file: string
  start: number
  end: number
}

export type DuplicationCluster = {
  instances: number
  fileCount: number
  maxLines: number
  pairCount: number
  members: DuplicationClusterMember[]
}

export type DuplicationStatistics = {
  duplicatedLines: Nullable<number>
  duplicatedTokens: Nullable<number>
  percentage: Nullable<number>
  percentageTokens: Nullable<number>
}

export type DuplicationDensity = {
  uniqueDuplicatedLines: number
  analyzableFiles: number
  analyzableLines: number
  density: Nullable<number>
}

export type DependencyEdge = {
  from: string
  to: string
  kind: string
}

export type DependencyComponent = {
  files: string[]
  cyclic: boolean
  internalEdges: number
  incomingEdges: number
  outgoingEdges: number
}

export type CouplingFact = {
  scope: 'workspace'
  key: string
  afferent: number
  efferent: number
  instability: Nullable<number>
}

export type TemporalCoupling = {
  firstFile: string
  secondFile: string
  cochangeCount: number
  firstCommits: number
  secondCommits: number
  confidenceFirstToSecond: number
  confidenceSecondToFirst: number
  jaccard: number
}

export type DependencyCycle = {
  path: string[]
}

export type FileFact = {
  path: string
  extension: string
  workspace: Nullable<string>
  classification: Classification
  tracked: boolean
  size: SizeFact
  git: Record<string, GitWindowFact>
  complexity: Nullable<ComplexityFact>
}

export type Aggregate = {
  dimension: string
  key: string
  sampleCount: number
  sumBytes: number
  sumTotalLines: Nullable<number>
  sumNonEmptyLines: Nullable<number>
  min: Nullable<number>
  max: Nullable<number>
  mean: Nullable<number>
  percentiles: Record<string, Nullable<number>>
}

export type Ranking = {
  metric: string
  path: string
  rank: number
  percentile: number
}

export type Diagnostic = {
  collector: string
  severity: 'info' | 'warning' | 'error'
  path: Nullable<string>
  message: string
}

export type MetricsFacts = {
  files: FileFact[]
  duplication: DuplicationMatch[]
  crossClassificationDuplication: DuplicationMatch[]
  duplicationStatistics: Nullable<DuplicationStatistics>
  graph: {
    nodes: string[]
    edges: DependencyEdge[]
    unresolved: { from: string; request: string; reason: string }[]
    cycles: DependencyCycle[]
    components: DependencyComponent[]
    coverage: Nullable<number>
    externalEdgeCount: Nullable<number>
  }
}

export type MetricsReport = {
  schemaVersion: string
  meta: {
    generatedAt: string
    durationMs: number
    commit: Nullable<string>
    dirty: boolean
    historyStart: Nullable<string>
    historyCoverageDays: Nullable<number>
    node: string
    pnpm: string
    platform: string
    configHash: string
    command: string[]
    factsFile: string
    readingLimit: number
    duplicationCluster: { ratioBase: string; threshold: number }
    tools: { typescript: string; jscpd: string; dependencyCruiser: string }
    git: {
      referenceAt: Nullable<string>
      excludeMerges: boolean
      followRenames: boolean
      temporalWindow: string
      temporalMinCount: number
    }
  }
  scope: {
    source: 'tracked' | 'worktree'
    include: string[]
    extensions: string[]
    excludedDirectories: string[]
    classification: { id: string; pattern: string }[]
  }
  derived: {
    aggregates: Aggregate[]
    rankings: Ranking[]
    duplicationClusters: DuplicationCluster[]
    duplicationDensity: DuplicationDensity
    coupling: CouplingFact[]
    temporalCoupling: TemporalCoupling[]
  }
  evaluation: {
    enabled: boolean
    formula: Nullable<unknown>
    score: Nullable<number>
    matches: {
      ruleId: string
      path: string
      values: Record<string, Nullable<number>>
      score: Nullable<number>
    }[]
  }
  diagnostics: Diagnostic[]
}
