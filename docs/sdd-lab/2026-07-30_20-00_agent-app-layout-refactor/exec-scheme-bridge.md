# Exec Scheme Bridge: agent-app-layout-refactor

## 1. 改动依赖范围内的能力与代码现实

**范围**：仅本次实现会调用或必须修改/扩展的仓库能力。

| 能力 | 现状 | 证据 |
|------|------|------|
| 主布局容器 | 固定 CSS grid，3 行 3 列 | [+page.svelte](file:///home/lab/Documents/trae_projects/new-start/packages/agent-app/src/routes/+page.svelte#L294-L310) `.app-layout` |
| Sidebar 折叠态 | SessionList 已有 `collapsed`（48px），但无触发入口 | [SessionList.svelte](file:///home/lab/Documents/trae_projects/new-start/packages/agent-app/src/lib/components/SessionList.svelte#L112) |
| 键盘快捷键 | `handleKeydown` 仅处理 Ctrl+J 新建会话、Escape 关抽屉 | [+page.svelte](file:///home/lab/Documents/trae_projects/new-start/packages/agent-app/src/routes/+page.svelte#L159-L168) |
| 本地持久化模式 | `localStorage.setItem("agent-app:providerId", ...)` 直接调用 | [+page.svelte](file:///home/lab/Documents/trae_projects/new-start/packages/agent-app/src/routes/+page.svelte#L155) |
| 视图互斥 | `{#if showNeuronView}` 替换 ChatArea | +page.svelte main 区域 |
| PollerPanel | 在 SidePanel 的 tab 内 | [SidePanel.svelte](file:///home/lab/Documents/trae_projects/new-start/packages/agent-app/src/lib/components/SidePanel.svelte#L99-L100) |
| Svelte 模块级 state 模式 | i18n 已用 `$state` 模块级 rune | i18n/index.svelte.ts |
| 新增依赖 | 无 dock/splitter 库（Svelte 生态无成熟方案） | package.json |

## 2. 外部依赖：包与本任务用到的精确 API

| 包（版本） | 本任务依赖的具体 API | 备注 |
|------------|----------------------|------|
| `@tauri-apps/api` (lockfile) | 无新增 | 布局纯前端 |
| **无新增 npm 依赖** | — | 方案 C：自研 splitter/dock/activitybar |

## 3. 设计契约

**技术文档出处**：`docs/sdd-lab/2026-07-30_20-00_agent-app-layout-refactor/requirements.md`

**契约正文**：

### 3.1 目录结构（新增）

```
src/lib/layout/
├── layoutTypes.ts           // LayoutState / DockId / LayoutView 类型 + 默认值
├── LayoutStore.svelte.ts    // 模块级 $state store + 持久化接入
├── layoutStorage.ts         // LayoutStorage 接口 + LocalStorageLayoutStorage 实现
├── useResizable.svelte.ts   // pointer 拖拽 hook（rAF 节流）
├── Splitter.svelte          // 通用分割条（h/v 两向）
├── DockPane.svelte          // 分区容器（标题栏 + 内容 + 折叠）
├── ActivityBar.svelte       // 左侧 icon 轨
└── views.ts                 // 视图注册表（id → 组件 + 默认 dock）
```

### 3.2 状态模型（layoutTypes.ts）

```typescript
export type DockId = "sidebar" | "info" | "panel" | "main";

export type SplitOrientation = "horizontal" | "vertical";

export type MainSplit = {
  id: string;             // "chat" | "neuron" | "logs"
  orientation: SplitOrientation;
  ratio: number;          // 第一个 pane 占比 0.3~0.7
};

export type LayoutState = {
  version: 1;
  sidebar: { visible: boolean; width: number };
  info:    { visible: boolean; width: number };
  panel:   { visible: boolean; height: number; activeView: string };
  main:    { splits: MainSplit[] };          // 空数组 = 单一视图
  activity: { active: string | null };        // "sessions" | "chat" | "neurons" | "info"
};
```

默认值：

```typescript
export const DEFAULT_LAYOUT: LayoutState = {
  version: 1,
  sidebar: { visible: true, width: 260 },
  info:    { visible: true, width: 280 },
  panel:   { visible: false, height: 200, activeView: "poller" },
  main:    { splits: [] },
  activity: { active: "sessions" },
};
```

### 3.3 存储抽象（layoutStorage.ts）

```typescript
// 面向未来迁移的接口——实现可换为后端文件存储（如 tauri 的 fs 插件）
export interface LayoutStorage {
  load(): LayoutState | null;
  save(state: LayoutState): void;
}

export class LocalStorageLayoutStorage implements LayoutStorage {
  private readonly key = "agent-app:layout";
  load(): LayoutState | null { /* JSON.parse + version 校验 + DEFAULT_LAYOUT diff-merge */ }
  save(state: LayoutState): void { /* JSON.stringify */ }
}
```

**version 演进策略**：`load()` 读取后与 `DEFAULT_LAYOUT` 做**浅合并**（缺字段补默认值），未知字段丢弃。未来存储迁移只需换 `LayoutStorage` 实现，store 零改动。

### 3.4 LayoutStore（LayoutStore.svelte.ts）

模块级 `$state`（与 i18n 模式一致）：

```typescript
import { $state } from "svelte";

const storage: LayoutStorage = new LocalStorageLayoutStorage();
const state = $state<LayoutState>(storage.load() ?? DEFAULT_LAYOUT);

// 变更即持久化：统一走 setter
export const layoutStore = {
  state,
  toggleSidebar() { state.sidebar.visible = !state.sidebar.visible; persist(); },
  toggleInfo()    { state.info.visible = !state.info.visible; persist(); },
  togglePanel()   { state.panel.visible = !state.panel.visible; persist(); },
  setSidebarWidth(w: number) { state.sidebar.width = clamp(w, 120, 400); persist(); },
  setInfoWidth(w: number)    { state.info.width = clamp(w, 160, 480); persist(); },
  setPanelHeight(h: number)  { state.panel.height = clamp(h, 100, 0.6 * innerHeight); persist(); },
  setMainSplits(s: MainSplit[]) { state.main.splits = s; persist(); },
  setActivity(id: string | null) { state.activity.active = id; persist(); },
};
```

持久化时机：**拖动结束 / 折叠切换后**（不是拖动过程中），避免高频写 localStorage。

### 3.5 Splitter 组件

```svelte
<script lang="ts">
  let { orientation = "vertical", onResize }: {
    orientation?: "vertical" | "horizontal";
    onResize: (deltaPx: number) => void;
  } = $props();
</script>

<!--
  结构: 2px 宽的绝对定位 handle
  逻辑: useResizable → pointerdown 开始 → pointermove 累加 delta → rAF 节流回调 onResize → pointerup 结束（触发 persist）
  hover: 高亮 var(--color-primary)
  移动端 (<800px): 不渲染（media query 隐藏），维持 drawer 方案
-->
```

### 3.6 DockPane 组件（分区 chrome）

```svelte
<script lang="ts">
  let { icon, title, collapsible = true, collapsed, onToggle, children }: {
    icon?: string; title: string;
    collapsible?: boolean; collapsed: boolean; onToggle: () => void;
    children?: Snippet;
  } = $props();
</script>

<!--
  header: [icon] Title ......... [collapse btn]
  body:   <slot />
  collapsed 时 body 隐藏，header 保留（或整体收起）
-->
```

### 3.7 ActivityBar 组件

icon 轨（左侧 48px 竖列），4 个入口：

| icon | id | 行为 |
|------|-----|------|
| 💬 | sessions | 切换 sidebar 显隐（VS Code Ctrl+B 行为） |
| 🖥 | chat | 主区聚焦聊天（若 neuron split 存在则关闭 split） |
| 🧠 | neurons | 主区打开 neuron split（chat + neuron 并排） |
| ⓘ | info | 切换 info bar 显隐 |

激活态高亮（左侧 2px 主色指示条）。点击已激活的 icon 折叠对应区域。

### 3.8 视图注册表（views.ts）

```typescript
import type { Component } from "svelte";
import SessionList from "$lib/components/SessionList.svelte";
import ChatArea from "$lib/components/ChatArea.svelte";
import NeuronManager from "$lib/components/NeuronManager.svelte";
import SidePanel from "$lib/components/SidePanel.svelte";
import PollerPanel from "$lib/components/PollerPanel.svelte";

export type LayoutView = {
  id: string;
  label: string;
  component: Component;
  defaultDock: "sidebar" | "info" | "panel" | "main";
};

export const views: Record<string, LayoutView> = {
  sessions: { id: "sessions", label: "Sessions", component: SessionList, defaultDock: "sidebar" },
  chat:     { id: "chat", label: "Chat", component: ChatArea, defaultDock: "main" },
  neuron:   { id: "neuron", label: "Neurons", component: NeuronManager, defaultDock: "main" },
  info:     { id: "info", label: "Info", component: SidePanel, defaultDock: "info" },
  poller:   { id: "poller", label: "Poller", component: PollerPanel, defaultDock: "panel" },
};
```

**注意**：`SidePanel` 目前内嵌 PollerPanel tab，重构后 Poller 迁到 Panel dock，SidePanel 保留其余 4 个 tab（providers/models/skills/topics）。视图注册表只负责"哪个组件放哪个 dock"，具体内容由组件自身管理。

### 3.9 主区布局（+page.svelte 重构）

主区渲染逻辑：

```svelte
<!-- main area -->
{#if main.splits.length === 0}
  <!-- 单一视图：activity.active 决定 -->
  {#if activity.active === "neurons"}
    <NeuronManager />
  {:else}
    <ChatArea {messages} {loading} onSend={handleSend} />
  {/if}
{:else}
  <!-- split: chat | neuron 并排 -->
  <div class="main-split" style="grid-template-columns: {ratio}fr {1-ratio}fr">
    <ChatArea ... />
    <Splitter orientation="vertical" onResize={...} />
    <NeuronManager />
  </div>
{/if}
```

外层 `app-layout` grid 改为动态（由 store 驱动）：

```css
.app-layout {
  display: grid; height: 100vh;
  grid-template-rows: auto 1fr auto;
  grid-template-columns: 48px auto 1fr auto;
  grid-template-areas:
    "activity status status status"
    "activity sidebar chat info"
    "activity panel  panel panel"
    "activity error  error error";
}
/* 折叠时: sidebar/info 列宽 = 0，panel 行高 = 0 */
```

### 3.10 键盘快捷键（+page.svelte 扩展 handleKeydown）

```typescript
function handleKeydown(e: KeyboardEvent) {
  if (e.ctrlKey && e.shiftKey) return; // 保留给未来
  if (e.ctrlKey && e.key === "j") { e.preventDefault(); showCreateModal = true; }
  if (e.ctrlKey && e.key === "b") { e.preventDefault(); layoutStore.toggleSidebar(); }
  if (e.ctrlKey && e.key === "j" && e.shiftKey) { e.preventDefault(); layoutStore.togglePanel(); }
  if (e.ctrlKey && e.key === "i") { e.preventDefault(); layoutStore.toggleInfo(); }
  if (e.key === "Escape") { /* 关抽屉 + 退出 neuron split 视情况 */ }
}
```

### 3.11 组件迁移清单

| 变更 | 文件 | 内容 |
|------|------|------|
| 新增 | src/lib/layout/*（8 个文件） | 见 3.1 |
| 修改 | +page.svelte | 布局改 store 驱动；Splitter/ActivityBar 接入；键盘扩展；PollerPanel 从 SidePanel 移出到 Panel dock |
| 修改 | SidePanel.svelte | 移除 poller tab（迁入 Panel） |
| 修改 | SessionList.svelte | 折叠入口接到 layoutStore.toggleSidebar（复用现有 collapsed 样式） |
| 新增 | 无后端改动 | — |

## 4. 相对技术文档的增量说明

| 项目 | 说明 |
|------|------|
| 沿用 | P0+P1 全部需求、Q1-Q4 决策 |
| 补充 — Panel 位置 | 固定底部（不拖拽），P2 再做配置切换 |
| 补充 — Activity Bar 行为 | 与 VS Code 对齐：icon 即"显隐切换"（点击激活 → 再点折叠） |
| 补充 — 视图互斥语义 | split 只有 chat\|neuron 两种；logs 仅在 panel 内占位 |
| 排除 | 面板 drag-dock、浮动窗口、Zen Mode（P2+） |

## 5. 验收验证方式

- `pnpm check` 0 error
- 手动验证：拖拽三处分割条、三区折叠/展开、Ctrl+B/J/I、ActivityBar 四入口、布局重启保持、移动端 (<800px) 仍走 drawer
