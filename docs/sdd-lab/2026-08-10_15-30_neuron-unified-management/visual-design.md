# Visual Design / 设计文档: Neuron Unified Management

## Source / 来源

* 交互形态：来自迭代内与用户确认的 ASCII 设计模拟（2026-08-10）。

* 设计规范来源（本项目现有事实，不新增体系）：

  * 全局 design tokens：[app.html](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/app.html) `:root` 与 `[data-theme="dark"]`（cool palette，hue 265；primary hue 260；字体 `--fs-xs/sm/base/lg/xl`；间距 `--space-1..16`；圆角 `--radius-sm/md/lg/full`；动效 `--ease-out` + `--duration-fast/normal`）。

  * 系统神经元色板：`--color-system-*`（default / core / user / environment / assistant / tool / knowledge / topic / memory / note）。

  * info 容器 tab 形态：[ViewContainer.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/layout/ViewContainer.svelte)（header 32px、tab = icon + label、激活态 primary 下边框 2px、⋯ 菜单、拖拽换容器）。

  * 现有 system\_type 颜色映射机制：[NeuronIndex.svelte#L63](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/components/NeuronIndex.svelte#L63) 与 [NeuronDetailDrawer.svelte#L193](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/components/NeuronDetailDrawer.svelte#L193) `var(--color-system-${system_type}, var(--color-system-default))`。

* 按钮词汇：`.btn` / `.btn-primary` / `.btn-danger` / `.btn-sm`（app.html 全局）。

## Design Principles / 设计原则

* 沿用现有 cool palette 与按钮/输入词汇，不引入新视觉体系。

* 列表与画布同源（同一神经元数据），仅展示密度不同：列表负责「管理数据」，画布负责「关系可视化」。

* 系统神经元与普通神经元同列表，靠「类型徽标」区分；系统类型配色沿用 `--color-system-*` 机制，无匹配回落 `--color-system-default`。

* 徽标圆点（行为标识）：系统神经元列表项与画布节点均显示，便于一眼区分系统/普通。

## Page Design / 页面设计

### 1. 全局布局（info 容器新增 tab）

* info 容器（右侧栏）tabs 由 `providers / models` 扩展为 `providers / models / ★ 神经元`。

* 新 tab 形态沿用 ViewContainer tab 规则：height 32px、icon 12px + label `--fs-xs`、激活态 `--color-primary` + 底部 2px 下边框、悬停 `--color-text`。

* 移动端：info 容器以 drawer 呈现（沿用现有抽屉行为），列表 panel 随容器抽屉化，不单独做移动布局。

### 2. 《神经元》列表 panel（info 容器，管理入口）

```
┌─────────────────────────────┐
│ [🔍 搜索…]      [类型 ▾]     │  ← 工具栏：搜索框 + 类型筛选(全部/系统/普通)
│ ☑ 多选开关   [＋ 创建]       │  ← 多选开关 + 创建(孤立)入口(自画布迁移)
├─────────────────────────────┤
│ ◉ 核心提示词    session.x   │  ← 列表项：选中态 primary 左边条 + 高亮
│    desc 摘要          [编辑] │
│ ○ 打分助手      assistant_* │  ← 普通项：灰点徽标或按类型色
│    desc 摘要          [编辑] │
│ …                           │
│ [加载更多 ↓]                 │  ← 滚动到底部出现，点击加载下一页
└─────────────────────────────┘
```

* 容器背景 `--color-surface`；列表区域 `overflow-y: auto`。

* **工具栏行**（高 36px，底部 `1px solid var(--color-border)`）：

  * 搜索框：复用现有 input 词汇（surface 背景、`--radius-sm`、`--fs-sm`、placeholder `--color-text-muted`）。

  * 类型筛选：`select`（app.html 已做原生主题适配），三档「全部 / 系统 / 普通」。

  * 多选开关：label + checkbox（`accent-color: var(--color-primary)`），label 文案「多选」；关闭时列表项点击 = 单选设核心，开启时列表项点击 = 勾选。

  * 「＋ 创建」：`.btn .btn-sm`，点击打开创建弹窗（孤立节点，原画布创建能力迁移）。

* **列表项**（行高自适应，min-height 40px，padding `6px var(--space-2)`，底部分隔 `1px solid var(--color-border)` 或间距分割）：

  * 选择指示：单选态 = 左侧 `3px` 主色条 + 背景 `color-mix(in oklch, var(--color-primary) 8%, transparent)`；多选态 = checkbox 勾选。

  * 标题行：`desc`（`--fs-base`，`--color-text`，单行省略）。

  * 副行：`system_type` 徽标（`--fs-xs`，`--color-text-muted`，`font-family: monospace` 风格）+ desc 摘要省略。

  * 系统类型徽标配色：沿用 `var(--color-system-${system_type}, var(--color-system-default))`；**新增视觉决策**：按前缀映射 `session.*` → `--color-system-core`、`assistant_*` → `--color-system-assistant`、其余 → 现有匹配（保持现有 API，前端做前缀映射，失败回落 default）。

  * 「编辑」按钮：`.btn .btn-sm`，`opacity: 0` 悬停显示（或常显，实现时定，设计推荐悬停显示减少视觉噪音），点击打开编辑弹窗。

* **加载更多**：列表底部通栏按钮「加载更多 ↓」，`--fs-sm` `--color-text-muted`，加载中显示 spinner 或禁用态（`opacity: 0.45`）。

* 空态：无匹配项时居中显示「无匹配神经元」`--fs-sm` `--color-text-muted`。

### 3. 主区《神经元》画布子页面

* 结构沿用现有 `NeuronManager` 画布（graph canvas + toolbar + 详情抽屉/弹窗）。

* **toolbar 收缩**：移除搜索框与核心筛选（MultiSelect）框；仅保留 深度 / 布局 / 连线方式 控件，以及（若有）画布缩放等现有控件；布局与现有 toolbar 一致。

* **数据源变化**：画布核心来自右侧列表选中项（单选=单核心；多选=多项，沿用现有 `coreSelection` 机制与画布展开规则——以首个选中为 seed 展开）。

* **节点视觉**：系统节点加类型徽标圆点（`--color-system-*` 机制，与列表一致）；「设为画布核心」节点工具栏操作保留（同步回列表选中态）。

* 空态：列表无选中时画布保持现有 empty 提示。

### 4. 节点编辑弹窗

* 弹窗容器沿用现有编辑弹窗/抽屉规范（`--color-elevated` 背景、`--radius-lg`、`--border-width solid var(--color-border)`、阴影）。

* **字段分组**（纵向，`gap: var(--space-4)`）：

  * 基本信息：desc（单行输入）、content（多行文本域）、tool\_ids（工具多选 chips）、权重（数字输入）——保留原始功能与表单控件样式。

  * 系统类型区（分隔线 `1px solid var(--color-border)`）：

    * 当前值展示：无系统类型时显示「未绑定」`--color-text-muted`；已绑定显示 system\_type（`--fs-sm`，monospace）。

    * 操作：非系统神经元显示「绑定」按钮；已绑定显示「换绑」「取消绑定」按钮（`.btn .btn-sm`；取消绑定用 `.btn-danger` 风格）。

  * 行为管理区（仅绑定系统类型后出现，分隔线）：selection（下拉，含全局上限条件字段）、tools（下拉，含 allowlist 条件字段）、insert\_id（输入）——沿用现有 BehaviorFields 表单控件样式（`--fs-sm`、label `--color-text-muted`）。

* 底部操作：`[取消]`（`.btn`）`[保存]`（`.btn-primary`），右对齐，`padding: var(--space-3) var(--space-4)`，顶部 `1px solid var(--color-border)`。

### 5. 二次确认弹窗（换绑 / 取消绑定）

* 容器同编辑弹窗，宽度较窄（内容自适应，min-width 320px）。

* 内容：标题「确认操作」（`--fs-base` 600）；说明文案（`--fs-sm`，`--color-text-muted`）：明确描述动作与后果，例如「将取消绑定系统类型 `session.assistant_dialogue`，该神经元将变为普通神经元（行为管理控件随之隐藏）」。

* 按钮：`[取消]`（`.btn`）+ `[确认]`（`.btn-danger`，危险语义动作）或 `[确认换绑]`（`.btn-primary`，换绑非破坏）；右对齐。

* 视觉警示：左侧可加 `--color-warning` 竖条或 `⚠` 图标（沿用现有确认弹窗风格，若有）。

### 状态汇总（token 映射）

| 状态     | token                                                                                                          |
| ------ | -------------------------------------------------------------------------------------------------------------- |
| 列表项选中  | 左条 `--color-primary`；背景 `color-mix(in oklch, var(--color-primary) 8%, transparent)`                            |
| 系统类型徽标 | `var(--color-system-${type}, var(--color-system-default))`（前缀映射 session.\* → core / assistant\_\* → assistant） |
| 危险操作   | `.btn-danger` + `--color-error`                                                                                |
| 加载更多禁用 | `opacity: 0.45`                                                                                                |
| 空态     | `--fs-sm` `--color-text-muted`                                                                                 |
| 弹窗容器   | `--color-elevated` / `--radius-lg` / `--border-width` `--color-border`                                         |
| 动效     | 状态切换 `--duration-fast` `--ease-out`                                                                            |

## Icon / SVG Component Export

* 新 tab 图标：复用现有 `views.neurons` 视图的 SVG icon（同源，语义一致）；如觉区分度不足，可用「神经元 + 列表线」变体，实现阶段确认。

* 系统类型徽标：不新增 SVG，使用 8px 圆点 + 色板变量（列表项与画布节点通用）。

* 二次确认弹窗警示：沿用项目现有确认弹窗的图标（若有），否则用 text 强调。

## Out of Scope（视觉）

* 不新增设计 token、不引入新配色体系。

* 画布布局算法、节点形状/力导向渲染不做视觉改造（仅新增类型徽标圆点）。

* 移动端除抽屉化外不做独立视觉设计。

