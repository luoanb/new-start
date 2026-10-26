import { spawnSync } from 'node:child_process'
import { mkdirSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

import { aggregate } from './aggregate.ts'
import { clusterDuplication } from './cluster-duplication.ts'
import { collectComplexity } from './collect-complexity.ts'
import { collectDependencies, workspaceCoupling } from './collect-dependencies.ts'
import { collectDuplication } from './collect-duplication.ts'
import { collectGit, historyBounds } from './collect-git.ts'
import { evaluate, type EvaluationConfig } from './evaluate.ts'
import { inspectFile } from './inspect-file.ts'
import { rank } from './rank.ts'
import { summarizeDuplicationDensity } from './read-metrics.ts'
import { renderMarkdown } from './render-markdown.ts'
import { validateReport } from './schema.ts'
import type { Diagnostic, MetricsFacts, MetricsReport } from './types.ts'
import { createProgress, dependencyVersion, runGit, sha256, toPosix } from './util.ts'

type Command = 'baseline' | 'evidence' | 'all'

type Config = {
  schemaVersion: string
  source: 'tracked' | 'worktree'
  include: string[]
  extensions: string[]
  binaryExtensions: string[]
  excludedDirectories: string[]
  directoryDepth: number
  percentiles: number[]
  classification: { id: string; pattern: string }[]
  git: {
    windows: { id: string; days: number | null }[]
    excludeMerges: boolean
    followRenames: boolean
    identityMap: Record<string, string>
    temporalCoupling: { window: string; minCount: number }
  }
  complexity: { decisionNodeKinds: string[]; logicalOperators: string[] }
  duplication: {
    minLines: number
    minTokens: number
    mode: string
    clusterRatioBase: 'shorter' | 'longer' | 'union'
    clusterThreshold: number
  }
  dependency: { tsConfig: string }
  reading: { limit: number }
  evaluation: EvaluationConfig
}

function readArg(args: readonly string[], name: string): string | null {
  const index = args.indexOf(name)
  return index >= 0 ? (args[index + 1] ?? null) : null
}

function listFiles(
  repoRoot: string,
  source: 'tracked' | 'worktree',
): { path: string; tracked: boolean }[] {
  const tracked = runGit(repoRoot, ['ls-files', '-z']).split('\0').filter(Boolean).map(toPosix)
  if (source === 'tracked') return tracked.map((filePath) => ({ path: filePath, tracked: true }))
  const untracked = runGit(repoRoot, ['ls-files', '--others', '--exclude-standard', '-z'])
    .split('\0')
    .filter(Boolean)
    .map(toPosix)
  return [
    ...tracked.map((filePath) => ({ path: filePath, tracked: true })),
    ...untracked.map((filePath) => ({ path: filePath, tracked: false })),
  ]
}

function pnpmVersion(): string {
  const result = spawnSync('pnpm', ['--version'], { encoding: 'utf8' })
  return result.status === 0 ? result.stdout.trim() : 'unknown'
}

function assertRuntimeConfig(command: string, source: string, config: Config): void {
  if (!['baseline', 'evidence', 'all'].includes(command))
    throw new Error(`unknown command: ${command}`)
  if (source !== 'tracked' && source !== 'worktree') throw new Error(`unknown source: ${source}`)
  if (!['shorter', 'longer', 'union'].includes(config.duplication.clusterRatioBase)) {
    throw new Error(`unknown duplication clusterRatioBase: ${config.duplication.clusterRatioBase}`)
  }
  if (!config.git.windows.some((window) => window.id === config.git.temporalCoupling.window)) {
    throw new Error(`unknown temporal coupling window: ${config.git.temporalCoupling.window}`)
  }
}

export async function run(repoRoot: string, args: readonly string[]): Promise<MetricsReport> {
  const startedAt = Date.now()
  const commandName = args[0] ?? 'all'
  const configPath = path.resolve(repoRoot, readArg(args, '--config') ?? 'code-metrics.config.mjs')
  const config = (await import(pathToFileURL(configPath).href)).default as Config
  const sourceName = readArg(args, '--source') ?? config.source
  assertRuntimeConfig(commandName, sourceName, config)
  const command = commandName as Command
  const source = sourceName as Config['source']
  const progress = createProgress(startedAt)
  progress('配置完成')
  const diagnostics: Diagnostic[] = []
  progress('枚举文件')
  const selected = listFiles(repoRoot, source)
    .filter((file) =>
      config.include.some((prefix) => file.path === prefix || file.path.startsWith(`${prefix}/`)),
    )
    .filter(
      (file) =>
        !file.path.split('/').some((segment) => config.excludedDirectories.includes(segment)),
    )
    .filter(
      (file) =>
        config.extensions.includes(path.posix.extname(file.path).toLowerCase()) ||
        config.binaryExtensions.includes(path.posix.extname(file.path).toLowerCase()),
    )
  progress(`检查文件 0/${selected.length}`)
  const files = selected.map((file, index) => {
    const fact = inspectFile(repoRoot, file, config.classification, config.binaryExtensions)
    const seen = index + 1
    if (seen === selected.length || seen % 500 === 0)
      progress(`检查文件 ${seen}/${selected.length}`)
    return fact
  })
  files.sort((left, right) => left.path.localeCompare(right.path))

  const history =
    command === 'baseline'
      ? { start: null, coverageDays: null, head: null }
      : (progress('Git 历史范围'), historyBounds(repoRoot))
  let duplication: {
    sameClassification: MetricsFacts['duplication']
    crossClassification: MetricsFacts['crossClassificationDuplication']
    statistics: MetricsFacts['duplicationStatistics']
  } = { sameClassification: [], crossClassification: [], statistics: null }
  let temporalCoupling: MetricsReport['derived']['temporalCoupling'] = []
  if (command !== 'baseline') {
    try {
      temporalCoupling = collectGit(repoRoot, files, config.git.windows, {
        excludeMerges: config.git.excludeMerges,
        followRenames: config.git.followRenames,
        identityMap: config.git.identityMap,
        referenceAt: history.head,
        temporalWindow: config.git.temporalCoupling.window,
        temporalMinCount: config.git.temporalCoupling.minCount,
        onProgress: progress,
      })
      diagnostics.push({
        collector: 'git',
        severity: 'info',
        path: null,
        message: `reference=${history.head ?? 'null'} temporalPairs=${temporalCoupling.length}`,
      })
    } catch (error) {
      diagnostics.push({
        collector: 'git',
        severity: 'error',
        path: null,
        message: error instanceof Error ? error.message : 'git failed',
      })
    }
    try {
      const coverage = collectComplexity(repoRoot, files, config.complexity, progress)
      diagnostics.push({
        collector: 'complexity',
        severity: coverage.failed > 0 ? 'warning' : 'info',
        path: null,
        message: `ok=${coverage.ok} failed=${coverage.failed} skipped=${coverage.skipped}`,
      })
      for (const failure of coverage.failures) {
        diagnostics.push({
          collector: 'complexity',
          severity: 'error',
          path: failure.path,
          message: failure.reason,
        })
      }
    } catch (error) {
      diagnostics.push({
        collector: 'complexity',
        severity: 'error',
        path: null,
        message: error instanceof Error ? error.message : 'complexity failed',
      })
    }
    try {
      const collected = collectDuplication(repoRoot, files, config.duplication, progress)
      duplication = collected
      diagnostics.push({
        collector: 'duplication',
        severity: 'info',
        path: null,
        message: `${collected.parameters} same=${collected.sameClassification.length} cross=${collected.crossClassification.length}`,
      })
    } catch (error) {
      diagnostics.push({
        collector: 'duplication',
        severity: 'error',
        path: null,
        message: error instanceof Error ? error.message : 'duplication failed',
      })
    }
  }
  const graph =
    command === 'baseline'
      ? {
          edges: [],
          unresolved: [],
          cycles: [],
          components: [],
          coverage: null,
          externalEdgeCount: null,
        }
      : (() => {
          try {
            const collected = collectDependencies(
              repoRoot,
              files,
              config.dependency.tsConfig,
              progress,
            )
            diagnostics.push({
              collector: 'dependency',
              severity: 'info',
              path: null,
              message: `${collected.parameters} edges=${collected.edges.length} externalEdges=${collected.externalEdgeCount ?? 'null'} unresolved=${collected.unresolved.length} cycles=${collected.cycles.length} coverage=${collected.coverage ?? 'null'}`,
            })
            return collected
          } catch (error) {
            diagnostics.push({
              collector: 'dependency',
              severity: 'error',
              path: null,
              message: error instanceof Error ? error.message : 'dependency failed',
            })
            return {
              edges: [],
              unresolved: [],
              cycles: [],
              components: [],
              coverage: null,
              externalEdgeCount: null,
            }
          }
        })()
  progress('聚合与排名')
  const report: MetricsReport = {
    schemaVersion: config.schemaVersion,
    meta: {
      generatedAt: new Date(startedAt).toISOString(),
      durationMs: Date.now() - startedAt,
      commit: runGit(repoRoot, ['rev-parse', 'HEAD']).trim(),
      dirty: runGit(repoRoot, ['status', '--porcelain']).trim().length > 0,
      historyStart: history.start,
      historyCoverageDays: history.coverageDays,
      node: process.version,
      pnpm: pnpmVersion(),
      platform: `${process.platform} ${process.arch}`,
      configHash: sha256(config),
      command: [command, ...args.slice(1)],
      factsFile: 'facts.json',
      readingLimit: config.reading.limit,
      duplicationCluster: {
        ratioBase: config.duplication.clusterRatioBase,
        threshold: config.duplication.clusterThreshold,
      },
      tools: {
        typescript: dependencyVersion('typescript'),
        jscpd: dependencyVersion('jscpd'),
        dependencyCruiser: dependencyVersion('dependency-cruiser'),
      },
      git: {
        referenceAt: history.head,
        excludeMerges: config.git.excludeMerges,
        followRenames: config.git.followRenames,
        temporalWindow: config.git.temporalCoupling.window,
        temporalMinCount: config.git.temporalCoupling.minCount,
      },
    },
    scope: {
      source,
      include: config.include,
      extensions: config.extensions,
      excludedDirectories: config.excludedDirectories,
      classification: config.classification,
    },
    derived: {
      aggregates: aggregate(files, config.directoryDepth, config.percentiles),
      rankings: [],
      duplicationClusters: clusterDuplication(
        [...duplication.sameClassification, ...duplication.crossClassification],
        {
          clusterRatioBase: config.duplication.clusterRatioBase,
          clusterThreshold: config.duplication.clusterThreshold,
        },
      ),
      duplicationDensity: summarizeDuplicationDensity(files, [
        ...duplication.sameClassification,
        ...duplication.crossClassification,
      ]),
      coupling: workspaceCoupling(graph.edges),
      temporalCoupling,
    },
    evaluation: { enabled: false, formula: null, score: null, matches: [] },
    diagnostics,
  }
  const facts = {
    files,
    duplication: duplication.sameClassification,
    crossClassificationDuplication: duplication.crossClassification,
    duplicationStatistics: duplication.statistics,
    graph: {
      nodes: [...new Set(graph.edges.flatMap((edge) => [edge.from, edge.to]))].sort(),
      edges: graph.edges,
      unresolved: graph.unresolved,
      cycles: graph.cycles,
      components: graph.components,
      coverage: graph.coverage,
      externalEdgeCount: graph.externalEdgeCount,
    },
  }
  report.derived.rankings = rank(facts)
  report.evaluation = evaluate(facts, config.evaluation)
  validateReport(report)
  progress('写入报告')
  const outputDir = path.resolve(repoRoot, readArg(args, '--out') ?? '.artifacts/code-metrics')
  mkdirSync(outputDir, { recursive: true })
  writeFileSync(path.join(outputDir, 'facts.json'), `${JSON.stringify(facts, null, 2)}\n`)
  writeFileSync(path.join(outputDir, 'report.json'), `${JSON.stringify(report, null, 2)}\n`)
  writeFileSync(path.join(outputDir, 'summary.md'), renderMarkdown(report, facts))
  progress('完成')
  return report
}
