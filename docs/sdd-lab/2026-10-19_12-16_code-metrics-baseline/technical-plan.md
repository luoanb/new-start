# Technical Plan / 技术方案: 客观代码度量基建

## Requirement Baseline / 需求基线

- 对应需求文档：`requirements.md`
- 需求确认状态：已确认范围与核心原则；执行尚未批准。
- 本方案覆盖范围：可信静态基线、Git/复杂度/重复/循环依赖事实、可选用户评价层。
- 与其他技术方案的关系：无。

## Current Project Facts / 当前项目事实

- 已读取文件/模块：
  - `package.json`
  - `pnpm-workspace.yaml`
  - `scripts/count-lines.mjs`
  - `docs/source-loc-report.md`
  - `docs/micro_specs/2026-10-19_11-20_source-loc-counter.md`
  - `eslint.config.js`
  - `tsconfig.json`
  - `apps/pixmax/vitest.config.ts`
  - `.gitlab-ci.yml`
- 当前实现事实：
  - 仓库是 pnpm monorepo，workspace 范围为 `apps/*`、`packages/*`、`demos/*`。
  - 根环境声明 Node `>=22.10.0`、pnpm `11.24.0`。
  - 当前 `scripts/count-lines.mjs` 直接遍历工作区，默认扫描 `apps`、`packages`，输出 Markdown，不输出统一 JSON。
  - 当前行数脚本仅提供文件级总行数和非空行数，默认排除测试，缺少分类、目录聚合、分位数和可复现元数据。
  - `docs/source-loc-report.md` 是脚本直接生成的 1500 行以上明细，不适合作为机器真相源。
  - 根目录已有 ESLint 9、TypeScript、Svelte parser 和 Vitest 基础，但没有根级质量/度量聚合命令。
  - 仓库没有现成的 jscpd、dependency-cruiser、复杂度采集或统一报告 Schema。
- Git 历史事实：
  - 当前仓库不是 shallow clone。
  - 历史起点为 2026-01-15，当前不足完整 365 天。
  - 合并提交占比较高，修改次数和 churn 需要固定排除 merge。
  - 作者存在多个名称/邮箱表示，身份归一需要显式映射并保留原始值。
- 采集边界事实：
  - 主源码类型包括 `.ts`、`.svelte`、`.js`、`.mjs`、`.css`，另有 HTML、JSON、Markdown 和二进制资源。
  - 测试主要使用 `*.test.ts`、`*.svelte.test.ts` 和 `**/test/**`。
  - `apps/pixmax/src/locales/**`、`apps/pixmax/src/components/icons/**`、`apps/pixmax/src/services/request/interface.ts` 会显著影响聚合结果，需要单独分类但不从事实中删除。
  - `apps/bridge` 和 `apps/www` 位于 `apps` 下，但不等同于常规 workspace 包。

## Open Questions / 开放问题

- 无。热点方案已由用户选择为“可配置公式，默认不设权重和阈值”。

## Solution Options / 方案候选

### Option A / 方案 A：统一采集内核与分层报告

- 推荐：是。
- 方案摘要：以一个 Node CLI 编排独立采集器，先写事实 JSON，再执行确定性派生，最后按需执行用户评价并生成摘要。
- 涉及模块：根配置、`packages/code-metrics/src/cli.ts`、`packages/code-metrics/src/**`、测试 fixture、根 `package.json`。
- 优点：
  - 原始事实、派生值和用户评价在结构上隔离。
  - 所有 Markdown 值均可追溯到 JSON 字段。
  - 可以独立验证各采集器的覆盖率和失败原因。
  - 保留当前 Node 脚本形态，适合本仓库环境。
- 缺点：
  - 需要定义并维护内部 Schema、分类器和聚合器。
  - 第二阶段需要增加专业工具依赖。
- 风险：不同解析器对 Svelte/TypeScript 的支持范围不同，必须显式报告覆盖率。

### Option B / 方案 B：直接拼接各工具 Markdown/HTML 报告

- 推荐：否。
- 方案摘要：分别运行 LOC、Git、重复和依赖工具，保留各自原生报告。
- 涉及模块：根命令和各第三方工具配置。
- 优点：初始编排成本较低。
- 缺点：
  - 字段、路径和分类口径不统一。
  - 难以证明综合报告中的值来自何处。
  - 难以实现统一分位数、事实向量和可配置评价层。
- 风险：工具默认阈值和措辞容易把隐含判断带入最终报告。

## Decision / 方案决策

- Selected / 选定方案：Option A，统一采集内核与分层报告。
- Why / 选择原因：满足“忠实于数据”的可追溯要求，并支持用户选择的可配置评价方式。
- Decision Owner / 决策人：用户。
- Decision Time / 决策时间：2026-10-19 12:14。
- 2026-10-19 12:28 补充：实现落在独立包 `packages/code-metrics`（`@physalis/code-metrics`）。仓库分类规则保留在根目录 `code-metrics.config.mjs`。CLI 通过 Git 根目录定位被测仓库，不依赖命令启动目录。
- Open Questions 状态：全部关闭。
- API 说明：本次没有业务对外 API；会新增内部 JSON 报告契约。

## API Design / API 设计

### Contract Scope / 契约范围

- 变更类型：新增内部工具契约。
- 消费方：Markdown 渲染器、后续 CI/趋势工具、人工分析。
- 真相源文件：`packages/code-metrics/src/schema.ts` 及对应 JSON Schema。

### MetricsReport

- `schemaVersion: string`：报告契约版本。
- `meta`：commit、dirty、采集时间、环境版本、配置哈希、命令参数、历史覆盖区间。
- `scope`：tracked/worktree、包含路径、分类规则、扩展名和排除项。
- `facts.files`：每个文件的物理事实和各采集器结果。
- `facts.graph`：依赖节点、边、未解析项和环。
- `derived.aggregates`：按明确维度聚合的统计值。
- `derived.rankings`：各单一指标的稳定排序及分位位置。
- `evaluation`：可选公式、参数、缺失值策略、分数和规则命中项；未配置时为禁用状态。
- `diagnostics`：采集器覆盖率、警告、错误及原因。

### FileFact

- `path: string`：仓库相对路径，统一使用 `/`。
- `classification`：单一主分类和多个标签。
- `size`：字节数、物理总行数、非空行数。
- `git`：按窗口记录修改提交数、added、deleted、churn、最后修改时间、raw/canonical contributor。
- `complexity`：函数数量、函数级明细、sum/max/分布和模板分支数量。
- `duplication`：重复片段引用及分类内重复行事实。
- `dependency`：入边、出边、未解析依赖和所属环。
- 不适用或失败字段使用 `null`，并在 `diagnostics` 提供原因，不使用零代替。

### Compatibility Notes / 兼容说明

- `scripts/count-lines.mjs` 在第一阶段替代能力通过验收后删除。
- 不兼容旧脚本参数，不提供转发或兼容层。
- `docs/source-loc-report.md` 不作为新契约输入；是否保留历史文件不影响采集。
- Schema 变更必须提升 `schemaVersion`。

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：
  - 用户明确批准本技术方案。
  - 当前未提交改动受到保护，不覆盖用户工作。
  - 再次确认 Node、pnpm、Git 和 lockfile 状态。
- 若执行前需求、范围或指标口径变化：先更新 `requirements.md` 和本文件，再等待重新批准。

### Step 1. 建立第一阶段采集内核

#### 文件：`code-metrics.config.mjs`

- 改动类型：新增。
- 改动内容：声明输入范围、分类优先级、扩展名、排除项、目录聚合层级和分位数算法。
- 设计约束：
  - 规则顺序必须稳定并写入配置哈希。
  - 分类采用一个主分类加多个标签。
  - 不包含评价阈值和默认热点权重。
- 验收点：配置可序列化，报告完整记录生效配置。

#### 文件：`packages/code-metrics/src/cli.ts`

- 改动类型：新增。
- 改动内容：提供 `baseline`、`evidence`、`all` CLI 入口并编排采集。
- 设计约束：
  - 默认文件集合为 `tracked`，通过 `git ls-files` 获取。
  - `worktree` 必须显式指定并记录 dirty/untracked。
  - 输出路径可配置，默认 `.artifacts/code-metrics/`。
- 验收点：命令退出码能够区分成功、部分采集失败和配置错误。

#### 文件：`packages/code-metrics/src/collect-files.ts`

- 改动类型：新增。
- 改动内容：采集路径、字节、总行数、非空行数、文件状态及分类。
- 设计约束：
  - 不将非空行命名为 SLOC。
  - CRLF/LF、尾随换行、空文件和不可读文件均有固定规则。
  - 二进制不计算行数，但保留文件事实。
- 验收点：fixture 对账通过，Windows 输出路径稳定。

#### 文件：`packages/code-metrics/src/aggregate.ts`

- 改动类型：新增。
- 改动内容：按 repository、分类、workspace、目录和扩展名聚合。
- 设计约束：
  - 输出 count/sum/min/max/mean。
  - P50/P75/P90/P95/P99 使用最近秩法，并记录样本数。
- 验收点：边界样本和空集合结果符合指标词典。

#### 文件：`packages/code-metrics/src/schema.ts`

- 改动类型：新增。
- 改动内容：定义报告类型、运行时校验和 JSON Schema。
- 设计约束：`facts`、`derived`、`evaluation`、`diagnostics` 独立。
- 验收点：合法报告通过，缺字段/错类型报告被拒绝。

### Step 2. 建立统一输出

#### 文件：`packages/code-metrics/src/render-markdown.ts`

- 改动类型：新增。
- 改动内容：仅消费报告 JSON 生成 `summary.md`。阅读报告包含指标定义、采集状态、仓库/分类/workspace 规模、按单一指标排序的前 N 名、全部循环依赖、重复片段前 N 名，以及按请求模块汇总的未解析依赖前 N 名。
- 设计约束：不得重新扫描文件。N 来自配置 `reading.limit`，只截断展示，不是质量阈值。完整排名和原始记录留在 JSON。
- 验收点：Markdown 中的数值都能在同一份报告 JSON 中找到；表格排序指标与标题一致。

#### 文件：`package.json`

- 改动类型：修改。
- 改动内容：增加 `metrics:baseline`、`metrics:evidence`、`metrics:all`。
- 设计约束：命令使用根声明的 pnpm 与 Node 环境。
- 验收点：本地从仓库根目录可重复执行。

#### 文件：`scripts/count-lines.mjs`

- 改动类型：删除。
- 改动内容：新入口通过第一阶段验收后移除旧脚本。
- 设计约束：不保留 CLI 兼容层。
- 验收点：仓库内不存在对该脚本的有效引用。

### Step 3. 接入 Git 事实

#### 文件：`packages/code-metrics/src/collect-git.ts`

- 改动类型：新增。
- 改动内容：解析无 merge 的 Git 历史，输出 90d、180d 和 full-history 文件事实。
- 设计约束：
  - `churn = added + deleted`。
  - 修改次数按唯一 commit 计。
  - 报告首个提交日期和实际覆盖天数。
  - 重命名策略固定并写入元数据。
  - raw identity 永远保留；canonical identity 只读取 `.mailmap` 或显式配置。
- 验收点：抽样结果与 Git 原生命令一致，二进制和重命名有明确状态。

### Step 4. 接入复杂度、重复和依赖事实

#### 文件：`packages/code-metrics/src/collect-complexity.ts`

- 改动类型：新增。
- 改动内容：使用 TypeScript/Svelte 解析器输出函数级 McCabe 圈复杂度。
- 设计约束：
  - 公式为 `1 + 配置中声明的决策节点数`。
  - Svelte 模板分支单独输出，不与脚本复杂度混加。
  - 不设置“过高”阈值。
- 验收点：已知 fixture 的每个决策节点可逐项对账。

#### 文件：`packages/code-metrics/src/collect-duplication.ts`

- 改动类型：新增。
- 改动内容：消费 jscpd JSON，转换为统一事实。
- 设计约束：向 jscpd 传入目录根，不传逐文件参数。结果归一化为仓库相对路径后，两端都在本次文件集合内才保留；同一主分类的片段进入文件事实，跨分类片段单独列出。参数包含 min-lines、min-tokens、mode、max-lines、max-size。
- 验收点：报告可定位每个重复片段的两个端点。

#### 文件：`packages/code-metrics/src/collect-dependencies.ts`

- 改动类型：新增。
- 改动内容：消费 dependency-cruiser JSON，生成节点、边、未解析项、强连通分量和完整环路径。向 dependency-cruiser 传入目录根，并用无 forbidden 规则的配置避免把环本身当成命令失败。只保留两端都在本次代码文件集合内的边；集合外的已解析边只记数量。
- 设计约束：先输出图覆盖率，不静默忽略 Svelte 或 alias 解析失败。
- 验收点：已知环和未解析依赖 fixture 均能复现。

##### 执行中发现与方案修正（2026-10-26）：依赖图大面积漏边

- 现象（当前基线 commit `82c3da52`）：
  - `unresolved=3739`（228 个请求模块）；内部边 `2678` 条中 apps→packages 的边为 **0 条**，跨包依赖图完全缺失；循环依赖结论只建立在残缺图上。
- 根因（三个，均已用对照探针实验证实）：
  1. `dependency.tsConfig` 指向根 `tsconfig.json`，其没有 `paths`；`@/*` 与 `@physalis/*` 别名实际定义在各 app 的 tsconfig（如 `apps/pixmax/tsconfig.json`）中。导致约 1900 条 `@/xxx` 与约 210 条 `@physalis/*` 内部边丢失。dependency-cruiser 17 的 `tsConfig.fileName` 类型为单个 string，不支持多 tsconfig 合并（数组会被拼成字符串并报 TS5083）。
  2. dependency-cruiser 17 默认 `exportsFields: []`（源码 `normalize.mjs`，为兼容 enhanced-resolve 4 的历史行为），即忽略包的 `package.json` `exports` 字段。导致仅靠 exports 暴露的解析全部失败：npm 包（`uuid` 等无 `main` 的包）、`@physalis/infra/ioc` 等子路径、`svelte/internal/client` 等 Svelte 5 编译产物注入的内部模块（后者在开启 exports 解析后变为"范围外已解析边"，噪音自然消解）。
  3. `?raw`/`?url` 是 Vite 专有查询后缀，通用 resolver 不识别（约 5 条）。
- 判断标准（用户确认）：未解析依赖只有当其造成"项目源码之间的边丢失"时才需要修；npm 外部包解析与否不进入内部图，不重要。
- 修复方案（仅改工具侧，不改任何项目 tsconfig / 包定义）：
  1. `collect-dependencies.ts`：对本次代码文件按"向上最近的 tsconfig.json"分组，每组用各自 tsconfig 运行一次 dependency-cruiser，按 `source` 去重合并 modules；无 tsconfig 的文件归入配置回退组。
  2. 生成 depcruise 配置时追加 `options.enhancedResolveOptions.exportsFields: ['exports']` 与 `conditionNames: ['import','module','node','default']`。
  3. `filterDependencies` 在判定前剥离 `?raw`/`?url` 等查询后缀。
- 修复结果与最终残余见下方"执行结果""最终残余噪音"。
- 验证证据（探针对照实验，commit `82c3da52` 工作区）：
  - 现行配置复现：`uuid`、`@/constants` 解析失败；去掉 `doNotFollow` 无效；`externalModuleResolutionStrategy` 默认已为 `node_modules`，追加无效。
  - 改用 `apps/pixmax/tsconfig.json` + exportsFields：从 `main.ts` follow 全闭包 4113 条依赖全部解析，仅剩 2 条 `workflow.css`；`@physalis/infra/ioc` → `packages/infra/src/ioc/index.ts`，全部 `@physalis/*` 主入口 → `packages/*/src/index.ts`。
  - 改用 `apps/mobile/tsconfig.json`：`@/stores/*` 全部解析到 `apps/mobile/src/stores/*`。
- 执行结果（2026-10-26 已实现并通过全量验证）：
  - 实现：`collect-dependencies.ts` 以 `nearestTsConfigPath` / `groupFilesByTsConfig` / `shallowRoots` / `mergeCruiseReports` 支撑"按最近 tsconfig 分组运行、权威记录覆盖占位"的合并策略；新增 `collect-dependencies.test.ts`（6 例，覆盖分组与权威合并）。
  - 执行中新发现并修复：depcruise 把 `tsConfig.fileName` 原样传给 TypeScript 的 `parseJsonConfigFileContent`，相对路径下 include 匹配不到任何文件，对没有 `files`/`references` 的 tsconfig（如 `apps/agent-bff/tsconfig.json`）抛 TS18003 并中断整个采集；修复为生成配置时传绝对路径。`?raw`/`?url` 后缀经实测可被 enhanced-resolve 自行剥离，无需在 `filterDependencies` 单独处理。
  - 指标对比（同 commit、同范围）：内部边 2678 → **4499**；apps→packages 边 0 → **183**；环 2 → **12**（新增环均为 barrel-file 环，原 2 个环仍包含在内）；unresolved 3739 → **4**（3 个请求模块）；另有 111 条解析成功的外部边，按定义计为范围外已解析边。
- 最终残余噪音（记录为事实，不处理）：
  - `@physalis/workflow/workflow.css`（2 条）：包 exports 未暴露 `.css` 子路径，属包定义事实。
  - `demos/canvas-playground → @physalis/player/video-player/controller`、`demos/scene-3d-demo → @physalis/widgets/scene-3d`（各 1 条）：demo tsconfig 的 paths 命中目标是目录或无扩展名文件，`tsconfig-paths-webpack-plugin` 不做补全；运行时由 demo 自身构建配置解析，工具侧修复成本远超收益。
  - npm 外部包（`uuid`、`three/addons/*` 等）在 exportsFields 开启后均已解析为范围外边，不再出现在未解析依赖中。

#### 文件：`package.json`、`pnpm-lock.yaml`

- 改动类型：修改。
- 改动内容：增加经验证所需的重复和依赖采集工具依赖。
- 设计约束：固定实际安装版本并记录到报告。
- 验收点：冻结 lockfile 安装可运行。

### Step 5. 实现可选用户评价

#### 文件：`packages/code-metrics/src/evaluate.ts`

- 改动类型：新增。
- 改动内容：执行用户配置的热点公式和候选筛选器。
- 设计约束：
  - 默认关闭，未配置时 `score=null`。
  - 公式只能引用报告已有字段。
  - 必须显式声明归一化、权重和缺失值策略。
  - 候选措辞仅为“匹配用户规则”。
- 验收点：结果包含完整公式、参数、实值和逐条命中依据。

### Step 6. 测试、验收与文档回写

#### 文件：`packages/code-metrics/src/**/*.test.ts`、`packages/code-metrics/src/fixtures/**`

- 改动类型：新增。
- 改动内容：覆盖分类优先级、换行、分位数、Git、复杂度、重复、依赖、Schema 和确定性。
- 验收点：同一输入连续运行的稳定字段完全一致。

#### 命令

- 运行：第一阶段测试与 baseline 对账。
- 运行：第二阶段各采集器 fixture 与仓库抽样对账。
- 运行：`metrics:all` 两次并比较除时间/耗时外的 JSON。
- 修复：只修复本次引入的问题，不扩大到业务代码清理。

#### 文件：`docs/sdd-lab/2026-10-19_12-16_code-metrics-baseline/lifecycle.md`

- 回写执行记录、实际文件变化、依赖版本、验证结果和偏差。
- 若实现与指标口径不一致，先回写需求与方案，再继续代码。

### Step 7. 重复簇聚合与展示（2026-10-26 需求增补）

- 背景：重复片段以 pair 为最小记录，缺少「同一处代码出现几次」的聚合口径。探针实验（`.artifacts/code-metrics/_probe-dup-cluster.cjs`，commit `82c3da52` 的 1005 条 pair）验证了归并算法与参数敏感性：阈值 0.5–1.0 结果不敏感，口径分母是唯一有实质影响的参数；默认参数下 ≥10 实例的簇 19 个，最大簇 81 实例 / 79 文件（Svelte 图标模板家族，为真实价值而非串簇）。

#### 文件：`packages/code-metrics/src/cluster-duplication.ts`

- 改动类型：新增。
- 改动内容：把 `facts.duplication` 与 `facts.crossClassificationDuplication` 的 pair 全集聚合为重复簇。算法：pair 两端（文件 + 行号区间）作为节点，pair 本身即连通边；同文件内区间按起点排序扫描，交叠率 ≥ 阈值时归并（交叠率 = 交叠行数 ÷ 分母，分母口径可配置）。
- 设计约束：
  - 重复次数 = 簇内区间实例数；同文件自我重复（如同一文件两处相同片段）计为不同实例。
  - 聚类在 pair 全集上执行，簇记录全部成员区间，不预先按同/跨分类拆分。
  - 纯确定性数据变换，不访问文件系统。
  - 输出排序：按重复体量 = 重复次数 × 最大区间行数 降序；体量相同时依次按重复次数、最大区间行数降序，再按首成员文件名、行号稳定排序（2026-10-26 11:11 用户决策）。
- 验收点：嵌套子 clone 归并入母簇；星型 pair 连通成簇；同文件自我重复计 2 实例；空输入返回空数组；结果与探针基线一致（簇集合与排序符合体量规则）。

#### 文件：`code-metrics.config.mjs`

- 改动类型：修改。
- 改动内容：duplication 配置增加归并参数：`clusterRatioBase`（`shorter` | `longer` | `union`，默认 `shorter`）、`clusterThreshold`（默认 `0.6`）。
- 设计约束：两参数写入报告 meta 并参与配置哈希。
- 验收点：默认值下报告结果与探针结论一致（≥10 实例簇 19 个，最大簇 81 实例 / 79 文件）。

#### 文件：`packages/code-metrics/src/schema.ts`

- 改动类型：修改。
- 改动内容：`derived` 新增 `duplicationClusters`（实例数、文件数、最大区间行数、成员区间列表、参与 pair 数）；`schemaVersion` 提升。
- 验收点：新报告通过校验；缺字段或错类型被拒绝。

#### 文件：`packages/code-metrics/src/render-markdown.ts`

- 改动类型：修改。
- 改动内容：新增「重复簇」小节，按重复体量（重复次数 × 最大区间行数）降序展示前 N（N 同 `reading.limit`），并在指标定义中写明归并口径与阈值。
- 设计约束：数值全部来自报告 JSON，Markdown 不重算、不重排；展示条数不是质量阈值。
- 验收点：表格数值可逐项追溯到 `derived.duplicationClusters`。

#### 文件：`packages/code-metrics/src/cluster-duplication.test.ts`

- 改动类型：新增。
- 改动内容：fixture 覆盖嵌套 clone、星型连通、同文件自我重复、空输入，以及三种口径 / 阈值下归并行为的差异断言。
- 验收点：包内全部测试通过；关键 fixture 与手工推导对账一致。

### Step 8. 补齐契约并增加第一优先级结构指标（2026-10-26）

- 报告契约升至 `schemaVersion: 1.2.0`。
- `schema.ts` 提供 JSON Schema 与运行时校验。写盘前校验失败视为配置/契约错误。
- CLI 退出码：`0` 成功，`2` 至少一个采集器 `error`，`1` 配置、参数或校验失败。
- `meta.tools` 记录 typescript、jscpd、dependency-cruiser 版本。`scope.classification` 记录分类规则。
- Git：`excludeMerges`、`followRenames` 按配置生效并写入 meta。有限时间窗使用 `--since=<HEAD 提交时间 - N 天>`。历史覆盖天数使用根提交到 HEAD 的间隔。raw author 使用 `%an <%ae>`，canonical 先取 `%aN <%aE>`，再允许 `identityMap` 覆盖。
- 复杂度：保留现有 McCabe 口径。新增 Sonar 认知复杂度子集：`if/for/while/do/switch/catch/三元` 记 `1 + 嵌套层级`，`else/else if` 和每个 `&&/||` 记 1，嵌套函数从 0 独立计算，同名直接递归记 1。Svelte 行号映射回原始文件。
- 重复：保留 pair 行数累加。新增闭区间并集后的 `uniqueDuplicatedLines` 与 `duplicatedLineDensity`。另保留 jscpd 原生 statistics，不与本口径混用。
- 依赖：Tarjan 强连通分量写入 `facts.graph.components`；环路径按规范化旋转去重。文件和 workspace 输出 `afferent`、`efferent`、`instability = efferent / (afferent + efferent)`。
- 时间耦合：在配置指定的 Git 窗口内统计共同提交，输出双向条件概率和 Jaccard。默认窗口 `90d`，最小共同提交数 3。
- 评价：按配置执行 `none/rank` 归一和 `skip/zero` 缺失策略。文件分数是已使用权重的加权平均。未启用时保持禁用状态。
- 验收点：新增纯函数均有对账测试；类型检查通过；采集失败不把退出码留在 0。

## Risk And Mitigation / 风险与缓解

- 风险：复杂度定义在不同工具间不一致。
  - 缓解方式：在配置和报告中枚举决策节点，使用 fixture 逐项验证。
- 风险：Svelte 与 tsconfig alias 导致依赖图漏边。
  - 缓解方式：输出已解析文件比例、未解析依赖和原因，覆盖率不达事实完整性要求时不输出无条件结论。
  - 2026-10-26 更新：该风险已在当前基线实锤（apps→packages 内部边为 0），根因与修复方案见 Step 4 `collect-dependencies.ts` 的"执行中发现与方案修正"小节，等待用户批准后执行。
- 风险：Git merge、重命名和作者别名导致历史数据膨胀或分裂。
  - 缓解方式：固定排除 merge、固定重命名策略、同时保留 raw/canonical identity。
- 风险：i18n、图标和大型接口文件主导仓库聚合。
  - 缓解方式：保留事实但独立分类，不用主观方式删除数据。
- 风险：用户配置的公式被误读为系统结论。
  - 缓解方式：评价层单独命名，报告展示公式与参数，并使用“匹配用户规则”措辞。
- 风险：新工具默认值变化导致结果漂移。
  - 缓解方式：锁定版本、显式配置关键参数、记录配置哈希。
- 风险：重复簇归并的交叠判定引发链式串簇，把不相关片段并成巨型簇。
  - 缓解方式：探针实验（`_probe-dup-cluster.cjs`）已证实阈值 0.5–1.0 区间不敏感、巨型簇来自 pair 边直连而非区间归并；口径与阈值双参数可配置并参与配置哈希，结果可复现。2026-10-26 记录。

## Execute Checkpoint / 执行检查点

- 当前理解：方案只提供客观事实、确定性派生及默认关闭的用户评价，不代替用户判断。
- 核心目标：先完成可复现基线，再补齐重构分析所需证据。
- 下一步动作：Step 8 已实现并通过包内测试；仍需对同一 commit 连续运行两次 `metrics:all` 做最终对账。
- 风险：第二阶段解析覆盖率必须通过实际 fixture 和仓库抽样验证，不能仅依赖工具宣称。
