# Visual Design / 设计文档: Workspace File Management

## Source / 来源

* 交互形态：来自迭代内与用户确认的交互设计（2026-08-16 ~ 2026-08-17），非 Figma 稿。
  * 确认项：右键菜单 + 快捷键（F2 / Delete）+ 顶部工具条；拖拽移动；删除直接删无确认；pad 端条目右侧 ⋮ 上下文按钮；多文件 tab（多实例）；系统对话框 + 输入回退选目录；per-workspace ignore 过滤。
* 设计规范来源（本项目现有事实，不新增体系）：
  * 全局 design tokens：[app.html](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/app.html) `:root` 与 `[data-theme="dark"]`（cool palette，hue 265；primary hue 260；字体 `--fs-xs/sm/base/lg/xl`；间距 `--space-1..16`；圆角 `--radius-sm/md/lg/full`；动效 `--ease-out` + `--duration-fast/normal`；`--color-hover` / `--color-text-muted` / `--color-border`）。
  * 侧栏容器 tab 形态：[ViewContainer.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/layout/ViewContainer.svelte)（header 32px、tab = icon + label、激活态 primary 底部 2px 下边框、⋯ 视图菜单、Pointer Events 拖拽换容器）。
  * main 区 tab 形态：[EditorTabs.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/layout/EditorTabs.svelte)（height 32px、激活态 surface 背景 + primary 顶部 2px 上边框、hover 显示 ✕ 关闭、HTML5 DnD 拖拽重排/新建分栏、`@media (hover: hover)` 触屏常显关闭按钮）。
  * 视图注册机制：[views.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/layout/views.ts)（`viewRegistry` / `mainViews` / `mainPanelMeta` / `activityItems`；icon 统一 14–16px inline SVG stroke currentColor）。
  * 按钮/输入词汇：`.btn` / `.btn-primary` / `.btn-danger` / `.btn-sm`、原生 `select` 主题适配（app.html 全局）。
  * 浮层模式：Select.svelte / ViewContainer 的 `⋯ 菜单`（`--color-elevated` 背景、`--radius-md`、`--border-width` `--color-border`、阴影、portal 到 body 防裁切）。

## Design Principles / 设计原则

* 沿用现有 cool palette 与按钮/输入/浮层词汇，不引入新视觉体系、不新增 design token。
* 文件树与编辑器共享同一「工作区」数据源与边界：树负责导航与结构管理，编辑器负责内容编辑；视觉密度对齐现有 sidebar 面板（ToolPanel / LogPanel 同风格）。
* 多实例 tab 语义对齐 VS Code 资源管理器：文件路径是 tab 实例唯一键，未保存状态独立于各 tab。
* 触屏与桌面并存：所有编辑操作在 `hover: none` 设备上通过条目右侧 ⋮ 按钮可达，不依赖右键。

## Page Design / 页面设计

### 1. 全局布局（sidebar 新增「文件」tab + main 区新增编辑器面板）

* sidebar 容器 tabs 增加 `files`：tab 形态沿用 ViewContainer 规则（height 32px、icon 12px + label `--fs-xs`、激活态 `--color-primary` + 底部 2px 下边框、悬停 `--color-text`）。
* main 区新增 `file-editor` 面板：走 `mainViews` + `EditorTabs` 语义，多实例（每个文件路径一个 tab），tab 标题 = 文件名（tooltip 完整路径），激活态 surface 背景 + primary 顶部 2px 上边框；关闭按钮 hover 显示（触屏常显，沿用现有行为）。
* 空态：sidebar「文件」视图无工作区时显示引导（「添加工作区」按钮 + 说明文案）；main 区无打开文件时编辑器面板不出现（由树点击触发）。

### 2. FileExplorer.svelte（sidebar「文件」视图）

```
┌─────────────────────────────┐
│ 工作区 [▶ 项目A ▾]      [+][⋯] │  ← 工作区工具条：下拉选择器 + 添加 + 操作
│ [↻ 刷新] [🗋 新建文件] [📁 新建文件夹] │  ← 树工具条
├─────────────────────────────┤
│ ▾ 📁 src                   │  ← 目录节点：折叠箭头 + 文件夹图标 + 名称
│   ▾ 📁 lib/                │  ← 子目录（缩进层级）
│     ▸ 📁 components         │
│     📄 types.ts       [⋯]  │  ← 文件节点 + 条目右侧 ⋮（触屏上下文入口）
│   📄 vite.config.ts         │
└─────────────────────────────┘
```

* 容器：`--color-surface` 背景；树区域 `overflow-y: auto`；节点行高 24–28px，padding `2px var(--space-2)`；层级缩进 `--space-3`/级。
* **工作区工具条**（height 36px，底部 `1px solid var(--color-border)`）：
  * 工作区下拉：原生 `select`（沿用 app.html 主题适配），列出全部已配置工作区，当前 active 高亮；切换即 `set_active_workspace`。
  * `+` 添加：桌面端调系统目录选择器（`tauri-plugin-dialog`），远程/非 Tauri 环境回退输入路径框；`.btn .btn-sm` 或 icon-btn。
  * `⋯`：工作区操作菜单（删除当前工作区、编辑过滤规则）。
* **树工具条**：`↻ 刷新`、`新建文件`、`新建文件夹` 三个 icon-btn / `.btn .btn-sm`，`--color-text-muted`，hover `--color-text`。
* **目录节点**：折叠箭头（▾/▸ 或 chevron icon）+ 文件夹 icon + 名称（`--fs-sm`，`--color-text`）；展开时按需 `fs_list`（懒加载），加载中显示 spinner 或节点禁用（`opacity: 0.45`）；错误态（无权限/已删除）节点变 `--color-warning` + tooltip 说明。
* **文件节点**：文件 icon（按扩展名可选着色，v1 统一 text-muted）+ 名称；被过滤目录（ignore）不展示。
* **选中态**：`background: color-mix(in oklch, var(--color-primary) 8%, transparent)` + 左侧 2px `--color-primary` 竖条（对齐 Neuron 列表项选中态规范）。
* **条目右侧 ⋮（触屏上下文入口，用户补充确认）**：hover 显示（`@media (hover: hover)`），触屏常显；点击弹出与右键相同的上下文菜单。命中区域 ≥ 32×32px（触屏可点）。
* **inline 新建/重命名**：节点原位变输入框（`.btn` 同族 input 词汇：`--color-surface` 背景、`--radius-sm`、`--fs-sm`、focus-visible `--color-primary` outline）；Enter 确认、Esc 取消；错误（重名/越界）输入框红边 `--color-error` + 下方 `--fs-xs` `--color-error` 提示，回滚。
* **拖拽移动**：拖源节点到目标目录行，目标目录高亮（`outline: 2px dashed var(--color-primary)` 或背景 tint）；悬停 600ms 自动展开目标目录；不支持的目标（非目录/越界/自嵌套）显示 `not-allowed`。
* 空态：无工作区 → 引导文案 + 「添加工作区」主按钮；工作区空目录 → 「此目录为空」。

### 3. ContextMenu.svelte（轻量上下文菜单，新增组件）

* 形态对齐 ViewContainer `⋯ 菜单`：`--color-elevated` 背景、`--radius-md`、`--border-width` `--color-border`、阴影、portal 到 body 防裁切；菜单项 height 28px、`--fs-sm`、hover `--color-hover`；分隔线 `1px solid var(--color-border)`。
* 菜单按目标类型动态组装：
  * 空白/根：新建文件、新建文件夹、刷新、分隔线、添加工作区
  * 目录：新建文件、新建文件夹、刷新、分隔线、重命名、删除、移动
  * 文件：打开、复制路径、分隔线、重命名、删除、移动
* 危险项（删除）：文字 `--color-error`，hover 背景 `color-mix(in oklch, var(--color-error) 10%, transparent)`。
* 关闭：点击菜单外 / Esc；复用 Select 的 backdrop 模式。
* 快捷键：F2 重命名 / Delete 删除（树聚焦时）；快捷键仅桌面键盘可用，触屏走 ⋮。

### 4. FileEditor.svelte（main 区多实例编辑器）

```
┌─────────────────────────────────────────────┐
│ types.ts ●             │  ChatArea (激活态)  │  ← EditorTabs：文件名 + ● 未保存标记
├─────────────────────────────────────────────┤
│ ┌─ CodeMirror 6 ──────────────────────────┐ │
│ │ 语法高亮内容（按扩展名/内容嗅探）          │ │
│ │ …                                        │ │
│ └─────────────────────────────────────────┘ │
│ [保存]（未保存时显示）      ● 未保存提示      │  ← 底部/顶部工具条（复用 .btn）
└─────────────────────────────────────────────┘
```

* 编辑器主体：CodeMirror 6，填充面板区域（`--color-bg` / `--color-surface` 背景随主题）；语言高亮按扩展名 + 内容嗅探（`@codemirror/language-data` 按需加载）；行号、当前行高亮沿用 CM6 默认主题，色板与主题自动适配。
* **tab 标题**：文件名 + 未保存时追加 `●`（`--color-warning` 或 `--color-primary`，实现时定，推荐 warning 区分「有修改」）；tooltip 完整路径（相对工作区根）。
* **保存**：`.btn .btn-sm`（未保存时显示，保存后隐藏）+ Ctrl+S；保存成功 toast/内联提示 `--color-success`；失败内联提示 `--color-error`（含越界/重名/外部修改冲突）。
* **外部修改冲突**（需求已确认）：保存时后端校验 mtime 不一致 → 弹确认（容器同现有确认弹窗：`--color-elevated`、`--radius-lg`、说明文案 `--fs-sm` `--color-text-muted`、按钮 `[取消] .btn` + `[覆盖] .btn-danger`）。
* **关闭未保存 tab**：弹确认（同上容器），文案「文件有未保存修改，确定关闭？」。
* 长文件：超过阈值（256KB）按 offset/limit 分页加载，编辑器内滚动到底部自动续载；顶部可显示「已加载 n/m 行」`--fs-xs` `--color-text-muted`。

### 状态汇总（token 映射）

| 状态 | token |
| ---- | ---- |
| 树节点选中 | 左条 `--color-primary`；背景 `color-mix(in oklch, var(--color-primary) 8%, transparent)` |
| 节点 hover | `--color-hover` |
| 未保存 ● | `--color-warning`（推荐） |
| 危险操作（删除/覆盖） | `.btn-danger` / 文字 `--color-error` |
| 加载/禁用 | `opacity: 0.45` |
| 空态/引导 | `--fs-sm` `--color-text-muted` |
| 菜单/确认容器 | `--color-elevated` / `--radius-md`(菜单) / `--radius-lg`(确认) / `--border-width` `--color-border` |
| 拖拽目标高亮 | `outline: 2px dashed var(--color-primary)`（对齐 ViewContainer 拖拽目标） |
| 动效 | hover/状态切换 `--duration-fast` `--ease-out` |

## Icon / SVG Component Export

* 复用现有：
  * sidebar「文件」tab icon：用现有 sessions 同款列表 icon（语义为「资源/文件列表」）或文件树专用 icon（折叠树形），实现时定；SVG 14px inline stroke currentColor。
  * main 区 `file-editor` 面板 tab icon：文档/代码 icon（对齐现有 16px SVG 风格）。
  * 树节点：文件夹/文件 icon（16px inline SVG，stroke currentColor；文件夹 `--color-text-muted`，展开态可换 open 变体）。
* 新增 inline SVG（组件内嵌，不导出为独立图标库）：
  * 刷新 `↻`、新建文件（＋ 文档角标）、新建文件夹（＋ 文件夹角标）、复制路径、移动、打开。
  * 颜色策略：全部 `stroke="currentColor"`，随上下文色（text-muted / text / primary / error）。
* 尺寸策略：sidebar 工具条 icon 14px；树节点 icon 14–16px；main tab icon 16px。
* 可访问性：icon 按钮均带 `aria-label`（i18n 文案）或 `title`。

## Out of Scope（视觉）

* 不新增 design token、不引入新配色/字体体系。
* 文件内容不做事后主题定制：CodeMirror 6 默认主题随 app 主题，不额外重绘配色体系。
* 拖拽动画、目录自动展开倒计时等 motion 细节按现有 `--duration-fast` / `--ease-out` 实现，不做弹性/回弹。
* pad 端仅保证 ⋮ 可达与触控命中区，不做独立视觉布局。
