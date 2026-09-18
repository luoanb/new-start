# Spec: 布局分隔线统一为 1px（消除双线与冗余横线）

## Goal

- 要解决什么问题：布局区内「边框偏粗、线多」——① 分割条 Splitter 画的是 2px 线，与其它 1px 细线并列显粗；② 左栏会话视图的 `border-right` 与容器 `.sidebar-area` 的 `border-right` 并排成 2px（另带一层阴影）；③ 视图自带的顶部工具条与容器 `.container-header` 各画一条 `border-bottom`，上下相邻形成两条横线。
- 验收结果：布局区所有常驻分隔线均为 1px、同色；左栏右缘只剩一条 1px；每个视图容器顶部只剩一条横线；分割条保留 4px 拖拽热区与 hover 高亮。

## Done Contract

- 什么算完成：`Splitter` 不再画常驻线（仅 hover/active 高亮）；`.main-pane` 非末尾分栏恒定 1px 右边线、末尾分栏不再画线；`SessionList .sidebar` 的 `border-right` 与 `box-shadow` 移除；`SessionList .sidebar-header` / `FileExplorer .ws-bar` / `SearchPanel .search-bar` / `GitPanel .toolbar` / `TerminalPanel .tabbar` 去掉 `border-bottom`。
- 由什么证明：`pnpm --filter pulsar-app check` 0 error；App 内目视：左栏右缘、侧栏↔中栏、中栏↔右栏、底栏上缘、main 分栏之间全部为等宽细线；分割条 hover 仍有高亮；拖动分隔条仍可调整尺寸。
- 哪些情况仍算未完成：任一边界出现 2px 或两条并排线；某处分隔线消失（如两个非激活分栏之间）；分割条拖拽热区变窄或失效；视图顶部横线一条都不剩。

## Scope

- In：`packages/pulsar-app/src/lib/layout/Splitter.svelte`；`packages/pulsar-app/src/routes/+page.svelte`（`.main-pane`）；`packages/pulsar-app/src/lib/components/SessionList.svelte`、`FileExplorer.svelte`、`SearchPanel.svelte`、`GitPanel.svelte`、`TerminalPanel.svelte`。
- Out：视图自带的重复**标题**（如 SessionList 的「会话」h2 与容器 tab 重复）不做去重；`.container-header` 自身的 `border-bottom` 保留；`.sidebar.collapsed { width: 48px }` 不动；`main-pane.active` 高亮语义不做增强（见 Facts）。

## Facts / Constraints

- `--border-width: 1px`、`--color-border` 是唯一分隔线口径；目标就是让所有常驻线都等于它。
- **Splitter 实际画 2px**：[Splitter.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/layout/Splitter.svelte#L58-L70) 盒子 4px 宽 + `margin: 0 -2px`，`::after` 再左右各内缩 1px → 2px、`opacity: .9`。
- 全仓仅 4 处 `<Splitter>`，全在 [+page.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/routes/+page.svelte#L695-L772)（侧栏↔中栏、main 分栏之间、底栏上缘、中栏↔右栏），没有组件内部使用——因此「分割条不画常驻线」不会误伤别处。
- 4 处分割条位置都已有相邻的 1px 边框兜底：`.sidebar-area` 的 `border-right`、`.main-pane` 的 `border-right`、`.panel-area` 的 `border-top`、`.info-area` 的 `border-left`。唯一缺口是 `.main-pane` 现在只有 `active` 才显色，非激活分栏之间无线 → 需改成「非末尾分栏恒定 1px」。
- `main-pane.active` 的 `border-right-color: var(--color-border)` 实际上**一直是不可见的**：该边框位于 [E-1,E)，而分割条 `::after` 覆盖 [E-1,E+1) 且同色，正好盖住。所以本次去掉它对视觉无回退。
- 末尾分栏（`:last-child`）绝不能画右边线：其右缘紧邻 `.info-area` 的 `border-left`，两者并排会成为新的 2px 双线。
- 视图自带顶部横线的**完整清单**（已逐文件核对，仅这 5 个）：`SessionList .sidebar-header`（347）、`FileExplorer .ws-bar`（976）、`SearchPanel .search-bar`（141）、`GitPanel .toolbar`（730）、`TerminalPanel .tabbar`（429）。其余 7 个容器视图（TopicPanel / HookJudgementPanel / ToolPanel / PollerPanel / LogPanel / ProvidersModelsPanel / NeuronListPanel）顶部本就无横线，不必改。
- `FileExplorer` 的 `.ws-input-row`（1021）与 `.tree-bar`（1041）也是横线，但它们不是「紧贴 container-header 的第一条」，属视图内部结构，本次不动。

## Restated Understanding

- 我理解当前任务是：把布局区的分隔线统一到 1px 单一口径，去掉「双线」和「多余的横线」。
- 当前核心目标是：同一屏内不再出现粗细/颜色不一致的分隔线，也不再出现上下相邻的两条横线。
- 当前边界是：只改分隔线的**绘制归属**，不改任何布局尺寸、交互与信息结构。
- 暂不处理：视图标题与容器 tab 的文案重复（SessionList「会话」出现两次）。

## Checkpoint Summary

- 当前任务理解：P0 分割条不再画常驻线、P1 左栏去重、P2 去掉 5 处视图自带顶部横线。
- 当前核心目标：布局分隔线统一 1px。
- 当前进度：待实现。
- 涉及文件 / 模块：`Splitter.svelte`、`+page.svelte`、`SessionList.svelte`、`FileExplorer.svelte`、`SearchPanel.svelte`、`GitPanel.svelte`、`TerminalPanel.svelte`。
- 风险：分割条常驻线去掉后，若某处相邻边框缺失会出现「无线」——已逐处核对 4 个位置均有兜底；hover 高亮从「不变色的 2px 线」变成「变主色的 2px 线」，视觉语义不变。
- 验证方式：`pnpm --filter pulsar-app check`；App 内逐边界目视 + 拖拽分隔条。
- Execution Approval: `Approved`（2026-09-18，用户选定 P0 + P1 + P2）

## Change Log

- 2026-09-18: 初始记录并实现。与 spec 的一处偏差（Reverse Sync 记录）：
  1. **P2 少改一个文件**：`TerminalPanel .tabbar` 的 `border-bottom` **未移除**。它不是单纯的分隔线，而是「浏览器式页签」的底部导轨——页签按钮是 `border-bottom: none` + 圆角上缘，`.tabbar button.active` 只画左右上三边，靠这条底线收口。去掉后激活页签会缺一条边、形状破掉。其余 4 处（SessionList / FileExplorer / SearchPanel / GitPanel）已按计划移除。
- 其它落地细节：`.main-pane.active` 规则整条删除（其 `border-right-color` 本来就被分割条 `::after` 覆盖而不可见），换成 `.main-pane:not(:last-child)` 恒定 1px；`.sidebar` 同时去掉了 `z-index: 1`（原为让阴影压过主区，阴影已删）。

## Validation

- Self-check: P0 / P1 完成；P2 完成 4/5（TerminalPanel 按上述理由保留）。
- Static checks: `pnpm --filter pulsar-app check` → 0 errors / 20 warnings（与既有基线一致）。
- Runtime / Test: 未跑自动化（纯 CSS 表现层）。
- Human confirmation: 待用户 App 内确认。
- 核心目标是否已由证据证明完成：否（差人工目视）。

## Resume / Handoff

- 当前状态：实现完成，静态检查通过，待人工确认。
- 下一步唯一动作：App 内逐边界目视确认——左栏右缘 / 侧栏↔中栏 / 中栏↔右栏 / 底栏上缘 / main 分栏之间均为等宽 1px 细线；SessionList / FileExplorer / SearchPanel / GitPanel 四个视图顶部只剩一条横线；拖拽 4 条分割条仍生效、hover 有主色高亮。
- 若接受不了某处「header 与内容贴合」的观感，可单点回退该视图的 `border-bottom`（四处彼此独立）。
