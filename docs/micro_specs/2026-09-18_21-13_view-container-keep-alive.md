# 视图容器保活：切换面板视图不再重建终端实例

- 日期：2026-09-18
- 状态：Approved（用户确认方案「视图保活」）
- 关联：`docs/specs/2026-08-20_10-00_terminal-panel.md`（集成终端面板）

## 问题

底部 panel 容器在 poller / logs / terminal 之间切换 tab 后切回终端，屏幕空白。后端 PTY 仍存活（cwd / 进程状态未变），属于观感上的「终端重启」。

## 根因

`ViewContainer.container-body` 用 `{#if activeView}` 条件渲染（`ViewContainer.svelte:248-254`）：切走时 `TerminalPanel` 被卸载，`onDestroy` 退订终端事件（`TerminalPanel.svelte:195-199`），xterm 实例被 `dispose`；切回重建为空白实例。卸载期间到达的输出经 `routeOutput`（`TerminalPanel.svelte:201-203`）直接丢弃，且无 buffer 回放。

前端不存在 tab 级 xterm 实例，后端会话表是进程级全局的，二者均与 tab 无绑定。

## 方案

视图注册表新增 `keepAlive?: boolean`，`terminal` 标记为保活。

`ViewContainer` 渲染改为「当前激活视图 + 已激活过的保活视图」共用同一个 `{#each}`（key = `reg.id`）：

- 保活视图：绝对定位叠满容器，非激活时 `visibility: hidden`（保留布局尺寸，避免 xterm 量到 0 尺寸）。
- 视图被 hide 或移出容器时，从保活集合清除并卸载。
- 非保活视图行为不变（失活即卸载）。
- 「首次激活 → 进入保活集合」不改变节点结构，避免多一次卸载重建。

## Done Contract

- 完成：panel 容器在 terminal ↔ logs/poller 之间来回切换，xterm 实例不重建，当前屏幕内容与滚动缓冲保留。
- 证明：`pnpm check` 通过；运行期手动冒烟（切走再切回终端，历史输出仍在）。
- 仍未完成：spawn / 输出 / 输入 / kill 任一链路回归，或其它容器视图布局出现偏差。

## 涉及文件

- `packages/pulsar-app/src/lib/layout/views.ts`：`ViewRegistration.keepAlive` + terminal 标记。
- `packages/pulsar-app/src/lib/layout/ViewContainer.svelte`：保活渲染与槽位样式。
- `packages/pulsar-app/src/lib/components/TerminalPanel.svelte`：面板内部 tab 的 xterm 实例保活。

## 风险

- 保活终端的 ResizeObserver 在容器隐藏时仍会 `fit`；FitAddon 对 0 尺寸返回 undefined、不改动 cols/rows，视为安全（既有行为，非本次新增）。
- 引入 `.view-slot` wrapper 改变 `container-body` 子元素层级，需确认各视图撑满行为不变。

## Change Log

- 2026-09-18：初稿并实施（`views.ts` + `ViewContainer.svelte`）；`pnpm check` 通过。
- 2026-09-18：补充同族问题 —— 终端面板**内部**多 tab 切换同样空白。`TerminalPanel.svelte` 原用 `{#key activeId}` 整块销毁重建 xterm，改为每个 tab 一个常驻实例（绝对定位叠放 + 非激活 `visibility: hidden`），并补「切 tab 焦点交还激活实例」。`pnpm check` 0 errors。

## Validation

- Static：`pnpm check` → 0 errors / 20 warnings（均为既有告警，改动文件无新增）。
- Runtime：待手动冒烟（panel 容器在 terminal ↔ logs/poller 间来回切换，历史输出保留）。
- 核心目标是否已由证据证明完成：否（待运行期冒烟）。
