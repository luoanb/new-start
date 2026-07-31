# Lifecycle / 生命周期: agent-app-layout-refactor

```yaml
status: done
result: success
created_at: 2026-07-30 20:00
updated_at: 2026-07-31 14:50
owner: user
```

## Current Summary / 当前摘要

- 批准状态：已批准并完成
- 当前状态：全部实现完成，0 errors 通过验证
- 当前核心目标：将 agent-app GUI 布局重构为 VS Code 级别（Activity Bar / 可拖拽 Splitter / 折叠 / Dock / 布局持久化）
- 本迭代已完成

## Execution Log / 执行记录

1. 2026-07-30 20:00: 创建需求分析文档，系统梳理 VS Code 布局机制（Resize / Collapse / Dock / Persist）。
2. 2026-07-30 20:30: 用户确认 Q1-Q4 决策：Activity Bar 做 / Main split 做 / Logs 仅占位 / localStorage + LayoutStorage 抽象封装。
3. 2026-07-30: 创建技术方案（exec-scheme-bridge.md），确定方案 C：纯 Svelte 自研布局引擎，零新增依赖。
4. 2026-07-31 14:40: 基础设施 8 个文件落地。
   - `src/lib/layout/layoutTypes.ts`：LayoutState 模型 + DEFAULT_LAYOUT + BOUNDS + clamp
   - `src/lib/layout/layoutStorage.ts`：`LayoutStorage` 接口 + `LocalStorageLayoutStorage`（version 校验 + 浅合并）
   - `src/lib/layout/LayoutStore.svelte.ts`：模块级 `$state` store，`setXxx(persistNow)` 拖动中免写 / 拖动结束持久化
   - `src/lib/layout/useResizable.svelte.ts`：pointer 拖拽 + rAF 节流 hook
   - `src/lib/layout/Splitter.svelte`：通用分割条（vertical/horizontal + extraClass）
   - `src/lib/layout/DockPane.svelte`：分区容器（header + body + 折叠按钮）
   - `src/lib/layout/ActivityBar.svelte`：48px icon 轨 + 激活指示条
   - `src/lib/layout/views.ts`：视图元数据注册表（activityItems / panelViews）
5. 2026-07-31 14:45: `+page.svelte` 重构完成。
   - grid 重排为 `activity | status / main / panel / error` 四区；main-area 内为 Sidebar + Splitter + Chat + Splitter + Info 弹性行
   - 快捷键扩展：Ctrl+B sidebar / Ctrl+I info / Ctrl+Shift+J panel / Ctrl+\ 切 neuron-chat split / Ctrl+J 新建会话
   - `SidePanel` 移除 poller tab（PollerPanel 迁入底部 Panel dock，tab 为 Poller/Logs）
   - StatusBar 切换按钮：桌面走 layoutStore、移动端走 drawer
6. 2026-07-31 14:50: 验证通过。
   - `pnpm check`：0 errors（9 warnings，均为既有 a11y / 初始值捕获提示）
   - 浏览器实测（Chromium 142）：Activity Bar 渲染、Neurons 分栏切换、Panel tab 切换、Sidebar 分割条拖拽（260→180px）、Neurons 分割条拖拽（ratio 0.49→0.62，分割条随动）全部生效
   - 修复 2 个实测 bug：
     - `main-split` grid 无单位 `var(--split-ratio)` 被解析为 px 而非 fr，且 Chromium 不支持 calc() 内 fr 乘法 → 改为百分比乘法 `calc(var(--split-ratio) * (100% - 4px))`
     - `class:` 指令与 `class={extraClass}` 冲突（attribute_duplicate）→ 改为字符串拼接 class
   - 2026-07-31 15:10: 修复拖拽方向问题（用户反馈"右面板拖动/缩放方向相反"）：
      - Info 面板分割条在面板左侧，向右拖应使面板变窄 → `setInfoWidth(width - delta)`
      - `useResizable` 原以"dx 非零即取 dx"选择方向，导致水平分割条垂直拖动时被水平抖动干扰 → 增加显式 `axis: "x" | "y"`，由 Splitter 按 orientation 传入
    - 2026-07-31 15:20: 修复拖拽跟手性（用户反馈"分割线与鼠标对不上 / 莫名选中文本"）：
      - 根因：浏览器原生 drag 手势与文本选择会吞掉 pointermove/pointerup 事件流，导致分割线失联
      - `pointerdown` 增加 `preventDefault` + 仅左键 + `setPointerCapture`
      - move/up/cancel 挂到 window 级监听（双保险，移出分割条仍持续跟踪）
      - 拖动期间 body 挂 `resizing` class：全局 `user-select: none`，拖过文本区不再选中
      - Splitter 自身加 `user-select: none`
    - 2026-07-31 15:30: 主区 tab strip + 底部栏默认可见（用户反馈"看不到分割线 / 主区应有 tab / 底部栏没显示"）：
      - 分割条默认显示边框色细线（hover 主色），不再透明
      - 新增 `EditorTabs.svelte`：VS Code editor group 风格主区 tab 栏（split 时 [Chat][Neurons] 并排可 ✕ 关闭；单视图显示当前 tab）
      - split 状态真源迁移：`isNeuronSplit` 改为 `main.splits.length > 0`（原依赖 `activity.active`，与数据耦合不干净）；`ensureNeuronSplit` 重构为 `toggleNeuronSplit`，`handleActivitySelect` 同步理顺
      - 新增单 neuron 视图分支（关闭 Chat tab 后保留）
      - 布局 version 1→2：底部 Panel 默认展开（对齐 VS Code 底部栏习惯），旧 localStorage 布局自动重置
     - 2026-07-31 15:40: 修复底部 Panel 拖拽方向（用户反馈"底部栏拖动方向也是反的"）：分割条在面板顶部，向上拖应使面板变高 → `setPanelHeight(height - delta)`。至此四处方向统一为"分割条移动方向 = 面板边缘移动方向"（sidebar + / info - / panel - / main-split +）

## Out of Scope / 未处理

- Logs 视图：仅占位（Q3 决策）
- Panel 自由拖拽停靠（drag-dock）：P2，本期不做
- Neuron 网络可视化 SvelteFlow 集成：由 `2026-07-30_22-59_neuron-graph-viz` 迭代承接
- Rust 后端：本期零改动

## Follow-up / 后续修补

- 2026-07-31: Wayland + fcitx5 下 IME 候选窗偏移（越靠下越大）。`display: contents` 改块级无效。用户 A/B：`GDK_BACKEND=x11` 与 `env -u GTK_IM_MODULE` 均基本正确。已在 `src-tauri/src/main.rs` 于 GTK 初始化前、Wayland 且未强制 x11 时 `remove_var("GTK_IM_MODULE")`。详见 `docs/sdd-lab/2026-07-31_23-49_agent-app-wayland-ime/micro-spec.md`。**待用户普通 `pnpm tauri dev` 复测**。
