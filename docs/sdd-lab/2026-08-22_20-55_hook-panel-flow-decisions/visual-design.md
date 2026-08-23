# Visual Design / 视觉设计文档: Hook 面板分页·命名·样式收敛

## Source / 来源

- 交互形态：来自用户反馈三项（2026-08-22）：
  - 「hook 判断需要加分页」
  - 「样式有问题，现在行太多，高度被挤没了」（补充：「不是行高太高，是看不见行了」）
  - 「hook 判定这个名字不好，能不能重新想一些合适的名字」
- 设计规范来源（本项目现有事实，不新增体系）：
  - 面板词汇：`panel-toolbar` / `filter-bar`（对齐 TopicPanel / ToolPanel，见 [ui-panel-layout.mdc](file:///home/lab/Documents/trae_projects/new-start/.cursor/rules/ui-panel-layout.mdc)）
  - 行高基线：时间线条目 28px（沿用 hook-judgement-recovery 迭代 visual-design §1）
  - 状态徽标：小号文字 + 语义色文字色（`ok`=success / `retried_ok`=primary / `downgraded`=warning / `pending`=muted）

## Design Changes / 设计变更

### 1. 命名（用户选定「流程决策」）

- 面板标题：zh「流程决策」en "Flow Decisions"（i18n key `views.flowDecisions`）。
- 视图 tab：icon 不变（判决天平），id `hook-judgements` 不变（layout 持久化安全）。
- 4 个 hook 标签（hook.*）不变：范围完成 / 课题匹配 / 课题修订 / 评分反馈。
- 语义：对齐 hook-inject-points 后的注入点调度机制（IP-1~5），为后续收纳非裁决 hook 留空间；同时去掉「Hook」英文黑话。

### 2. 布局滚动修复（「看不见行」根因）

- 现状：`.judgement-panel { overflow: auto }` 与 `.list { overflow-y: auto }` 双层滚动嵌套，滚动条落在外层，toolbar/过滤条随内容滚出可视区。
- 修正：`.judgement-panel` 改 `overflow: hidden`（flex column 布局不变，padding/gap 不变），滚动**唯一归属** `.list`（`flex:1; min-height:0; overflow-y:auto`）。对齐 SearchPanel 单层滚动模式。

### 3. 行高收敛

- 记录行 `.row`：padding 由 `var(--space-1) var(--space-2)`（上下 8px）收敛为 `3px var(--space-2)`，行高约 28px（对齐 visual-design 基线）。
- 其余词汇不变：时间戳 `--fs-xs` mono muted / hook 徽标 `--fs-sm` 正文色 / 状态徽标 `--fs-xs` 语义色 / 摘要 `--fs-xs` muted truncate / chevron 12px。

### 4. 分页交互（滚动自动加载）

- 列表滚动距底 < 80px 且 hasMore 时自动加载下一页；底部加载指示「已载入 M / 总数 N」（`--fs-xs` muted）。
- 过滤切换重置第一页并滚动回顶；过滤条计数显示过滤后总数 `total`（替换当前前端过滤计数）。

### 5. 空态

- 保持：无记录「暂无裁决记录」；有过滤条件为空「无匹配记录」（`--fs-xs` muted 居中）。

## Icon / SVG 组件导出

- 无新增 Icon。视图 icon（判决天平）沿用现有 `viewRegistry["hook-judgements"]` 内联 SVG，不导出文件。

## Design Tokens 引用

- 全部沿用既有 tokens：`--color-surface/border/bg/hover/text/text-muted/primary/success/warning/error`、`--fs-sm/xs`、`--space-1/2/4`、`--radius-md/sm`、`--font-mono`。不新增 token、不引入硬编码色值。
