# Lifecycle / 生命周期: 客观代码度量基建

```yaml
status: executing
result: pending
created_at: 2026-10-19 12:16
updated_at: 2026-10-26 16:54
owner: user
```

## Current Summary / 当前摘要

- 批准状态：Step 7（重复簇聚合与展示）及体量排序变更均已执行完成，验证通过。
- 当前状态：`executing`。
- 当前核心目标：分两个阶段建立忠实于数据、可复现、可追溯的代码度量基建。
- 下一步唯一动作：对同一 commit 连续运行两次 `metrics:all`，核对除时间和耗时外的 JSON。

## Execution Log / 执行记录

- 1. 2026-10-19 12:09：用户确认核心原则为忠实于数据，主观判断交给用户；限定第一、第二阶段范围。
- 2. 2026-10-19 12:14：用户选定“可配置热点公式，默认不设权重和阈值”。
- 3. 2026-10-19 12:15：用户确认新基建完成后可移除 `scripts/count-lines.mjs`。
- 4. 2026-10-19 12:16：需求文档与技术方案落盘，状态由 `draft` 进入 `planned`；尚未进入代码执行。
- 5. 2026-10-19 12:22：用户明确要求开始执行，状态由 `planned` 进入 `executing`。
- 6. 2026-10-19 12:28：用户要求把 `scripts/code-metrics` 改为独立 package；实现位置调整为 `packages/code-metrics`。
- 7. 2026-10-19 15:16：重复代码和依赖图改为按目录根扫描，再按文件集合与分类过滤，避免 Windows 命令行超长。
- 8. 2026-10-19 15:35：`summary.md` 改为阅读报告，按单一指标展示前 N 名和汇总事实，完整数据仍留在 JSON。
- 9. 2026-10-26 10:00：基线复盘中发现依赖图大面积漏边：`unresolved=3739`、apps→packages 内部边为 0。经对照探针实验定位三个根因（tsConfig 缺 paths 别名、depcruise 默认忽略包 exports、Vite `?raw`/`?url` 后缀），全部可通过工具侧修复且不动项目配置；证据与修复方案已落盘至 `technical-plan.md` Step 4 的"执行中发现与方案修正"。
- 10. 2026-10-26 10:17：用户批准后完成修复实现：`collect-dependencies.ts` 改为按最近 tsconfig 分组运行 depcruise 并按权威性合并，配置追加 `exportsFields`/`conditionNames`；执行中额外发现并修复 depcruise 相对 `tsConfig.fileName` 导致的 TS18003（改为传绝对路径）。全量复跑验证：内部边 2678→4499、apps→packages 边 0→183、环 2→12（含原 2 个）、unresolved 3739→4（3 个请求模块，均为已记录的残余噪音）；新增 6 例单测、包内 14 测试全部通过；`.artifacts` 基线已重新生成。
- 11. 2026-10-26 10:45：需求增补——重复片段新增「重复簇 / 重复次数」聚合指标。用户拍板：归并的重合度口径与阈值均做成可配置参数，默认口径 `交叠行数 ÷ 较短区间长度`，默认阈值 `0.6`。依据探针实验 `.artifacts/code-metrics/_probe-dup-cluster.cjs`（1005 条真实 pair）：阈值 0.5–1.0 不敏感，口径分母才是实质参数；默认参数下 ≥10 实例簇 19 个，最大簇 81 实例 / 79 文件（Svelte 图标模板家族）。`requirements.md`、`technical-plan.md` Step 7 已回写，等待用户批准进入实现。
- 12. 2026-10-26 10:52：用户批准后完成 Step 7 实现：新增 `cluster-duplication.ts`（pair 端点并查集 + 同文件区间按交叠率扫描归并，交叠率 = 交叠行数 ÷ 分母，分母口径 shorter/longer/union，阈值与口径均来自配置）；`code-metrics.config.mjs` duplication 增加 `clusterRatioBase`（默认 `shorter`）与 `clusterThreshold`（默认 `0.6`），`schemaVersion` 升至 `1.1.0`；`types.ts` 新增 `DuplicationCluster` 类型与 `derived.duplicationClusters`、`meta.duplicationCluster`；`run.ts` 在 pair 全集（同分类 + 跨分类）上聚簇；`render-markdown.ts` 新增指标定义、采集状态参数行与「重复簇」Top N 小节。执行中顺带修复既有文件 `collect-dependencies.test.ts` 第 104-106 行遗留的多余闭合括号（语法错误，非本次改动引入）。验证：新增 `cluster-duplication.test.ts` 8 例（星型连通、嵌套 clone 在 shorter/longer 口径下的归并差异、同文件自我重复计 2 实例、链式传递归并、阈值下不归并、非法口径运行时拒绝、稳定排序），包内 22 测试全部通过，`tsc --noEmit` 无错误；全量复跑后与探针基线完全对账——总簇数 515、≥10 实例簇 19 个、最大簇 81 实例 / 79 文件（Svelte 图标模板家族，`maxLines=21`、`pairCount=78`），第 2 大簇 54 实例 / 11 文件（`workflow-node/edit` 各 gen-node 表单复制块家族）。`.artifacts` 基线已重新生成。
- 13. 2026-10-26 11:12：用户决策变更——重复簇排序从「重复次数」改为「重复体量 = 重复次数 × 最大区间行数」降序。已按 Reverse Sync 先回写 `requirements.md` 决策记录与 `technical-plan.md` Step 7 排序定义，再改 `cluster-duplication.ts` 排序键（体量降序，其次次数、行数、首成员文件与行号稳定排序）与 `render-markdown.ts` 指标定义文案。验证：包内 22 测试通过（排序断言更新为体量规则）、`tsc --noEmit` 无错误、全量复跑对账簇集合不变（515 个，≥10 实例 19 个），排序变为：第 1 名 gen-node 表单家族 54×218=11772，第 2 名 account-modal 家族 39×296=11544，81 次的图标模板（体量 81×21=1701）跌出前列——大块少量复制不再被小片段大量复制埋没。`.artifacts` 基线已重新生成。
- 14. 2026-10-26 16:54：用户要求补充第一优先级指标，并补齐当前 SDD 已承诺但未兑现的契约。已先回写需求、技术方案和本记录，随后按 Step 8 实现。
- 15. 2026-10-26 17:12：Step 8 实现完成。新增认知复杂度、唯一重复密度、强连通分量、依赖稳定度和时间耦合；补齐报告校验、工具版本、HEAD 相对 Git 窗口、评价公式、CLI 退出码和采集诊断，并删除 `scripts/count-lines.mjs`。验证：`tsc --noEmit` 通过，包内 30 项测试通过，ESLint 无警告。全仓两次 `metrics:all` 对账尚未执行。

## Transition Log / 状态流转

- 2026-10-19 12:16:
  - From：`draft`
  - To：`planned`
  - 原因：需求边界、客观性原则、热点评价方式和旧脚本处置均已明确，技术方案已形成。
  - 依据：`requirements.md`、`technical-plan.md`。
  - 下一步：等待用户批准执行。
- 2026-10-19 12:22:
  - From：`planned`
  - To：`executing`
  - 原因：用户明确要求开始执行。
  - 依据：用户消息“开始执行”。
  - 下一步：实现并验证两阶段度量基建。
