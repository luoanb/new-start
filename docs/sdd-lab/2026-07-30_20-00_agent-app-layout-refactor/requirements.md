# Requirements / 需求分析: agent-app-layout-refactor

## Restated Understanding / 需求复述

- 我理解当前需求是：将 agent-app 的 GUI 布局从固定 CSS Grid 升级为 **VS Code 级别的可交互布局系统**。核心不是"更漂亮的界面"，而是"布局本身成为可操作的用户界面"——可拖拽调宽、可折叠、可持久化、支持多视图并存。
- 当前核心目标是：定义一个分层、可执行、有明确验收标准的布局重构需求，作为后续技术方案和实施的依据。
- 当前边界是：只涉及前端布局系统（Svelte 组件 + 布局状态），不涉及 Rust 后端改动；不改变现有视图的内容逻辑（SessionList/ChatArea/SidePanel/NeuronManager 只是被重新安置）。
- 暂不处理：主题/视觉再设计（上次已做）、字体排版系统、图表可视化（Neuron 网络已用 SvelteFlow）。

## 现状盘点 / Current State

### 当前布局事实（代码证据）

| 区域 | 现状 | 证据 |
|------|------|------|
| StatusBar | 顶部通栏，auto 高度 | [+page.svelte](file:///home/lab/Documents/trae_projects/new-start/packages/agent-app/src/routes/+page.svelte#L294-L310) `grid-template-areas: "status status status"` |
| Sidebar (SessionList) | 左侧，260px 固定宽，有 collapsed 折叠态（48px） | [SessionList.svelte](file:///home/lab/Documents/trae_projects/new-start/packages/agent-app/src/lib/components/SessionList.svelte#L111-L113) |
| ChatArea | 中央 1fr | grid-area chat |
| Info (SidePanel) | 右侧，280px 固定宽 | `.info-area { width: 280px }` |
| NeuronManager | **替换** ChatArea（互斥，不能并存） | `{#if showNeuronView}` |
| Error banner | 底部通栏，auto | grid-area error |
| 移动端 | Drawer 抽屉（300px 覆盖层） | `.drawer-backdrop / .drawer` |

### 布局系统的问题清单（按严重度）

1. **不可调整** — 侧栏/信息栏宽度全部硬编码（260px / 280px），用户无法拖拽，小屏内容被截断。
2. **不可折叠（桌面端）** — SessionList 有 collapsed 但无触发入口；Info 区完全不可折叠，用户只能看到 provider 列表时白占 280px。
3. **无底部 Panel** — 没有 VS Code 的 Terminal/Output 区域。Poller 状态、日志、错误事件流无处安放。
4. **视图互斥** — 聊天和神经元管理只能二选一，无法"边看神经元网络边聊天"。
5. **布局状态不持久化** — 每次启动回到默认布局，用户调整过的宽度/折叠全部丢失。
6. **键盘导航缺失** — 除 Ctrl+J（新建会话）外无布局快捷键（VS Code: Ctrl+B 侧栏、Ctrl+J 面板、Ctrl+` 终端）。
7. **分区无标题栏 chrome** — VS Code 每个分区有 icon+title+操作按钮，当前分区只有内容。
8. **分割条缺失** — 没有可拖拽的 visual splitter，区域边界是死的。

## VS Code 布局系统解剖 / Anatomy of VS Code Layout

### 区域划分（Region Model）

```
┌────────────────────────────────────────────────────────┐
│ Title Bar（菜单/命令中心）                               │
├──────┬─────────────────────────────────────┬───────────┤
│ Act- │  Editor Area（可多 tab / 可 split）  │  Pri-    │
│ ivity│                                     │  mary    │
│ Bar  │  Tab1 │ Tab2 │   ← split 成 grid    │  Side    │
│(icon)│       │      │                      │  Bar     │
│      ├─────────────────────────────────────┤  (可切    │
│      │  Editor Group（多组并存）            │  右移)    │
│      ├─────────────────────────────────────┤          │
│      │  Panel（底部 dock，可移右/顶/浮动）   │          │
│      └─────────────────────────────────────┴───────────┤
│ Status Bar（状态区）                                     │
└────────────────────────────────────────────────────────┘
```

### 核心机制（4 大件）

1. **Resize** — 任意两个区域之间有 drag handle，拖动调整宽/高，带最小宽度约束（侧栏 ~170px），双击 handle 重置。
2. **Collapse** — 每个区域可一键收起（快捷键 + 图标按钮），收起后仍保留一个窄条或仅剩 activity bar 图标。
3. **Dock / Move** — Panel 可以停靠在底部/右侧/顶部，Editor tab 可以跨 group 拖动。
4. **Persist** — 布局状态（每个区域的 width/height/visible/activeView/split 结构）序列化为 JSON，按窗口持久化。

### 交互细节（容易被忽略但构成专业感的部分）

- **分割条 hover 高亮**、拖动时有对齐辅助（是否贴合某参考线）
- **折叠动画**（宽度过渡，非瞬变）
- **分区标题栏**：icon + title + 右侧 action 按钮（折叠/关闭/菜单）
- **快捷键**：Ctrl+B 侧栏、Ctrl+J Panel、Ctrl+\ 分屏、Ctrl+Tab 切换视图
- **空态**：区域收起后工具栏图标仍可点击重新展开
- **溢出行为**：内容过多时区域内独立滚动，不影响其他区域

## 逐区需求分析 / Item-by-Item Requirements

以下按"需求点 → 现状 → 目标 → 验收"逐条分析，映射到 agent-app 的具体视图资产。

### R1. 可拖拽分割条（Splitter）

- **现状**：无分割条，宽度硬编码 260/280px。
- **目标**：Sidebar 与 Chat、Chat 与 Info 之间、底部 Panel 上方都有可拖拽 handle。
- **交互**：
  - hover 时 handle 高亮（主色竖线/横线）
  - 拖动实时更新宽度，带 `min`/`max` 约束（Sidebar 120~400px，Info 160~480px，Panel 100~60%）
  - 双击 handle 重置为默认宽度
  - 拖动结束（pointerup）时持久化宽度
- **验收**：拖动后宽度正确、不越界、不引起布局抖动；刷新/重启后宽度保持。

### R2. 区域折叠与恢复

- **现状**：Sidebar 有 collapsed class 但无入口；Info 不可折叠。
- **目标**：Sidebar / Info / Panel 三个区域都可折叠。
  - 折叠 Sidebar → 保留窄条（48px）可点图标展开（复用现有 48px 机制）
  - 折叠 Info → 完全隐藏，StatusBar 或 Activity Bar 图标可重新打开
  - 折叠 Panel → 完全隐藏（VS Code 行为），快捷键/按钮展开
- **快捷键**：`Ctrl+B` 侧栏、`Ctrl+J` Panel、`Ctrl+I` Info。
- **验收**：三个区域独立折叠/展开、动画过渡、状态持久化。

### R3. 底部 Panel（新区域）

- **现状**：无底部区域，Poller 状态被塞在右侧 SidePanel 的 tab 里。
- **目标**：新增底部 Panel dock，容纳"输出/日志/事件流"类视图。
- **初始视图**：
  - **Poller**（PollerPanel 从 SidePanel 移入）
  - **Logs**（预留：Gateway 事件/模型调用日志，本期可先占位）
- **交互**：Panel 可拖高（100~60%）、可折叠、可切换内部 tab；StatusBar 有快速开关按钮。
- **验收**：Panel 正常渲染 Poller 状态，拖拽调高、折叠、tab 切换均可用。

### R4. 多视图并存（Editor Area Split）

- **现状**：ChatArea 与 NeuronManager 互斥（`{#if showNeuronView}`）。
- **目标**：中央区域支持**并排 split**——聊天和神经元网络同时可见。
- **交互**：
  - Activity Bar / 视图切换时，若当前主区已被占用，新视图以 split 方式并排打开（不粗暴替换）
  - 每个 split 有 tab 标题栏，可关闭
  - 支持水平/垂直 split（本期至少支持水平）
- **验收**：聊天 + 神经元网络并排显示；各自独立滚动；关闭 split 后布局回收。

### R5. 布局状态持久化

- **现状**：无任何布局持久化。
- **目标**：布局状态序列化为 JSON，存 `localStorage`（与现有 `agent-app:providerId` 模式一致）。
- **状态结构**（草案）：
  ```json
  {
    "version": 1,
    "sideBar":  { "visible": true, "width": 260, "activeView": "sessions" },
    "infoBar":  { "visible": true, "width": 280, "activeView": "topics" },
    "panel":    { "visible": false, "height": 200, "activeView": "poller" },
    "main":     { "splits": [{ "id": "chat" }] }
  }
  ```
- **时机**：拖动结束/折叠切换时立即写入；启动时读取并应用；`version` 字段防 schema 升级冲突。
- **验收**：调整布局 → 重启 → 布局完整恢复。

### R6. 键盘导航

- **现状**：仅 Ctrl+J 新建会话。
- **目标**：布局快捷键全集（见 R2）+ 视图切换快捷键。
- **验收**：全部快捷键生效且不冲突。

### R7. 分区 Chrome（标题栏 + 操作按钮）

- **现状**：分区无标题、无操作按钮。
- **目标**：Sidebar / Info / Panel 各自带 32px 高的分区标题栏：`[icon] Title .... [collapse] [close]`。
- **验收**：三个分区标题栏渲染正确，按钮可用。

## 分层需求 / Prioritized Requirements

### P0（核心体验，必做）
- R1 可拖拽分割条
- R2 区域折叠与恢复（含快捷键 Ctrl+B / Ctrl+J / Ctrl+I）
- R5 布局状态持久化

### P1（专业感，重要）
- R3 底部 Panel（Poller 迁入 + Logs 占位）
- R4 主区水平 split（聊天 + 神经元并存）
- R7 分区标题栏 chrome

### P2（进阶，可延后）
- Activity Bar（左侧 icon 轨，收纳会话/神经元/信息入口）
- Panel 位置移动（底/右/顶 dock 切换）
- 双击 splitter 重置、对齐辅助线
- Zen Mode（纯聊天沉浸模式）

## 技术路线 / Technical Approach

### 方案对比

| 方案 | 说明 | 优点 | 缺点 |
|------|------|------|------|
| A. 引入 dock 库 | dockview（React 系）、flexlayout-react、svelte-dock | 功能全（drag dock/浮动/持久化） | Svelte 生态成熟 dock 库几乎不存在；React 库需包装，体积大、心智负担重 |
| B. 自研轻量布局引擎 | Splitter 组件 + 布局状态 store + 分区容器 | 完全可控、无依赖、贴合需求 | 需自行处理 resize 边界、动画、持久化（工作量约 1 个组件系统） |
| C. 半自研（推荐） | 自研 Splitter + 折叠 + 持久化；**不做**自由 drag-dock（Panel 位置固定底部，不做移动） | 覆盖 VS Code 95% 日常体验，开发量可控 | 缺面板自由移动（对 chat 应用影响小） |

### 推荐：方案 C

理由：
1. **Svelte 生态无成熟 dock 库**，硬上 React 库是技术债。
2. VS Code 布局体验的**核心价值** = resize + collapse + 快捷键 + persist + 多视图并存，这些自研完全可控。
3. Panel 自由 drag-dock（拖到任意边缘）开发成本极高、收益低——chat 应用里底部 Panel 固定是最常见形态（Slack/Discord/Notion 均如此）。
4. 现有代码已用 CSS grid + 明确 grid-area，改造为"宽度/高度变量 + 动态 grid-template"的增量路径清晰。

### 基础设施组件（草案）

```
src/lib/layout/
├── LayoutStore.svelte.ts      // 布局状态（$state + persist 到 localStorage）
├── Splitter.svelte            // 通用拖拽分割条（horizontal/vertical）
├── DockPane.svelte            // 分区容器（标题栏 + 内容 + 折叠按钮）
├── layoutTypes.ts             // LayoutState 类型 + 默认值 + 版本迁移
└── useResizable.svelte.ts     // pointer 拖拽逻辑 hook（复用性）
```

现有组件适配（仅改变安置方式，不改内容逻辑）：
- `SessionList` → Sidebar dock 的 activeView
- `SidePanel` → Info dock（tab 保留）
- `PollerPanel` → 迁入 Panel dock
- `ChatArea` / `NeuronManager` → Main area 的 split 视图

## 风险与边界 / Risks & Boundaries

- **风险1 — 拖拽性能**：splitter 拖动需用 `pointermove` + `requestAnimationFrame` 节流，避免频繁触发 svelte 更新。缓解：局部 state 更新宽度，拖动结束后统一写入 store。
- **风险2 — 布局状态 schema 演进**：新增区域导致旧状态不兼容。缓解：`version` 字段 + 默认值合并（diff 式 merge，而非整体覆盖）。
- **风险3 — 移动端**：当前 drawer 方案保留，桌面 splitter 在 <800px 不启用（responsive 降级）。
- **边界**：不改 Rust 后端；不改视图内部逻辑；不引入新 npm 依赖（方案 C 纯自研）。

## Open Questions / 开放问题

- [x] Q1: Activity Bar 本期是否做？→ **做**。哪些视图进 icon 轨：Sessions（会话）/ Neurons（神经元）/ Chat（主区）/ Info（信息区开关）
- [x] Q2: Main area split 本期是否必须？→ **做**。聊天 + 神经元网络并排
- [x] Q3: Logs 视图？→ **仅占位**（空态 + 标题，不接真实事件流）
- [x] Q4: 布局持久化？→ **localStorage，但抽象封装**（`LayoutStorage` 接口，便于未来迁移到后端文件存储）

## Requirement Decisions / 需求决策

- 2026-07-30: 需求分析初稿创建，用户确认 Q1-Q4 决策（Activity Bar 做 / split 做 / Logs 占位 / localStorage+抽象）。
- 2026-07-30: 确认后范围 = **P0 全部 + P1 全部 + Activity Bar**。Panel 位置移动仍为 P2（不做拖拽，改为配置项切换）。
- 2026-07-31: 实现完成并通过验证（pnpm check 0 errors；浏览器实测 ActivityBar / 分栏 / Panel tab / 分割条拖拽全部生效）。实施偏差记录：
  - 外层 grid 采用 `activity | status/main/panel/error` 4 区 + main-area 内 flex 行（sidebar/splitter/chat/splitter/info），而非方案 3.9 的 5 列 grid——效果等价且更简单
  - `main-split` 比例用百分比乘法 `calc(var(--split-ratio) * (100% - 4px))`（Chromium 不支持 calc 内 fr 乘法）
  - Poller 从 SidePanel 迁入底部 Panel dock（Q3 Logs 占位为 tab），SidePanel 保留 4 个 tab
  - 详细执行记录见本目录 `lifecycle.md`
