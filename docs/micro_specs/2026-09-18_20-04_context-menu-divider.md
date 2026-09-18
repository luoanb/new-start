# Spec: ContextMenu 真分隔符项（非交互）

## Goal

- 要解决什么问题：main 区 tab 右键菜单的「分隔线」是用一个 `label: "—"` 的普通菜单项模拟的——它会 hover 变色、可点击、点击后关闭菜单，语义与视觉都不是分割线；同菜单出现多个还会撞 `item.label` key。
- 验收结果：`ContextMenu` 支持一种**独立的分隔符条目**，渲染为不可 hover / 不可点击 / 不可聚焦的 hairline；tab 右键菜单改用该条目。

## Done Contract

- 什么算完成：`ContextMenu` 导出分隔符条目类型并在渲染层分流；tab 右键菜单不再有 `"—"` 占位项；分隔符无 hover 反馈、不可点击、不进 tab 焦点序列。
- 由什么证明：`pnpm --filter pulsar-app check` 0 error；App 内 tab 右键菜单目视确认两组之间为细线，鼠标移上去无高亮、点击不关闭菜单。
- 哪些情况仍算未完成：分隔符仍能 hover / 点击；仍保留 `"—"` 占位项；其它 `ContextMenu` 调用方类型报错。

## Scope

- In：`packages/pulsar-app/src/lib/components/ContextMenu.svelte`（条目类型 + 渲染分流 + CSS）；`packages/pulsar-app/src/lib/layout/EditorTabs.svelte`（替换 `"—"` 占位）。
- Out：菜单定位高度估算修正（`items.length * ITEM_H` 的既有偏差）；其它调用方新增分隔线；i18n 文案（分隔符无 label，不需要 key）。

## Facts / Constraints

- `ContextMenu.svelte` 当前只支持单一形态：所有 `items` 一律渲染为 `<button class="ctx-item">`，无 divider 分支。
- 触发方：[EditorTabs.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/layout/EditorTabs.svelte#L180-L185) 第 180-185 行 `items.push({ label: "—", icon: "", onSelect: () => {} })`。
- 调用方共 6 处：EditorTabs、FileExplorer、GitPanel、SessionList、Select、MultiSelect；除 EditorTabs 外均未使用占位分割项。
- `ContextMenuItem[]` 必须仍可赋值给新的条目数组类型（数组协变），避免改动其它调用方。
- `.ctx-menu` 为 `flex column` + `gap: 1px`，分隔符直接作为 flex 子项即可。

## Open Questions

- [x] Q1 类型表达：**判别联合** `ContextMenuEntry = ContextMenuItem | ContextMenuDivider`（`ContextMenuDivider = { divider: true }`），而非给 `ContextMenuItem` 加可选 `divider` 标志——后者会把 `label` 变可选，削弱普通项的类型约束，且所有调用方无需改动。
- [x] Q2 渲染形态：`<div class="ctx-divider" role="separator">`，非 button，天然不可聚焦 / 不可点击。

## Restated Understanding

- 我理解当前任务是：把右键菜单里「假装是分割线」的菜单项，换成组件层真正支持的分隔符条目。
- 当前核心目标是：分隔符在视觉与行为上都只是分隔线，不带任何交互语义。
- 当前边界是：只动 `ContextMenu` 的类型/渲染/CSS 与 EditorTabs 的调用点；不改菜单定位算法、不给其它菜单加分隔线。
- 暂不处理：`items.length * ITEM_H` 的高度估算偏差。

## Checkpoint Summary

- 当前任务理解：ContextMenu 新增独立分隔符条目，tab 右键菜单改用之。
- 当前核心目标：分隔符不可交互。
- 当前进度：实现完成，静态检查通过，待 App 内目视确认。
- 涉及文件 / 模块：`packages/pulsar-app/src/lib/components/ContextMenu.svelte`、`packages/pulsar-app/src/lib/layout/EditorTabs.svelte`。
- 风险：`{#each}` 的 key 由 `item.label` 改为下标（分隔符无 label）；`items` 公共类型由 `ContextMenuItem[]` 放宽为 `ContextMenuEntry[]`，现有调用方靠数组协变保持兼容。
- 验证方式：`pnpm --filter pulsar-app check`；App 内 tab 右键目视 + 鼠标悬停 / 点击分隔符。
- Execution Approval: `Approved`（2026-09-18，用户确认「单独做一个分隔符的 item」）

## Change Log

- 2026-09-18: 初始记录并实现。与 spec 的一处偏差（Reverse Sync 记录）：
  1. `{#each}` 的 key 由原计划的「保持 label」改为**下标 `(i)`**——分隔符条目无 `label`，且原 `(item.label)` 在同名项下本就有冲突风险。
  另：`ContextMenuEntry` 顺带在 `EditorTabs.svelte` 的菜单状态与 `items` 局部变量两处替换类型标注（原为 `ContextMenuItem[]`）。

## Validation

- Self-check: 已实现（类型 + 渲染分流 + CSS + 调用点替换）。
- Static checks: `pnpm --filter pulsar-app check` → 0 errors / 20 warnings（与既有基线一致，`ContextMenu.svelte` 无新增问题）。
- Runtime / Test: 未跑自动化（纯 UI 展示层）。
- Human confirmation: 待用户 App 内确认。
- 核心目标是否已由证据证明完成：否（差人工目视确认）。

## Resume / Handoff

- 当前状态：实现完成，静态检查通过，待人工确认。
- 下一步唯一动作：App 内右键任意 tab，确认两组菜单项之间为一条细线，且鼠标移到该线无高亮、点击不关闭菜单。
