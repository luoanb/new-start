import { cognitiveOfFile, couplingByFile, cyclomaticOf, duplicationByFile } from './read-metrics.ts'
import type { Aggregate, DuplicationClusterMember, DuplicationMatch, FileFact, MetricsFacts, MetricsReport } from './types.ts'

const DEFINITIONS = [
  ['文件数', '分组内的文件个数，包含没有行数的二进制文件。'],
  [
    '总行数',
    '按物理行计数。文件末尾的换行不额外增加一行；空行计入总行数。二进制文件没有行数，规模表中的总行数只累加有行数的文件。',
  ],
  [
    'P90',
    '把分组内有行数的文件按总行数从小到大排列，取位次 ceil(0.9 × 文件数) 的值。没有有行数的文件时为 null。',
  ],
  ['分类', '按配置规则从上到下匹配，第一个命中的类别是主分类。一个文件可以同时带有多个标签。'],
  [
    '最大圈复杂度',
    '文件内单个函数的最大 McCabe 圈复杂度。圈复杂度 = 1 + 决策节点数。决策节点包括 if、for、for-in、for-of、while、do、catch、三元表达式、除 default 以外的 switch case，以及 &&、||、??。只计算 .ts 和 .svelte；.svelte 只解析 script，模板分支不计入。其他文件为 null。',
  ],
  [
    '最大复杂度函数',
    '达到该文件最大圈复杂度的函数，格式为「函数名 (起始行-结束行)」。并列取最大值时用顿号连接；没有函数记录时为 null。仅最大圈复杂度排名表展示该列。',
  ],
  [
    '圈复杂度总和',
    '文件内每个函数的圈复杂度相加。外层函数的决策节点包含其内部函数中的决策节点，因此内部决策会同时进入外层函数和内部函数。',
  ],
  ['90 天修改提交数', '从采集时刻向前 90 天，排除 merge 提交后，该文件出现过的不同提交数。'],
  [
    '90 天 churn',
    '上述提交中该文件的新增行数加删除行数。Git 对二进制文件给出的行数是 "-"，不计入。',
  ],
  [
    '同分类重复行数',
    '该文件参与的、两端主分类相同的重复片段行数之和。同一个片段会分别计入两端文件。',
  ],
  [
    '重复片段',
    'jscpd 找出的一对相似代码。两端都必须是本次采集到的、可读的非二进制文件。两端主分类相同的列入同分类，不同的列入跨分类。行号是片段在各自文件中的起止行。',
  ],
  [
    '重复簇',
    '把重复片段按连通规则聚成的等价类：每条 pair 的两端视为相连；同一文件内两条区间的交叠率达到阈值时也视为相连。交叠率 = 交叠行数 ÷ 分母，分母口径与阈值见采集状态。簇在 pair 全集上计算，不区分同分类与跨分类。重复簇表按重复体量 = 重复次数 × 最大区间行数降序排列。',
  ],
  [
    '重复次数',
    '重复簇内的区间实例数。同一文件内的自我重复计为不同实例，因此重复次数可以大于文件数。',
  ],
  [
    '循环依赖',
    '依赖图中的有向环。图只保留两端都在本次 .ts、.svelte、.js、.mjs 文件集合内的 import。路径末尾重复起点，表示回到环的入口。',
  ],
  [
    '未解析依赖',
    'dependency-cruiser 读到了 import，但没有定位到具体文件。次数是该请求模块出现的 import 次数。它不表示运行时一定缺少这个模块；路径别名或工具无法识别的内部模块也会记在这里。',
  ],
  [
    '依赖图覆盖率',
    '本次代码文件中，被 dependency-cruiser 当作模块输出的文件所占比例。覆盖率为 1 只表示文件都被扫描到，不表示其中的 import 都已解析。',
  ],
  [
    '范围外已解析边',
    'import 已经定位到文件，但目标不在本次代码文件集合内的边数。这些边不进入循环依赖。',
  ],
  [
    '认知复杂度',
    'Sonar 认知复杂度子集。if、for、while、do、switch、catch、三元表达式记 1 + 当前嵌套层级；else、else if 和每个 &&、|| 记 1。嵌套函数从 0 单独计算。同名直接递归记 1。文件值 = 模块顶层 + 各函数。',
  ],
  ['唯一重复行', '同一文件内同分类重复区间按闭区间取并集后的行数。相邻或重叠区间只计一次。'],
  [
    '重复密度',
    '唯一重复行 / 有行数文件的物理总行数 × 100。仓库值先分别求和再相除。jscpd 原生百分比另存在 JSON，不与此口径混用。',
  ],
  ['强连通分量', '依赖图中任意两点都可互相到达的最大节点集合。只列出包含环或自环的分量。'],
  [
    '依赖稳定度',
    '唯一出度 /（唯一入度 + 唯一出度）。分母为 0 时为 null。workspace 只统计跨 workspace 的唯一文件。',
  ],
  [
    '时间耦合',
    '同一 Git 窗口内，两个文件出现在同一提交中的次数。条件概率 = 共同次数 / 该文件提交数；Jaccard = 共同次数 / 两个文件提交数的并集。',
  ],
  [
    '评价分数',
    '只在启用评价层后按用户公式计算。rank 归一使用该指标降序名次 / 样本数。缺失策略为 skip 时，该项不进入权重；为 zero 时按 0 进入。文件分数是已使用权重的加权平均。仓库级 score 固定为 null。',
  ],
]

const RANKING_TABLES = [
  { metric: 'totalLines', title: '总行数' },
  { metric: 'complexityMax', title: '最大圈复杂度' },
  { metric: 'complexitySum', title: '圈复杂度总和' },
  { metric: 'cognitiveMax', title: '认知复杂度' },
  { metric: 'gitCommits90d', title: '90 天修改提交数' },
  { metric: 'gitChurn90d', title: '90 天 churn' },
  { metric: 'duplicatedLines', title: '同分类重复行数' },
  { metric: 'uniqueDuplicatedLines', title: '唯一重复行' },
  { metric: 'duplicatedLineDensity', title: '重复密度' },
  { metric: 'instability', title: '文件依赖稳定度' },
] as const

function cell(value: string | number | null | undefined): string {
  if (value === null || value === undefined || value === '') return 'null'
  return String(value).replaceAll('|', '\\|')
}

function take<T>(items: readonly T[], limit: number): T[] {
  return items.slice(0, limit)
}

function note(shown: number, total: number): string {
  return shown < total
    ? `展示前 ${shown} 条，完整记录共 ${total} 条，其余在 JSON。`
    : `共 ${total} 条。`
}

function scaleTable(items: readonly Aggregate[]): string[] {
  const lines = ['| 分组 | 文件数 | 总行数 | P90 |', '| --- | ---: | ---: | ---: |']
  for (const item of items) {
    lines.push(
      `| ${cell(item.key)} | ${item.sampleCount} | ${cell(item.sumTotalLines)} | ${cell(item.percentiles.p90)} |`,
    )
  }
  return lines
}

function maxComplexityFunctions(file: FileFact | undefined): string | null {
  const complexity = file?.complexity
  const max = file ? cyclomaticOf(file)?.max : null
  if (!complexity || max === null || max === undefined || complexity.functions.length === 0) return null
  const top = complexity.functions.filter((fn) => fn.cyclomatic === max)
  if (top.length === 0) return null
  return top.map((fn) => `${fn.name} (${fn.startLine}-${fn.endLine})`).join('、')
}

function fileColumns(
  file: FileFact | undefined,
  includeMaxFunction: boolean,
  duplication: ReturnType<typeof duplicationByFile>,
): string {
  const cyclomatic = file ? cyclomaticOf(file) : null
  return [
    cell(file?.classification.primary ?? null),
    cell(file?.size.totalLines ?? null),
    cell(cyclomatic?.max ?? null),
    ...(includeMaxFunction ? [cell(maxComplexityFunctions(file))] : []),
    cell(cyclomatic?.sum ?? null),
    cell(file?.git['90d']?.commits ?? null),
    cell(file?.git['90d']?.churn ?? null),
    cell(file ? (duplication.get(file.path)?.duplicatedLines ?? 0) : null),
  ].join(' | ')
}

function metricColumns(
  metric: string,
  file: FileFact | undefined,
  duplication: ReturnType<typeof duplicationByFile>,
  coupling: ReturnType<typeof couplingByFile>,
): string | null {
  const cyclomatic = file ? cyclomaticOf(file) : null
  const cognitive = file ? cognitiveOfFile(file) : null
  const duplicate = file ? duplication.get(file.path) : undefined
  const linked = file ? coupling.get(file.path) : undefined
  if (metric === 'cognitiveMax') {
    return [
      cell(file?.classification.primary ?? null),
      cell(cognitive?.max ?? null),
      cell(cognitive?.sum ?? null),
      cell(cyclomatic?.max ?? null),
    ].join(' | ')
  }
  if (metric === 'uniqueDuplicatedLines' || metric === 'duplicatedLineDensity') {
    return [
      cell(file?.classification.primary ?? null),
      cell(duplicate?.uniqueDuplicatedLines ?? 0),
      cell(duplicate?.duplicatedLineDensity ?? null),
      cell(duplicate?.duplicatedLines ?? 0),
    ].join(' | ')
  }
  if (metric === 'instability') {
    return [
      cell(file?.classification.primary ?? null),
      cell(linked?.afferent ?? 0),
      cell(linked?.efferent ?? 0),
      cell(linked?.instability ?? null),
    ].join(' | ')
  }
  return null
}

function duplicationTable(matches: readonly DuplicationMatch[]): string[] {
  const lines = ['| 重复行数 | 文件 A | 行号 | 文件 B | 行号 |', '| ---: | --- | --- | --- | --- |']
  for (const match of matches) {
    lines.push(
      `| ${match.lines} | ${cell(match.firstFile)} | ${match.firstStart}-${match.firstEnd} | ${cell(match.secondFile)} | ${match.secondStart}-${match.secondEnd} |`,
    )
  }
  return lines
}

function memberCell(members: readonly DuplicationClusterMember[], shown: number): string {
  const visible = take(members, shown)
  const suffix = members.length > visible.length ? ` 等 ${members.length} 个区间` : ''
  return `${visible.map((member) => `${member.file}:${member.start}-${member.end}`).join('、')}${suffix}`
}

export function renderMarkdown(report: MetricsReport, facts: MetricsFacts): string {
  const limit = report.meta.readingLimit ?? 30
  const files = new Map(facts.files.map((file) => [file.path, file]))
  const duplication = duplicationByFile(
    [...facts.duplication, ...facts.crossClassificationDuplication],
    facts.files,
  )
  const fileCoupling = couplingByFile(facts)
  const lines = [
    '# 代码度量报告',
    '',
    '本报告只摘录 JSON 中的事实。每个排名表只按一个指标排序；展示条数不是质量阈值。',
    '',
    '## 指标定义',
    '',
    ...DEFINITIONS.map(([name, definition]) => `- **${name}**：${definition}`),
    '',
    '## 采集状态',
    '',
    `- commit: \`${cell(report.meta.commit)}\``,
    `- dirty: \`${report.meta.dirty}\``,
    `- source: \`${report.scope.source}\``,
    `- Git 历史起点: \`${cell(report.meta.historyStart)}\``,
    `- Git 历史覆盖天数: \`${cell(report.meta.historyCoverageDays)}\``,
    `- 重复簇归并: 口径 \`${cell(report.meta.duplicationCluster.ratioBase)}\`，阈值 \`${report.meta.duplicationCluster.threshold}\``,
    `- 工具版本: typescript \`${cell(report.meta.tools?.typescript)}\`，jscpd \`${cell(report.meta.tools?.jscpd)}\`，dependency-cruiser \`${cell(report.meta.tools?.dependencyCruiser)}\``,
    `- Git 参考时间: \`${cell(report.meta.git?.referenceAt)}\``,
    `- 唯一重复密度: \`${cell(report.derived.duplicationDensity?.density)}\``,
    `- 展示条数: ${limit}`,
    '',
  ]
  if (report.diagnostics.length === 0) lines.push('无采集诊断。', '')
  for (const diagnostic of report.diagnostics) {
    lines.push(`- [${diagnostic.severity}] ${diagnostic.collector}: ${diagnostic.message}`)
  }
  lines.push('', '## 规模', '')
  for (const dimension of ['repository', 'classification', 'workspace'] as const) {
    const items = report.derived.aggregates.filter((item) => item.dimension === dimension)
    lines.push(`### ${dimension}`, '', ...scaleTable(items), '')
  }

  lines.push('## 分指标排名', '')
  for (const table of RANKING_TABLES) {
    const ranked = report.derived.rankings.filter((item) => item.metric === table.metric)
    const shown = take(ranked, limit)
    const metricHeader =
      table.metric === 'cognitiveMax'
        ? {
            header: '| 排名 | 文件 | 分类 | 认知复杂度 | 认知复杂度总和 | 最大圈复杂度 |',
            separator: '| ---: | --- | --- | ---: | ---: | ---: |',
          }
        : table.metric === 'uniqueDuplicatedLines' || table.metric === 'duplicatedLineDensity'
          ? {
              header: '| 排名 | 文件 | 分类 | 唯一重复行 | 重复密度 | 同分类重复行 |',
              separator: '| ---: | --- | --- | ---: | ---: | ---: |',
            }
          : table.metric === 'instability'
            ? {
                header: '| 排名 | 文件 | 分类 | 入度 | 出度 | 稳定度 |',
                separator: '| ---: | --- | --- | ---: | ---: | ---: |',
              }
            : null
    const includeMaxFunction = table.metric === 'complexityMax'
    const rankingHeader = metricHeader
      ? metricHeader.header
      : includeMaxFunction
        ? '| 排名 | 文件 | 分类 | 总行数 | 最大圈复杂度 | 最大复杂度函数 | 圈复杂度总和 | 90天提交 | 90天churn | 同分类重复行 |'
        : '| 排名 | 文件 | 分类 | 总行数 | 最大圈复杂度 | 圈复杂度总和 | 90天提交 | 90天churn | 同分类重复行 |'
    const rankingSeparator = metricHeader
      ? metricHeader.separator
      : includeMaxFunction
        ? '| ---: | --- | --- | ---: | ---: | --- | ---: | ---: | ---: | ---: |'
        : '| ---: | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |'
    lines.push(
      `### ${table.title}`,
      '',
      note(shown.length, ranked.length),
      '',
      rankingHeader,
      rankingSeparator,
    )
    for (const item of shown) {
      const file = files.get(item.path)
      const columns =
        metricColumns(table.metric, file, duplication, fileCoupling) ??
        fileColumns(file, includeMaxFunction, duplication)
      lines.push(`| ${cell(item.rank)} | ${cell(item.path)} | ${columns} |`)
    }
    lines.push('')
  }

  lines.push('## 循环依赖', '', `共 ${facts.graph.cycles.length} 个。`, '')
  facts.graph.cycles.forEach((cycle, index) => {
    lines.push(`${index + 1}. \`${cycle.path.join(' -> ')}\``)
  })
  const components = facts.graph.components ?? []
  const shownComponents = take(components, limit)
  lines.push(
    '',
    '## 强连通分量',
    '',
    note(shownComponents.length, components.length),
    '',
    '| 文件数 | 内部边 | 入口边 | 出口边 | 文件 |',
    '| ---: | ---: | ---: | ---: | --- |',
  )
  for (const component of shownComponents) {
    const filesInComponent = take(component.files, 8)
    const suffix = component.files.length > filesInComponent.length ? ' 等' : ''
    lines.push(
      `| ${component.files.length} | ${component.internalEdges} | ${component.incomingEdges} | ${component.outgoingEdges} | ${cell(`${filesInComponent.join('、')}${suffix}`)} |`,
    )
  }
  lines.push('')
  const coupling = report.derived.coupling ?? []
  const shownCoupling = take(coupling, limit)
  lines.push(
    '## 依赖稳定度',
    '',
    note(shownCoupling.length, coupling.length),
    '',
    '| workspace | 入度 | 出度 | 稳定度 |',
    '| --- | ---: | ---: | ---: |',
  )
  for (const item of shownCoupling) {
    lines.push(
      `| ${cell(item.key)} | ${item.afferent} | ${item.efferent} | ${cell(item.instability)} |`,
    )
  }
  lines.push('')

  const duplicationGroups = [
    { title: '同分类重复片段', matches: facts.duplication },
    { title: '跨分类重复片段', matches: facts.crossClassificationDuplication },
  ]
  lines.push('## 重复片段', '')
  for (const group of duplicationGroups) {
    const sorted = [...group.matches].sort(
      (left, right) =>
        right.lines - left.lines ||
        left.firstFile.localeCompare(right.firstFile) ||
        left.secondFile.localeCompare(right.secondFile),
    )
    const shown = take(sorted, limit)
    lines.push(
      `### ${group.title}`,
      '',
      note(shown.length, sorted.length),
      '',
      ...duplicationTable(shown),
      '',
    )
  }

  const clusters = report.derived.duplicationClusters
  const shownClusters = take(clusters, limit)
  lines.push('## 重复簇', '', note(shownClusters.length, clusters.length), '')
  if (clusters.length > 0) {
    lines.push(
      '| 重复次数 | 文件数 | 最大区间行数 | 参与片段数 | 成员区间 |',
      '| ---: | ---: | ---: | ---: | --- |',
    )
    for (const cluster of shownClusters) {
      lines.push(
        `| ${cluster.instances} | ${cluster.fileCount} | ${cluster.maxLines} | ${cluster.pairCount} | ${cell(memberCell(cluster.members, 8))} |`,
      )
    }
    lines.push('')
  }

  const unresolved = new Map<string, number>()
  for (const item of facts.graph.unresolved)
    unresolved.set(item.request, (unresolved.get(item.request) ?? 0) + 1)
  const unresolvedRows = [...unresolved.entries()].sort(
    (left, right) => right[1] - left[1] || left[0].localeCompare(right[0]),
  )
  const shownUnresolved = take(unresolvedRows, limit)
  lines.push(
    '## 未解析依赖',
    '',
    `原始记录 ${facts.graph.unresolved.length} 条，汇总为 ${unresolvedRows.length} 个请求模块。${note(shownUnresolved.length, unresolvedRows.length)}`,
    '',
    '| 请求模块 | 次数 |',
    '| --- | ---: |',
  )
  for (const [request, count] of shownUnresolved) lines.push(`| ${cell(request)} | ${count} |`)
  lines.push('')
  const temporal = report.derived.temporalCoupling ?? []
  const shownTemporal = take(temporal, limit)
  lines.push(
    '## 时间耦合',
    '',
    note(shownTemporal.length, temporal.length),
    '',
    '| 共同提交 | Jaccard | 文件 A | 文件 B |',
    '| ---: | ---: | --- | --- |',
  )
  for (const pair of shownTemporal) {
    lines.push(
      `| ${pair.cochangeCount} | ${cell(pair.jaccard)} | ${cell(pair.firstFile)} | ${cell(pair.secondFile)} |`,
    )
  }
  lines.push('')
  lines.push('## 评价', '')
  if (!report.evaluation.enabled) {
    lines.push('评价层未启用。', '')
  } else {
    const matches = [...report.evaluation.matches].sort(
      (left, right) =>
        (right.score ?? -1) - (left.score ?? -1) || left.path.localeCompare(right.path),
    )
    const shownMatches = take(matches, limit)
    lines.push('文件分数来自报告 JSON，仓库级 score 为 null。', '', '```json')
    lines.push(JSON.stringify(report.evaluation.formula, null, 2))
    lines.push('```', '', note(shownMatches.length, matches.length), '')
    lines.push('| 文件 | 分数 |', '| --- | ---: |')
    for (const match of shownMatches) lines.push(`| ${cell(match.path)} | ${cell(match.score)} |`)
    lines.push('')
  }
  return `${lines.join('\n')}\n`
}
