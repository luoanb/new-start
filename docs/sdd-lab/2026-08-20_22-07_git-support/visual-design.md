# Visual Design / 设计文档: Git Support

## Source / 来源

* 交互形态：来自迭代内与用户确认的需求（2026-08-20）+ **社区共识**（业界主流 Git 工具 UI 的通行交互模式）：
  * VS Code 源代码管理（SCM）视图：变更文件分组（更改/暂存区）、文件级 staging 勾选、分支切换、变更徽标。
  * VS Code / GitLens 的 diff 视图：unified diff 行内渲染、hunk 头（`@@ -a,b +c,d @@`）、增删行色、行号、前后 hunk 导航。
  * GitHub Desktop / lazygit：staging 面板（Changes → Staged）、commit message 输入 + 提交按钮、commit 前预览 diff。
  * VS Code 冲突编辑器：冲突 hunk 的「接受当前/接受传入/接受两者」操作按钮。
  * GitLens 行 blame：行首显示 `commit 短哈希 作者 日期`，hover 展示完整信息。
* 设计规范来源（本项目现有事实，不新增体系）：
  * 全局 design tokens：[app.html](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src/app.html) `:root` 与 `[data-theme="dark"]`（cool palette hue 265 / primary hue 260；字体 `--fs-xs/sm/base`；间距 `--space-1..16`；圆角 `--radius-sm/md/lg`；动效 `--ease-out` + `--duration-fast/normal`；`--color-hover` / `--color-text-muted` / `--color-border` / `--color-success` / `--color-warning` / `--color-error` / `--color-error-bg`）。
  * 侧栏容器 tab 形态：[ViewContainer.svelte](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src/lib/layout/ViewContainer.svelte)（header 32px、tab = icon + label、激活态 primary 底部 2px 下边框、⋯ 视图菜单）。
  * main 区 tab 形态：[EditorTabs.svelte](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src/lib/layout/EditorTabs.svelte)（height 32px、激活态 surface 背景 + primary 顶部 2px 上边框、多实例 tab、HTML5 DnD）。
  * 既有可复用组件：[ConfirmDialog.svelte](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src/lib/components/ConfirmDialog.svelte)（确认弹窗）、[ContextMenu.svelte](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src/lib/components/ContextMenu.svelte)（右键菜单）、[Select.svelte](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src/lib/components/Select.svelte)（下拉）、[FileExplorer.svelte](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src/lib/components/FileExplorer.svelte)（文件树 + 状态徽标挂靠点）。
  * 视图注册机制：[views.ts](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src/lib/layout/views.ts)（`viewRegistry` / `mainViews` / `mainPanelMeta`；icon 统一 14–16px inline SVG stroke currentColor）。
  * 文件编辑器多实例语义：[layoutTypes.ts](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src/lib/layout/layoutTypes.ts) `MainPanelType` 已含 `file-editor` 实例 key 机制，`git-diff` 面板复用同一实例语义。

## Design Principles / 设计原则

* 沿用现有 cool palette 与按钮/输入/浮层词汇，不引入新视觉体系、不新增 design token。
* **Git 状态色即语义色**：变更/未暂存 = 现有 `--color-warning` 系或文本强调；新增 = `--color-success` 系；冲突/危险 = `--color-error` 系；已暂存 = `--color-primary` 系。不引入 VS Code 的绿色/红色等新色，映射到现有 token。
* git 面板与文件树共享同一「active workspace + 当前仓库」数据源；仓库切换器是面板的全局作用域控制。
* diff 视图采用 **unified（行内）渲染**而非双栏并排：输出即 `git diff` 文本、实现轻、信息密度适合本项目窄面板；对比等双栏形态不在本期（Out of Scope）。
* 多实例语义对齐 VS Code：`git-diff` 面板按文件路径多开（复用 file-editor 实例机制）；git 面板单实例。
* 触屏与桌面并存：git 面板操作全部走按钮/下拉（不依赖右键/hover）；文件树状态徽标 hover 展示完整提示，触屏点击徽标打开 diff。

## Page Design / 页面设计

### 0. 布局总览

```
┌────────────────────────────────────────────────────────────┐
│ sidebar: sessions │ files │ git │ topics │ tools  …         │ ← 新增「git」视图（单实例）
│                                                            │
│  ┌─ git 面板 ───────────────────────────────────────┐      │
│  │ 仓库 [repo-a ▾]  分支 [main ▾]             [⋯]   │      │ ← 作用域工具条
│  │ 变更汇总：0 暂存 / 3 更改   [↻]              │      │
│  │ ── 暂存区 (1) ──                                  │      │
│  │   ☑ src/a.ts          [−]                        │      │ ← 分组列表
│  │ ── 更改 (2) ──                                   │      │
│  │   ☐ src/b.ts (M)  [＋]                           │      │
│  │   ☐ docs/x.md (??)  [＋]                          │      │
│  │ [提交信息输入框 …]                                │      │
│  │ [✓ 提交]（按钮）                                  │      │
│  │ ── 分支 ── / ── Stash ──（可折叠区段）            │      │
│  └──────────────────────────────────────────────┘      │
│                                                            │
│  ┌─ main:  [git-diff: src/a.ts ●?] [file-editor: b.ts] ┐  │
│  │  Diff 视图（unified）：                               │  │
│  │  @@ -12,5 +12,6 @@ func foo()                          │  │
│  │    context line…                                       │  │
│  │  -deleted line…                                        │  │
│  │  +added line…                                          │  │
│  └────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────┘
```

### 1. sidebar「git」视图（GitPanel.svelte，单实例）

```
┌──────────────────────────────┐
│ [仓库 ▾]  [分支 ▾]      [⋯]   │  ← 作用域工具条（36px，底部 border）
│ 0 暂存 · 3 更改   [↻]        │  ← 变更汇总 + 刷新
├──────────────────────────────┤
│ ▸ 暂存区 (1)                 │  ← 分组头（可折叠）
│   ☑ src/a.ts  (M)  [−]       │
│ ▸ 更改 (2)                   │
│   ☐ src/b.ts  (M)  [＋]      │  ← 条目：勾选 + 状态徽标 + 暂存/取消
│   ☐ docs/x.md (??) [＋]      │
│   ☐ dir/ (2 files)  [＋]     │
├──────────────────────────────┤
│ [Commit message 输入框]       │  ← 输入区（有 staging 时可用）
│ [✓ 提交]                     │  ← .btn-primary，提交前弹确认（含 diff 摘要）
├──────────────────────────────┤
│ ▸ 分支                        │  ← 折叠区段：当前分支 + 分支列表/切换
│ ▸ Stash                       │  ← 折叠区段：stash 列表（创建/应用/删除）
└──────────────────────────────┘
```

* 容器：`--color-surface` 背景，`overflow-y: auto`；区段/分组头 height 28px、`--fs-xs`、`--color-text-muted`；条目行高 28px、`--fs-sm`。
* **作用域工具条**：仓库下拉（列出 workspace 内全部发现的 repo，当前高亮，切换即切当前操作仓库）；分支下拉（当前分支 + 全部分支，切换 = `git checkout`）；`⋯` 菜单（拉到最新、推送到远端、丢弃更改、编辑仓库忽略）。
* **变更汇总行**：`N 暂存 · M 更改`（`--fs-xs` `--color-text-muted`），变更数 >0 时数字以 `--color-primary` 强调；`↻` 刷新按钮（触发 git_status 重拉）。
* **暂存区/更改分组**：grouped 列表（对齐 VS Code SCM 分组头语义）。条目 = 勾选框 + 文件名 + 状态徽标（M/A/D/R/??/U）+ 右侧操作按钮（未暂存 `＋` 暂存；已暂存 `−` 取消暂存）。目录可折叠聚合显示「dir/ (n files)」。
* **勾选框**：原生 checkbox（`accent-color: var(--color-primary)`）或自定义 16px 勾选框，对齐文件树选中态。
* **提交区**：单行 message 输入（多行 → 按 Shift+Enter 展开为 textarea，对齐 GitKraken）；`[✓ 提交]` `.btn-primary`，未勾选任何暂存文件时 disabled（`opacity 0.45`）；点击后弹确认（见 §5），确认内嵌展示 staged diff 摘要（最近变更的 hunk 列表）。
* **折叠区段（分支 / Stash）**：header `▸` + 标题 + 计数；展开后分支列表（当前分支加 ✓ 前缀 + `--color-primary`）、stash 列表（`stash@{n}: message`）。空态：非 git 仓库 → 引导文案「当前工作区不是 Git 仓库」；无变更 → 「工作区干净」。

### 2. 文件树 git 状态徽标（FileExplorer 集成）

* 文件树条目（`FileExplorer.svelte`）行右侧追加状态徽标区（原 ⋮ 按钮左侧）。
* **徽标 = git 状态字母**：`M`（已修改）/ `A`（已暂存新增）/ `D`（已删除）/ `R`（已重命名）/ `??`（未跟踪）/ `U`（冲突）；目录聚合为最显著子项状态或计数（`n`）。
* 视觉：`--fs-xs` 等宽（`--font-mono`）字符徽标，位于条目文字与 ⋮ 之间；状态色映射：
  * 冲突 `U`：`--color-error`（最高优先级）
  * 已暂存（A/M 大写字母）：`--color-primary`
  * 未暂存修改 `M` / 未跟踪 `??`：`--color-warning`
  * 已删除 `D`：`--color-text-muted`
* hover/触屏点击徽标 → 打开该文件的 `git-diff` 面板（未跟踪/二进制文件除外，未跟踪文件直接打开 file-editor）。
* 数据刷新：git 面板任何写操作 / 文件树写操作成功后，经 `StateChange::Git` 事件刷新徽标（不做轮询）。

### 3. GitDiff.svelte（main 区「git-diff」面板，按文件路径多实例）

```
┌────────────────────────────────────────────────────────┐
│ git-diff: src/a.ts  ·  vs HEAD (staged)     [选择范围 ▾]│  ← 面板头部：范围切换
├────────────────────────────────────────────────────────┤
│ [◀ 上一处]  [下一处 ▶]   @@ -12,5 +12,6 @@  (1/3)      │  ← hunk 导航条
│ ─────────────────────────────────────────────────────  │
│ 12  │ import { foo } from "./foo";                      │
│ 13  │                                                   │
│ 14  │-export function oldName() {                       │  ← 删除行（error-bg 底）
│ 15  │+export function newName() {                       │  ← 新增行（success 底）
│ 16  │   const x = 1;                                    │
│ ... │                                                   │
└────────────────────────────────────────────────────────┘
```

* **unified 行内 diff**：`--font-mono`，行号列（右对齐，`--fs-xs` `--color-text-muted`）+ 内容列。行高 20px。
* **hunk 头**（`@@ -a,b +c,d @@` 与上下文首行）：`--color-text-muted` 背景 `--color-elevated`，`--fs-xs`。
* **增删行色**（token 映射，替代 VS Code 绿/红）：删除行背景 `color-mix(in oklch, var(--color-error) 12%, transparent)` + 前导 `-`（`--color-error`）；新增行背景 `color-mix(in oklch, var(--color-success) 12%, transparent)` + 前导 `+`（`--color-success`）；上下文行默认色无背景。
* **hunk 导航条**：`[◀ 上一处] [下一处 ▶]`（`.btn .btn-sm`）+ `n/total` 计数 + 当前 hunk 摘要；点击在 hunk 间滚动。
* **范围切换**：面板头部下拉/分段控件 `staged（vs HEAD）` / `unstaged（vs index）` / `both`；映射 `git diff --cached` / `git diff` / 合并展示。
* **冲突文件 diff**（merge conflict 场景）：冲突 hunk 以三块渲染——`ours` 段 / 分隔线 / `theirs` 段，各带操作按钮 `[接受当前] [接受传入] [接受两者]`（对齐 VS Code 冲突编辑器）；点击后调 `git_checkout --ours/--theirs` 或写入合并结果 + `git_add`，随后刷新状态。
* **blame 视图**（同一面板内切换 tab/开关）：行首 blame 栏（`hash7 作者 日期`，`--fs-xs` `--color-text-muted`，hover 展开完整信息 tooltip），列宽固定可横向滚动。

### 4. 分支 / Stash / 多仓库交互

* **分支切换**：作用域工具条分支下拉 → `git_checkout <branch>`；切换成功后面板与文件树状态刷新；本地未提交改动导致 checkout 失败 → 错误提示（`--color-error` 内联），可选转 stash 提示（弹确认）。
* **Stash 区段**：`[Stash…]` 按钮（输入 message，弹确认）→ 创建；列表项 = `stash@{n}: message` + hover 按钮 `[应用]` `[丢弃]`；应用/丢弃前弹确认。
* **多仓库**：仓库下拉列出 workspace 内发现的所有 repo（含嵌套），`当前操作仓库` 高亮；切换仅改变面板作用域，不改变文件树；文件树徽标按文件归属 repo 渲染（同一 workspace 不同 repo 各自的 status）。

### 5. 危险操作确认弹窗（复用 ConfirmDialog）

* 触发：提交、reset（含 reset --hard）、clean、checkout 丢弃改动、push、pull、stash 应用/丢弃。
* 形态：复用 [ConfirmDialog.svelte](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src/lib/components/ConfirmDialog.svelte) 容器（`--color-elevated`、`--radius-lg`、`--border-width` `--color-border`、阴影、backdrop）。
* 内容分级：
  * 常规写（commit / push / pull / stash 应用）：标题 + 摘要描述（commit 附 staged diff 摘要 hunk 列表；push 附 `origin/main -> main` 方向描述）+ `[取消] .btn` + `[确认] .btn-primary`。
  * 高危写（reset / clean / checkout 丢弃改动）：标题 + 影响描述 + **将丢失改动清单**（reset 前 dry-run 列出的文件/hunk）+ 操作按钮改用 `[取消] .btn` + `[确认丢弃] .btn-danger`；danger 区说明文案 `--fs-sm` `--color-text-muted`。
* 注：确认交互仅 GUI（需求 Q4）；Rust 侧确认接口为稳定接口设计，TUI 未来接入不改变本视觉形态。

### 状态汇总（token 映射）

| 状态 | token |
| ---- | ---- |
| 冲突徽标 `U` / 危险操作 | `--color-error` / `.btn-danger` |
| 新增/成功/提交 | `--color-success` / `.btn-primary`（提交） |
| 暂存状态（A/M 暂存） | `--color-primary` |
| 未暂存修改/未跟踪 | `--color-warning` |
| 已删除 `D` / 次要信息 | `--color-text-muted` |
| diff 删除行背景 | `color-mix(in oklch, var(--color-error) 12%, transparent)` |
| diff 新增行背景 | `color-mix(in oklch, var(--color-success) 12%, transparent)` |
| hunk 头 / 分组头 | `--color-elevated` 背景 / `--color-text-muted` 文字 |
| 面板/条目 | `--color-surface` / 行高 28px / `--fs-sm` |
| 动效 | hover/状态切换 `--duration-fast` `--ease-out` |

## Icon / SVG Component Export

* 复用现有：
  * sidebar「git」tab icon：分支图标（git branch SVG，对齐现有 14px inline stroke currentColor 风格）。
  * main「git-diff」面板 tab icon：diff/对比图标（对齐 file-editor 16px SVG 风格）。
  * 确认/菜单/下拉：复用 `ConfirmDialog` / `ContextMenu` / `Select` 组件，不新增图标。
* 新增 inline SVG（组件内嵌，不导出为独立图标库）：
  * `＋` 暂存、`−` 取消暂存、`↻` 刷新、`◀/▶` hunk 上一处/下一处、`[Stash…]` 存档图标、`✓` 提交、分支切换（git-branch）、`⋯` 面板菜单。
  * 颜色策略：全部 `stroke="currentColor"`，随上下文色（text-muted / text / primary / error / success）。
* 尺寸策略：sidebar 工具条 icon 14px；面板条目内 icon 12–14px；main tab icon 16px。
* 可访问性：icon 按钮均带 `aria-label`（i18n 文案）或 `title`；状态徽标仅用字母 + 颜色双通道（不单靠颜色区分）。

## Out of Scope（视觉）

* 不新增 design token、不引入新配色/字体体系；Git 状态色全部映射到现有 token。
* 不做双栏（side-by-side）diff 视图；本期统一 unified 行内渲染。
* 不做 diff 行内编辑/手动 hunk staging（`git add -p` 交互式暂存 UI）——本期暂存为文件级勾选。
* 不做事后主题定制（diff 配色随主题 token 自动适配）。
* 不引入 lazygit/GitLens 的复杂图形（分支图、提交图）渲染；log 用列表呈现。
