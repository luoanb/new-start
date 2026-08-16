# Spec: window-edge-resize-cursor

## Goal

- 要解决什么问题：无边框 Tauri 窗口（`decorations: false`）在 Linux/WebKitGTK 下，窗口边缘可拖动缩放但**没有 resize 光标提示**（WM 不渲染无边框窗口的边缘光标）。
- 验收结果：鼠标悬停窗口四边显示 `ew-resize` / `ns-resize`、四角显示对角光标，移开恢复；拖动缩放能力不受影响。

## Done Contract

- 什么算完成：新增边缘热区组件（四边 4px + 四角 10px，固定定位、透明、`aria-hidden`），仅 Tauri 环境挂载；hover 显示正确 resize 光标；`pnpm build` 通过。
- 由什么证明：build 通过 + 用户在应用内实测悬停四边/四角光标变化、缩放仍正常、滚动条与面板分隔条交互不受影响。
- 哪些情况仍算未完成：光标未出现（WebKitGTK CSS cursor 不生效，需升级方案 B `setCursor`）；热区遮挡滚动条或内部边缘交互；非 Tauri 环境错误显示。

## Scope

- In:
  1. 新建 `src/lib/layout/WindowEdgeResize.svelte`：左/右 4px 竖条 `ew-resize`，上/下 4px 横条 `ns-resize`，四角 10px `nwse-resize` / `nesw-resize`；`position: fixed; z-index` 高于内容。
  2. `+page.svelte` 根层 `{#if isTauriEnv}` 挂载该组件。
- Out:
  - 后端 / 权限 / tauri.conf 无改动；不做方案 B（setCursor）；不动 StatusBar 拖拽逻辑；不做视觉高亮（仅光标）。

## Facts / Constraints

- 已确认事实：
  - 窗口 `decorations: false`（[tauri.conf.json L20](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/tauri.conf.json)），resizable 默认 true；边缘 resize 由系统/WM 接管（拖动正常）。
  - WebKitGTK 原生滚动条为 webview chrome 层，优先于 DOM 热区命中 → 热区不遮挡滚动条功能。
  - 项目已有 `isTauriEnv` 工具与 `$lib/layout/` 目录（Splitter 同域）。
- 技术/业务约束：
  - 热区必须 `pointer-events: auto` 才能触发 hover 光标，4px 宽度内无重要交互（Splitter 位于面板之间、不在窗口最外边缘）。
  - 热区透明、无 aria 文本（纯装饰光标提示，`aria-hidden="true"`）。
- 已知风险：
  - WebKitGTK 下 CSS cursor 可能不生效（若实测无效 → 升级方案 B `setCursor`，追加 `allow-set-cursor` 权限）。
  - 四角热区 10px 与边热区重叠处需保证对角光标优先（z-index 或 DOM 顺序）。

## Open Questions

- [x] 方案选择：CSS 边缘热区（用户确认，方案 A）。
- [ ] （执行后）WebKitGTK 实测 CSS cursor 是否生效：待验证，无效则按 Reverse Sync 回写并升级方案 B。

## Restated Understanding

- 我理解当前任务是：为无边框 Tauri 窗口补上系统缺失的边缘 resize 光标提示，用纯 CSS 四边/四角热区实现，仅 Tauri 环境生效。
- 当前核心目标是：最小改动（1 个新组件 + 1 处挂载）交付可用的边缘光标反馈。
- 当前边界是：纯前端、零权限、零后端改动；若 CSS cursor 在 WebKitGTK 不生效，才升级方案 B。
- 暂不处理：光标之外的边缘视觉提示、方案 B、纯浏览器环境的边缘提示。

## 接口契约设计

```svelte
<!-- WindowEdgeResize.svelte（自包含，无 props / 无事件） -->
<div class="window-edge-resize" aria-hidden="true">
  <div class="edge edge-top"></div>
  <div class="edge edge-bottom"></div>
  <div class="edge edge-left"></div>
  <div class="edge edge-right"></div>
  <div class="corner corner-tl"></div>
  <div class="corner corner-tr"></div>
  <div class="corner corner-bl"></div>
  <div class="corner corner-br"></div>
</div>

<!-- +page.svelte 根层 -->
{#if isTauriEnv}
  <WindowEdgeResize />
{/if}
```

```css
.window-edge-resize { position: fixed; inset: 0; z-index: 9999; pointer-events: none; }
.window-edge-resize > div { position: fixed; pointer-events: auto; }
/* 边：4px，光标 ew/ns-resize；角：10px，对角光标，后声明覆盖边 */
```

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是（已定位根因与方案）。
- 若否，偏差在哪里：无。
- 是否需要调整本轮目标或范围：否。

## Checkpoint Summary

- 当前任务理解：无边框窗口边缘缺 resize 光标提示，用 CSS 热区补上。
- 当前核心目标：1 个新组件 + 1 处挂载，交付边缘光标反馈。
- 当前进度：根因与方案已确认（用户选方案 A）。
- 下一步 1: 新建 `WindowEdgeResize.svelte`（结构 + 样式）。
- 下一步 2: `+page.svelte` 根层 `{#if isTauriEnv}` 挂载。
- 下一步 3: `pnpm build` 验证。
- 涉及文件 / 模块：`src/lib/layout/WindowEdgeResize.svelte`（新建）、`src/routes/+page.svelte`。
- 风险：WebKitGTK CSS cursor 可能无效（兜底方案 B）；热区遮挡滚动条（原生 chrome 优先，实测确认）。
- 验证方式：build + 用户应用内实测（四边/四角光标、缩放、滚动条、分隔条）。
- Execution Approval: `Approved`

## Change Log

- 2026-08-16: 创建 spec；根因（无边框 + WebKitGTK 不渲染边缘光标）与分析完成，用户选方案 A（CSS 热区）。
- 2026-08-16: 执行完成。新建 `WindowEdgeResize.svelte`（容器 `pointer-events:none` + 四边 4px `ew/ns-resize`（角部让出 10px）+ 四角 10px 对角光标，fixed z-index 9999，`aria-hidden`）；`+page.svelte` 根层 `{#if isTauriEnv}` 挂载。

## Validation

- Self-check: 热区指针事件（容器 none / 子项 auto）、边角重叠（边让位 10px 无光标冲突）、`aria-hidden` 均复查通过。
- Static checks: 两文件 diagnostics 均为空。
- Runtime / Test: `pnpm build` 通过（8.89s，adapter-static 产出成功）。
- Human confirmation: 待用户在应用内实测：①四边/四角光标提示；②边缘缩放仍正常；③滚动条/分隔条交互不受影响。
- 结果汇总：构建证据通过；CSS cursor 在 WebKitGTK 的实际生效性待人工实测确认。
- 核心目标是否已由证据证明完成：代码与构建层面已完成；运行时光标反馈待实测定论。
- 若未完成，当前剩余差距：若 WebKitGTK 下 CSS cursor 不生效，需按方案 B 升级（`setCursor` + `allow-set-cursor` 权限）。
- 剩余风险：中低——CSS cursor 生效性未实测；若无效按 Reverse Sync 回写本 spec 后升级方案 B。

## Resume / Handoff

- 当前状态：执行完成，代码与构建验证通过。
- 当前卡点：无（CSS cursor 生效性属实测验收环节）。
- 下一步唯一动作：用户运行应用实测边缘光标与缩放；如有偏差按 Reverse Sync 回写本 spec 后调整。
- 下一轮核心目标：按实测反馈收尾，或升级方案 B。
