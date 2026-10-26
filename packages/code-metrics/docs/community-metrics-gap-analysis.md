# 社区指标差距分析

> 审计日期：2026-10-26  
> 适用范围：`@physalis/code-metrics`  
> 文档性质：指标候选与产品边界分析。2026-10-26 已按 `schemaVersion 1.2.0` 实现第一优先级五项，并补齐当时已承诺的契约缺口。

## 1. 目的

本文对照 SonarQube、Qlty、jscpd、dependency-cruiser 和 DORA 等社区主流体系，整理当前项目尚未提供、但可能有价值的指标。

本文遵循以下原则：

1. 区分“当前 Spec 已承诺但尚未完成”“适合后续引入”“当前明确范围外”。
2. 优先选择定义清楚、可复现、可追溯的事实或确定性派生指标。
3. 不把单一指标直接解释为质量、风险或重构结论。
4. 不默认引入综合权重、质量阈值或主观评级。
5. 新指标进入实现前，仍需先更新需求文档和技术方案并获得执行批准。

## 2. 当前项目定位

当前 `code-metrics` 定位为仓库事实采集器，而不是 SonarQube/Qlty 式完整质量平台。现有能力主要包括：

- 文件规模：字节数、物理总行数、非空行数、文件数和分位数。
- 文件分类：主分类、标签、workspace、目录和扩展名。
- Git 事实：按时间窗统计提交数、added、deleted、churn、贡献者和最后修改时间。
- 复杂度：TypeScript/Svelte script 的函数级 McCabe 圈复杂度。
- 重复：jscpd 重复 pair、同/跨分类重复和重复簇。
- 依赖：内部边、入度、出度、未解析依赖、循环路径和扫描覆盖率。
- 报告：`facts`、`derived`、`evaluation`、`diagnostics` 分层 JSON 及 Markdown 摘要。

当前 SDD 明确排除测试覆盖率、构建耗时、包体积、线上缺陷、CI 门禁、长期趋势平台和默认综合评分。因此，这些能力在社区对比中属于“缺失”，但不是当前实现缺陷。

## 3. 状态分类

| 状态       | 含义                                            |
| ---------- | ----------------------------------------------- |
| 当前缺口   | 已被当前 SDD 或技术方案承诺，但实现尚未完整兑现 |
| 后续候选   | 符合客观事实采集定位，适合进入后续需求迭代      |
| 范围外候选 | 有社区价值，但需要改变当前范围或引入外部数据源  |
| 不建议优先 | 容易误导、主观性较强或投入产出比较低            |

## 4. 推荐优先级

如果只选择五项新增指标，建议优先考虑：

1. 认知复杂度。
2. 唯一重复密度。
3. 依赖强连通分量。
4. 依赖稳定度。
5. 时间耦合。

这五项可以补足“理解难度、重复比例、循环结构、依赖方向和隐性协作关系”五个当前薄弱面，并且不要求立即引入质量评级。

## 5. 第一优先级：适合当前产品定位

### 5.1 认知复杂度

**状态**：已实现。

**定义**

认知复杂度衡量控制流对人类理解造成的负担。与 McCabe 圈复杂度相比，它通常会考虑嵌套深度、流程中断和结构化控制流，而不会简单地把所有分支视为等价。

**建议输出**

- 函数级 `cognitiveComplexity`。
- 文件级 `cognitiveComplexityMax`。
- 文件级 `cognitiveComplexitySum`。
- 达到最大值的函数及源码位置。
- 算法名称、版本和语言覆盖状态。

**价值**

- 更贴近“阅读和理解难度”，可以补充当前只衡量独立执行路径数量的圈复杂度。
- SonarQube 和 Qlty 均将其作为核心可维护性指标。

**注意事项**

- 必须固定算法版本；不同实现之间不可假设数值完全可比。
- Svelte script 的源码行号必须映射回原始 `.svelte` 文件。
- 函数级统计必须避免把嵌套函数的决策同时计入父函数和子函数，除非算法明确定义如此。
- 不应内置“超过某数值即低质量”的默认判断。

### 5.2 唯一重复密度

**状态**：已实现。

**定义**

```text
重复密度 = 唯一重复行数 / 可分析代码行数 × 100%
```

“唯一重复行数”应先对同一文件中的重复区间取并集，不能直接累加每个 clone pair 的行数。

**建议输出**

- 仓库、分类、workspace、扩展名和文件级 `duplicatedLineDensity`。
- `uniqueDuplicatedLines`。
- jscpd 原生 `duplicatedTokens` 和 `percentageTokens`。
- 分母使用的代码行数及可分析文件数量。

**价值**

- 比当前重复 pair 数量或 `duplicatedLines` 简单累加更适合横向比较。
- 仓库规模变化时，百分比比绝对重复行数更稳定。
- 可与 SonarQube、Qlty 和 jscpd 原生报告进行有限度对照。

**注意事项**

- 需要明确“第一份原始代码是否也算重复行”；应固定与选用工具一致的定义。
- 同分类和跨分类应分别输出，也可另给全范围总值。
- 必须记录 jscpd 版本、模式、最小行数和最小 token 数。
- 不同语言、生成内容和文档是否进入分母必须由 scope 明确决定。

### 5.3 依赖强连通分量

**状态**：已实现。只输出含环或自环的强连通分量。

**定义**

强连通分量（SCC）是有向图中任意节点都可到达其他节点的最大节点集合。包含两个及以上节点的 SCC，或存在自环的单节点 SCC，代表循环依赖区域。

**建议输出**

- `stronglyConnectedComponents` 明细。
- 循环 SCC 数量。
- 最大 SCC 节点数。
- 位于循环 SCC 中的文件数和占比。
- 每个 SCC 的内部边数、入口边数和出口边数。

**价值**

- 比枚举若干 DFS 回边更稳定。
- 同一个循环区域只形成一个结构性事实，便于聚合和趋势比较。
- 可识别大规模互相依赖区域，而不只是展示某一条环路径。

**注意事项**

- SCC 与“所有简单环”不是同一概念，不应混用字段名称。
- 若仍需要完整环路径，应作为独立、可能很昂贵的输出。
- 结果应基于依赖采集覆盖率；覆盖不足时必须保留诊断。

### 5.4 依赖稳定度

**状态**：已实现。文件级与 workspace 级都输出稳定度。

**定义**

参考 Robert C. Martin 的依赖稳定度指标：

```text
Instability = Ce / (Ca + Ce)
```

- `Ca`：afferent coupling，被其他模块依赖的数量。
- `Ce`：efferent coupling，当前模块依赖其他模块的数量。
- 结果接近 `0` 表示更稳定，接近 `1` 表示更依赖外部。

**建议输出**

- 文件、目录、workspace、package 级 `afferentCoupling`。
- 文件、目录、workspace、package 级 `efferentCoupling`。
- `instability`。
- `Ca + Ce = 0` 时输出 `null`，不要补零。

**价值**

- 当前 incoming/outgoing 只给原始边数，稳定度可以帮助比较不同规模模块的依赖方向。
- dependency-cruiser 原生 `--metrics` 已提供相似的 dependents、dependencies 和 instability 能力。

**注意事项**

- 文件级 import 条数容易受重复 import 和 barrel file 影响，优先按唯一依赖节点计算。
- package 级统计通常比文件级更适合架构分析。
- 稳定不等于优质，不稳定也不等于缺陷；它只描述依赖方向。

### 5.5 时间耦合

**状态**：已实现。默认使用 90 天窗口，最小共同提交数为 3。

**定义**

时间耦合用于描述两个文件在 Git 历史中共同修改的频率。建议至少保留以下事实：

```text
cochangeCount(A, B) = A 与 B 同时出现的 commit 数
confidence(A → B) = cochangeCount(A, B) / commits(A)
jaccard(A, B) = cochangeCount(A, B) / commits(A ∪ B)
```

**建议输出**

- 文件对共同修改次数。
- A→B 和 B→A 的条件概率。
- Jaccard 系数。
- 使用的历史时间窗、最小共同提交数和排除策略。
- 按 workspace/package 聚合后的时间耦合边。

**价值**

- 可以发现静态 import 图看不到的隐性关系，例如接口与实现、配置与运行代码、跨包同步修改。
- 有助于评估现有模块边界是否符合实际变更边界。

**注意事项**

- 大规模格式化、批量重命名、版本升级提交可能产生虚假耦合，需要显式、可配置的过滤规则。
- Git rename 策略必须先固定。
- 两个文件同时修改不代表因果关系，不能自动解释为架构问题。
- 文件对数量可能呈平方级增长，应设置客观的最小支持度并保留完整参数。

## 6. 第二优先级：需要更多数据或基础能力

### 6.1 所有权集中度

**状态**：后续候选。

当前项目已有 contributor 集合，但“贡献者数量”不能反映知识是否集中。

**建议指标**

- `topContributorShare`：贡献最多作者的修改占比。
- `contributorEntropy`：作者贡献分布的信息熵。
- `effectiveContributorCount`：基于贡献分布计算的有效贡献者数量。
- 可选的 Bus Factor 近似值。

**输入选择**

- commit 次数。
- churn。
- `git blame` 行所有权。

三种输入表达的含义不同，不能混为同一个指标。

**注意事项**

- `.mailmap` 或显式身份映射必须先可靠生效。
- 所有权集中只描述知识分布，不代表人员绩效或代码质量。
- 需要谨慎处理隐私、机器人账户和批量格式化提交。

### 6.2 代码年龄与活跃度

**状态**：后续候选；当前已有 `lastModifiedAt` 的部分基础。

**建议指标**

- 文件首次出现时间。
- 文件最后修改时间。
- 距离最后修改的天数。
- 指定时间窗内活跃文件比例。
- package/workspace 的修改年龄分布。

**价值**

- 区分活跃区域、稳定核心和长期未触达区域。
- 可以和复杂度、依赖或重复事实组合，由用户配置形成热点公式。

**注意事项**

- 时间相关指标与“固定 commit 可复现”的要求存在定义冲突。
- 若计算“距今天数”，结果会随墙钟时间变化；更可复现的方案是相对 HEAD 提交时间计算。
- 久未修改可能表示稳定，也可能表示无人维护，不能自动下结论。

### 6.3 New Code / Diff 指标

**状态**：范围外候选。

**建议指标**

- 新增或修改行的覆盖率。
- 新代码复杂度增量。
- 新代码重复密度。
- 新增依赖环、未解析依赖和架构规则违规。
- 相对 baseline 新增的静态问题数量。

**价值**

- SonarQube 等主流平台倾向优先治理新代码，而不是要求一次性消除全部历史债务。
- 更适合 PR 检查和渐进改进。

**前置条件**

- 定义 baseline commit、目标分支或时间窗口。
- 增加 diff 映射能力。
- 明确 rename、删除、生成文件和 dirty worktree 的行为。

### 6.4 API 表面积与变化

**状态**：后续候选。

**建议指标**

- package 导出入口数量。
- 导出符号数量。
- 公共 API 依赖入度。
- 相对 baseline 的新增、删除和签名变化。

**价值**

- 对 monorepo package 边界、公共接口膨胀和潜在破坏性变更有帮助。
- 比单纯统计文件数更接近 package 的外部维护成本。

**注意事项**

- 必须基于 `package.json#exports` 和 TypeScript 类型信息，而不是只扫描 `export` 关键字。
- 类型导出、条件导出和 Svelte 组件 API 需要单独定义。

## 7. 第三优先级：需要测试或规则体系

### 7.1 测试覆盖率

**状态**：当前 SDD 明确范围外。

**社区常见指标**

- 行覆盖率。
- 分支覆盖率。
- 函数覆盖率。
- statement 覆盖率。
- Diff Coverage。

**判断**

分支覆盖率和 Diff Coverage 通常比单纯全仓行覆盖率更有行动价值。但覆盖率只说明代码是否被执行，不说明断言质量。

**前置条件**

- 定义覆盖率报告输入格式，例如 Istanbul/LCOV。
- 固定多 workspace 报告合并规则。
- 处理生成文件、source map 和测试分类。

### 7.2 变异测试分数

**状态**：范围外候选。

```text
Mutation Score = 被测试杀死的 mutant 数 / 有效 mutant 总数 × 100%
```

它比普通覆盖率更能反映测试是否能发现行为错误，但运行成本高、工具链复杂，适合核心 package 或抽样运行，不适合直接加入每次全仓基础采集。

### 7.3 架构规则违规

**状态**：后续评价层候选。

**示例**

- 禁止生产代码依赖测试代码。
- 禁止跨层反向依赖。
- 禁止绕过 package public API。
- 禁止指定 workspace 之间直接依赖。
- 孤立模块、不可解析依赖和循环依赖规则。

**判断**

dependency-cruiser 已支持 `forbidden` 规则，但规则本身属于项目决策。建议继续保留原始依赖事实，并把规则命中放在 `evaluation` 层，避免把规则伪装成客观事实。

### 7.4 静态问题、技术债与评级

**状态**：范围外候选。

SonarQube/Qlty 通常提供：

- maintainability、reliability、security issue 数量。
- code smell、bug、vulnerability、security hotspot。
- 修复成本和技术债率。
- A–E 或 A–F 评级。

这些指标依赖规则集、严重度和修复时间模型，主观性明显高于当前事实采集器。若未来引入，应完整记录规则版本、启用配置和评级公式，并与事实层隔离。

## 8. 不建议优先引入的指标

### 8.1 Maintainability Index

Maintainability Index 通常由 Halstead Volume、圈复杂度和 LOC 等组合而成。不同工具公式和归一化方式不一致，而且 LOC 权重容易让大文件天然处于劣势。

除非存在明确消费场景和固定公式，否则不建议优先于认知复杂度、重复密度和依赖指标。

### 8.2 默认综合质量分

把复杂度、churn、重复和规模加权成单一分数会隐藏原始差异，权重也必然包含主观判断。当前项目可以支持用户显式配置公式，但不应提供未经批准的默认权重或默认“风险”解释。

### 8.3 原始提交数或贡献者数排名

提交粒度受团队习惯影响，贡献者数量也受身份拆分影响。它们可以作为事实保留，但不应单独解释为活跃度、协作质量或 Bus Factor。

### 8.4 仅增加更多 LOC 变体

当前已有物理总行数与非空行数。除非能够稳定解析多语言并明确区分 comment、blank、code，否则新增所谓 SLOC 容易制造不可比数据。优先级低于认知复杂度和唯一重复密度。

## 9. DORA 与工程效能边界

DORA 当前官方模型包含五项软件交付绩效指标：

- Change lead time。
- Deployment frequency。
- Failed deployment recovery time。
- Change fail rate。
- Deployment rework rate。

这些指标需要部署、发布、事故和恢复数据，不能仅通过代码仓事实可靠推导。建议由独立工程效能系统采集，再通过 commit/deployment 标识与代码指标关联，不应直接放入 `code-metrics` 静态采集内核。

## 10. 建议的后续迭代拆分

### 迭代 A：先提升现有指标可信度

- 解决固定 commit 可复现性与滚动 Git 窗口的定义冲突。
- 补运行时 Schema 和工具版本。
- 修正 Svelte 源码位置。
- 对重复区间取并集。
- 用 SCC 替代当前不完整的环结构表示。
- 补齐采集覆盖率、失败项和失败原因。

### 迭代 B：补充静态结构指标

- 认知复杂度。
- 唯一重复密度。
- 依赖稳定度。
- 时间耦合。
- 所有权集中度。

### 迭代 C：引入外部输入

- 覆盖率与 Diff Coverage。
- New Code 指标。
- API 变化。
- 可选架构规则。

### 迭代 D：跨系统工程效能

- 部署、事故与 DORA 数据。
- 长期趋势和跨版本对比。
- CI 或质量门禁策略。

## 11. 官方参考资料

- [SonarQube Server 2025.4：Measures and metrics](https://docs.sonarsource.com/sonarqube-server/2025.4/user-guide/code-metrics/metrics-definition)
- [SonarQube Server 2025.4：Quality gates](https://docs.sonarsource.com/sonarqube-server/2025.4/quality-standards-administration/managing-quality-gates/introduction-to-quality-gates)
- [Qlty：Maintainability metrics and ratings](https://docs.qlty.sh/cloud/maintainability/metrics)
- [Qlty：Complexity](https://docs.qlty.sh/complexity)
- [Qlty：Duplication](https://docs.qlty.sh/duplication)
- [Qlty：Coverage metrics](https://docs.qlty.sh/coverage/metrics)
- [jscpd：JSON reporter](https://jscpd.dev/reference/reporters/json)
- [dependency-cruiser：Rules reference](https://github.com/sverweij/dependency-cruiser/blob/main/doc/rules-reference.md)
- [dependency-cruiser：CLI metrics](https://gitlab.com/sverweij/dependency-cruiser/-/blob/develop/doc/cli.md)
- [DORA：Software delivery performance metrics](https://dora.dev/guides/dora-metrics/)
- [DORA：Metrics history](https://dora.dev/guides/dora-metrics/history/)
