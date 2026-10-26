import { existsSync, mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'

import { workspaceOf } from './classify.ts'
import type {
  CouplingFact,
  DependencyComponent,
  DependencyCycle,
  DependencyEdge,
  FileFact,
} from './types.ts'
import {
  commandFailure,
  normalizeReportPath,
  packageScript,
  type Progress,
  roundRatio,
  runPackageBin,
} from './util.ts'

const CODE_EXTENSIONS = new Set(['.ts', '.svelte', '.js', '.mjs'])

type CruiseDependency = {
  resolved?: string
  module: string
  coreModule?: boolean
  couldNotResolve?: boolean
  dependencyTypes?: string[]
}

type CruiseModule = {
  source: string
  dependencies?: CruiseDependency[]
}

type CruiseReport = {
  modules?: CruiseModule[]
}

export function filterDependencies(
  report: CruiseReport,
  files: readonly FileFact[],
  repoRoot: string,
): {
  edges: DependencyEdge[]
  unresolved: { from: string; request: string; reason: string }[]
  cycles: DependencyCycle[]
  components: DependencyComponent[]
  coverage: number | null
  externalEdgeCount: number
} {
  const scope = new Set(
    files.filter((file) => CODE_EXTENSIONS.has(file.extension)).map((file) => file.path),
  )
  const edges: DependencyEdge[] = []
  const unresolved: { from: string; request: string; reason: string }[] = []
  let externalEdgeCount = 0
  const seen = new Set<string>()
  for (const module of report.modules ?? []) {
    const source = normalizeReportPath(repoRoot, module.source)
    if (!scope.has(source)) continue
    seen.add(source)
    for (const dependency of module.dependencies ?? []) {
      if (dependency.coreModule) continue
      if (dependency.couldNotResolve || !dependency.resolved) {
        unresolved.push({ from: source, request: dependency.module, reason: 'unresolved' })
        continue
      }
      const target = normalizeReportPath(repoRoot, dependency.resolved)
      if (!scope.has(target)) {
        externalEdgeCount += 1
        continue
      }
      edges.push({
        from: source,
        to: target,
        kind: (dependency.dependencyTypes ?? ['import']).join(','),
      })
    }
  }
  const cycles = findCycles(edges)
  return {
    edges,
    unresolved,
    cycles,
    components: dependencyComponents(edges),
    coverage: scope.size === 0 ? null : seen.size / scope.size,
    externalEdgeCount,
  }
}

export function instability(afferent: number, efferent: number): number | null {
  return afferent + efferent === 0 ? null : roundRatio(efferent / (afferent + efferent))
}

export function workspaceCoupling(edges: readonly DependencyEdge[]): CouplingFact[] {
  const afferent = new Map<string, Set<string>>()
  const efferent = new Map<string, Set<string>>()
  const workspaces = new Set<string>()
  for (const edge of edges) {
    const from = workspaceOf(edge.from)
    const to = workspaceOf(edge.to)
    if (!from || !to) continue
    workspaces.add(from)
    workspaces.add(to)
    if (from === to) continue
    const outgoing = efferent.get(from) ?? new Set<string>()
    outgoing.add(edge.to)
    efferent.set(from, outgoing)
    const incoming = afferent.get(to) ?? new Set<string>()
    incoming.add(edge.from)
    afferent.set(to, incoming)
  }
  return [...workspaces].sort().map((key) => {
    const incoming = afferent.get(key)?.size ?? 0
    const outgoing = efferent.get(key)?.size ?? 0
    return {
      scope: 'workspace',
      key,
      afferent: incoming,
      efferent: outgoing,
      instability: instability(incoming, outgoing),
    }
  })
}

export function dependencyComponents(edges: readonly DependencyEdge[]): DependencyComponent[] {
  const graph = new Map<string, string[]>()
  for (const edge of edges) {
    graph.set(edge.from, [...(graph.get(edge.from) ?? []), edge.to])
    if (!graph.has(edge.to)) graph.set(edge.to, [])
  }
  let index = 0
  const indices = new Map<string, number>()
  const low = new Map<string, number>()
  const stack: string[] = []
  const onStack = new Set<string>()
  const groups: string[][] = []
  const connect = (node: string) => {
    indices.set(node, index)
    low.set(node, index)
    index += 1
    stack.push(node)
    onStack.add(node)
    for (const next of graph.get(node) ?? []) {
      if (!indices.has(next)) {
        connect(next)
        low.set(node, Math.min(low.get(node) ?? index, low.get(next) ?? index))
      } else if (onStack.has(next)) {
        low.set(node, Math.min(low.get(node) ?? index, indices.get(next) ?? index))
      }
    }
    if (low.get(node) === indices.get(node)) {
      const component: string[] = []
      let current = ''
      do {
        current = stack.pop() ?? ''
        onStack.delete(current)
        component.push(current)
      } while (current !== node)
      groups.push(component.sort())
    }
  }
  for (const node of [...graph.keys()].sort()) {
    if (!indices.has(node)) connect(node)
  }
  const selfLoops = new Set(edges.filter((edge) => edge.from === edge.to).map((edge) => edge.from))
  return groups
    .map((files) => {
      const members = new Set(files)
      let internalEdges = 0
      let incomingEdges = 0
      let outgoingEdges = 0
      for (const edge of edges) {
        const fromInside = members.has(edge.from)
        const toInside = members.has(edge.to)
        if (fromInside && toInside) internalEdges += 1
        else if (toInside) incomingEdges += 1
        else if (fromInside) outgoingEdges += 1
      }
      return {
        files,
        cyclic: files.length > 1 || files.some((file) => selfLoops.has(file)),
        internalEdges,
        incomingEdges,
        outgoingEdges,
      }
    })
    .filter((component) => component.cyclic)
    .sort(
      (left, right) =>
        right.files.length - left.files.length || left.files[0].localeCompare(right.files[0]),
    )
}

/**
 * 从仓库相对路径向上查找最近的 tsconfig.json。
 * 返回 null 表示直到仓库根都没有找到。
 */
export function nearestTsConfigPath(repoRoot: string, filePath: string): string | null {
  let directory = path.posix.dirname(filePath)
  while (directory !== undefined) {
    const candidate = path.posix.join(directory, 'tsconfig.json')
    if (existsSync(path.join(repoRoot, candidate))) return candidate
    if (directory === '.') return null
    const parent = path.posix.dirname(directory)
    if (parent === directory) return null
    directory = parent
  }
  return null
}

/** 取文件所在目录的前两段作为目录根，避免向依赖工具传过宽的目录。 */
export function shallowRoots(files: readonly { path: string }[]): string[] {
  const roots = new Set<string>()
  for (const file of files) {
    const directory = path.posix.dirname(file.path)
    if (directory === '.') continue
    const parts = directory.split('/')
    roots.add(parts.slice(0, Math.min(2, parts.length)).join('/'))
  }
  return [...roots].filter(Boolean).sort()
}

type CruiseGroup = {
  tsConfig: string
  roots: string[]
}

/** 按最近 tsconfig 把代码文件分组；找不到的文件归入回退 tsConfig 组。 */
export function groupFilesByTsConfig(
  repoRoot: string,
  filePaths: readonly string[],
  fallbackTsConfig: string,
): CruiseGroup[] {
  const byTsConfig = new Map<string, string[]>()
  for (const filePath of filePaths) {
    const tsConfig = nearestTsConfigPath(repoRoot, filePath) ?? fallbackTsConfig
    const members = byTsConfig.get(tsConfig) ?? []
    members.push(filePath)
    byTsConfig.set(tsConfig, members)
  }
  const groups: CruiseGroup[] = []
  for (const tsConfig of [...byTsConfig.keys()].sort()) {
    const members = byTsConfig.get(tsConfig) ?? []
    const directory = path.posix.dirname(tsConfig)
    groups.push({
      tsConfig,
      roots:
        directory === '.' || directory === ''
          ? shallowRoots(members.map((m) => ({ path: m })))
          : [directory],
    })
  }
  return groups
}

type CruiseRun = { tsConfig: string; report: CruiseReport }

/**
 * 合并多组 cruise 结果。
 * 一个模块的权威记录来自"该模块自身最近 tsconfig"对应的那组运行，
 * 其余运行中被 follow 到的同名模块仅在尚无记录时作为占位保留。
 */
export function mergeCruiseReports(runs: readonly CruiseRun[], repoRoot: string): CruiseReport {
  const merged = new Map<string, { module: CruiseModule; authoritative: boolean }>()
  for (const run of runs) {
    for (const module of run.report.modules ?? []) {
      const source = normalizeReportPath(repoRoot, module.source)
      const authoritative = (nearestTsConfigPath(repoRoot, source) ?? '') === run.tsConfig
      const existing = merged.get(source)
      if (!existing || (authoritative && !existing.authoritative)) {
        merged.set(source, { module, authoritative })
      }
    }
  }
  const modules = [...merged.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([, value]) => value.module)
  return { modules }
}

export function collectDependencies(
  repoRoot: string,
  files: readonly FileFact[],
  tsConfig: string,
  onProgress?: Progress,
): {
  edges: DependencyEdge[]
  unresolved: { from: string; request: string; reason: string }[]
  cycles: DependencyCycle[]
  components: DependencyComponent[]
  coverage: number | null
  externalEdgeCount: number | null
  parameters: string
} {
  const scopePaths = files
    .filter((file) => CODE_EXTENSIONS.has(file.extension))
    .map((file) => file.path)
  const parameters = `tsConfig=${tsConfig} groups=multi`
  if (scopePaths.length === 0) {
    return {
      edges: [],
      unresolved: [],
      cycles: [],
      components: [],
      coverage: null,
      externalEdgeCount: null,
      parameters,
    }
  }
  const groups = groupFilesByTsConfig(repoRoot, scopePaths, tsConfig)
  onProgress?.(`依赖 0/${groups.length}`)
  const configDir = mkdtempSync(path.join(tmpdir(), 'code-metrics-depcruise-'))
  const runs: CruiseRun[] = []
  groups.forEach((group, index) => {
    onProgress?.(`依赖 ${index + 1}/${groups.length} ${group.tsConfig}`)
    const configPath = path.join(configDir, `dependency-cruiser-${index}.cjs`)
    // tsConfig.fileName 必须传绝对路径：TypeScript 在相对 configFileName 下
    // 解析 include 时会匹配不到任何文件，进而对没有 files/references 的
    // tsconfig 抛出 TS18003（depcruise 会原样透传该文件名）。
    const absoluteTsConfig = path.resolve(repoRoot, group.tsConfig)
    writeFileSync(
      configPath,
      `module.exports = { forbidden: [], options: { doNotFollow: { path: 'node_modules' }, exclude: { path: ['node_modules', '(^|/)dist/', '(^|/)\\\\.svelte-kit/', '(^|/)coverage/'] }, tsConfig: { fileName: ${JSON.stringify(absoluteTsConfig)} }, enhancedResolveOptions: { exportsFields: ['exports'], conditionNames: ['import', 'module', 'node', 'default'] } } }\n`,
    )
    const result = runPackageBin(
      repoRoot,
      packageScript(
        import.meta.url,
        '../node_modules/dependency-cruiser/bin/dependency-cruise.mjs',
      ),
      ['--config', configPath, '--output-type', 'json', '--progress', 'none', ...group.roots],
    )
    let report: CruiseReport
    try {
      report = JSON.parse(result.stdout) as CruiseReport
      if (!report.modules) throw new Error('missing modules')
    } catch {
      throw commandFailure('depcruise', result)
    }
    runs.push({ tsConfig: group.tsConfig, report })
  })
  const filtered = filterDependencies(mergeCruiseReports(runs, repoRoot), files, repoRoot)
  return { ...filtered, parameters }
}

function canonicalCycle(path: readonly string[]): string {
  const body = path.length > 1 && path[0] === path[path.length - 1] ? path.slice(0, -1) : [...path]
  const rotations: string[] = []
  const push = (nodes: readonly string[]) => {
    for (let index = 0; index < nodes.length; index += 1) {
      rotations.push([...nodes.slice(index), ...nodes.slice(0, index)].join('\0'))
    }
  }
  push(body)
  push([...body].reverse())
  rotations.sort()
  return rotations[0] ?? ''
}

function findCycles(edges: readonly DependencyEdge[]): DependencyCycle[] {
  const graph = new Map<string, string[]>()
  for (const edge of edges) graph.set(edge.from, [...(graph.get(edge.from) ?? []), edge.to])
  const visited = new Set<string>()
  const stack: string[] = []
  const found: string[][] = []
  const visit = (node: string) => {
    if (stack.includes(node)) {
      found.push([...stack.slice(stack.indexOf(node)), node])
      return
    }
    if (visited.has(node)) return
    visited.add(node)
    stack.push(node)
    for (const next of graph.get(node) ?? []) visit(next)
    stack.pop()
  }
  for (const node of [...graph.keys()].sort()) visit(node)
  const seen = new Set<string>()
  const cycles: DependencyCycle[] = []
  for (const path of found) {
    const key = canonicalCycle(path)
    if (!key || seen.has(key)) continue
    seen.add(key)
    const body = key.split('\0')
    cycles.push({ path: [...body, body[0]] })
  }
  return cycles.sort((left, right) => left.path.join('\0').localeCompare(right.path.join('\0')))
}
