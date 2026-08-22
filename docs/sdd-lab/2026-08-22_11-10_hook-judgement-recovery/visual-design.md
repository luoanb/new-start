# Visual Design / 视觉设计文档: Hook 判定执行与纠偏

## Source / 来源

* 交互形态：来自迭代内与用户确认的需求（2026-08-22）：
  * 「独立统一查看所有裁决记录的面板（与会话、文件等同级）」：sidebar 新增同级视图。
  * 「不重跑、全量保留」：面板为只读时间线，任何记录不提供重跑入口，历史全量留存。
  * 「用户侧消息列表也要能看到裁决执行进度和过程（AI 侧保持透明）」：消息列表锚点消息下方内联「裁决卡」，两阶段实时进度（pending → 终态）。
  * 「消息列表内联裁决卡」设计思路经用户认可写入 spec。
  * 社区共识（业界主流任务/执行面板的通行交互模式）：
  * GitHub Actions / CI 任务面板：任务列表（时间倒序）+ 展开详情（步骤/日志）+ 状态徽标（success/failure/warning）。
  * 开发者工具 Network / 调试日志时间线：过滤条件 + 时间戳列表 + 点击展开请求/响应全量内容。
  * 即时通讯的「已读/投递状态」附属块：消息下方小字状态行（发送中 → 已送达），不侵入消息气泡本体。
* 设计规范来源（本项目现有事实，不新增体系）：
  * 全局 design tokens：[app.html](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/app.html) `:root` 与 `[data-theme="dark"]`（cool palette hue 265 / primary hue 260；字体 `--fs-xs/sm/base`；间距 `--space-1..16`；圆角 `--radius-sm/md/lg`；动效 `--ease-out` + `--duration-fast/normal`；`--color-hover` / `--color-text-muted` / `--color-border` / `--color-success` / `--color-warning` / `--color-error` / `--color-error-bg`）。
  * 侧栏容器 tab 形态：[ViewContainer.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/layout/ViewContainer.svelte)（header 32px、tab = icon + label、激活态 primary 底部 2px 下边框、⋯ 视图菜单）。
  * 视图注册机制：[views.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/layout/views.ts)（`viewRegistry` / `mainViews` / `activityItems`；icon 统一 14px inline SVG stroke currentColor，`movableTo: "*"`）。
  * 既有可复用组件：[Select.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/components/Select.svelte)（下拉，过滤用）、[ContextMenu.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/components/ContextMenu.svelte)、[ConfirmDialog.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/components/ConfirmDialog.svelte)。
  * 消息列表渲染：[ChatArea.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/components/ChatArea.svelte)（`rounds` 分组渲染，消息全局索引 = `round.startIndex + mi`，作为裁决卡锚点语义）。
  * 状态徽标先例：会话列表 `mode-badge` 色板、文件树 git 状态徽标（`--fs-xs` 字符徽标 + 语义色映射）。

## Design Principles / 设计原则

* 沿用现有 cool palette 与按钮/输入/浮层词汇，不引入新视觉体系、不新增 design token。
* **裁决终态即语义色**（四态映射，替代竞品的绿/红新色）：一次成功 `ok` = `--color-success`；重试后成功 `retried_ok` = `--color-primary`；降级兜底 `downgraded` = `--color-warning`；执行中 `pending` = `--color-text-muted` + 旋转 spinner。
* **只读不重跑**：面板与内联卡均为纯展示，无编辑、重试、删除入口；数据全量保留，只增不删。
* **附属渲染块，不侵入消息流**：内联裁决卡是锚点消息下方的附属块，不插入消息数组、不占消息序号、不影响虚拟滚动与消息分页。
* **单一数据源，双处投影**：sidebar 面板（全量时间线）与消息内联卡（锚点上下文）读同一份裁决记录，两处状态实时一致；AI 侧保持透明。
* 触屏与桌面并存：过滤与定位操作全部走按钮/下拉（不依赖右键/hover）；悬停提示仅作为增强，触屏可点击打开详情。

## Page Design / 页面设计

### 0. 布局总览

```
┌────────────────────────────────────────────────────────────┐
│ sidebar: sessions │ files │ git │ topics │ hook-judgements │  ← 新增「Hook 判定」视图（单实例）
│                                                            │
│  ┌─ hook-judgements ─────────────────────────────────┐     │
│  │ [类型 ▾] [状态 ▾] [会话搜索…]         (12) [↻]     │     │ ← 过滤工具条 + 计数
│  │ ─────────────────────────────────────────────     │     │
│  │ 10:24:03 [complete_scope] ✓ ok  “分析电池健康度”    │     │ ← 时间线条目（created_at 倒序）
│  │ 10:24:01 [match_topic]    ✓ ok  “分析电池健康度”    │     │
│  │ 10:23:58 [complete_scope] ⚠ downgraded “对比充电”   │     │
│  │  ▸ 详情展开（payload/decision/attempts/error）     │     │
│  │  [在会话中定位]                                    │     │
│  │ …                                                  │     │
│  └───────────────────────────────────────────────────┘     │
│                                                            │
│  ┌─ main: [chat: 会话 A] ─────────────────────────────┐    │
│  │  …锚点消息 n（用户/助手内容）…                        │    │
│  │   ┌─ 裁决卡 ────────────────────────────┐           │    │
│  │   │ ⟳ 裁决中 · complete_scope            │           │    │ ← pending
│  │   └────────────────────────────────────┘           │    │
│  │   ┌─ 裁决卡 ────────────────────────────┐           │    │
│  │   │ ✓ ok · complete_scope    ▸          │           │    │ ← 终态（可展开）
│  │   └────────────────────────────────────┘           │    │
│  │  …消息 n+1…                                          │    │
│  └────────────────────────────────────────────────────┘    │
└────────────────────────────────────────────────────────────┘
```

### 1. sidebar「Hook 判定」视图（HookJudgementPanel.svelte，单实例）

```
┌────────────────────────────────┐
│ [类型 ▾] [状态 ▾] (12)         │  ← 过滤工具条 + 计数
│ ─────────────────────────────────────────────     │
│ 10:24:03                       │  ← 时间线条目（--fs-xs 时间戳，短格式 HH:mm:ss）
│ [complete_scope] ✓ ok          │  ← hook 徽标 + 四态终态徽标
│  {decision 摘要}               │  ← 决策/错误摘要（truncate）
│   ▸ 展开详情                    │  ← 点击条目展开/折叠
│   [在会话中定位]                │  ← 次要操作
│ ───────────────────────────────│
│ 10:23:58                       │
│ [complete_scope] ⚠ downgraded  │
│  “对比充电”                     │
└────────────────────────────────┘
```

* 容器：`--color-surface` 背景，`overflow-y: auto`；列表条目行高 28px、`--fs-sm`，hover 时 `--color-hover` 背景。
* **过滤工具条**（面板标题栏下方，对齐课题面板 `filter-bar` 词汇：surface 底 + 圆角容器）：两个过滤控件 + 结果计数。类型下拉（`hook_defs_list` 驱动，4 个 hook：complete_scope / match_topic / revise_topic / score_feedback + 「全部」）；状态下拉（四态 + 「全部」）。结果计数显示在条末（`--fs-xs` `--color-text-muted`）。过滤条件变化即时重渲染，不改变时间倒序主序。
* **面板标题栏**（对齐 ToolPanel / TopicPanel `panel-toolbar` 词汇）：标题 `views.hookJudgements` + 刷新按钮（`icon-btn` 词汇，26px 方形）。注：裁决记录无会话标题字段，不做会话搜索框（需求收敛，见 lifecycle 记录）。
* **时间线条目**：`created_at` 倒序（新在上）。行内容 = 短时间戳（`HH:mm:ss` `--fs-xs` `--color-text-muted`，完整时间 title 悬停）+ hook 徽标（i18n 短名，等宽 `--font-mono`，底色 `--color-elevated`）+ 终态徽标（见 §3 映射，小号文字 + 语义色文字色，无底色/圆点/动画）+ 决策/错误摘要（单行 truncate，`--fs-xs`）。
* **详情展开**（点击条目行任意处切换）：展开区为条目下方缩进块（`--color-elevated` 背景、`--radius-sm`、`--space-2` 内边距），分字段展示：
  * 元信息行：模型名、耗时 `duration_ms`、尝试次数 `attempts`（`--fs-xs` `--color-text-muted`）。
  * `payload`（裁决输入）与 `decision`（裁决输出/降级值）：等宽字体 `--font-mono` 的折叠代码块（JSON 原样，`--fs-xs`），超出面板宽度横向滚动。
  * `attempts_detail`（全量重试明细）：仅 `retried_ok` / `downgraded` 展示，列表式（第 1 次失败原因 / 第 2 次结果）。
  * `error`（降级原因）：仅 `downgraded` 展示，`--color-warning` 色小字。
* **「在会话中定位」**（`.btn .btn-sm`，条尾）：打开该会话的 chat 主面板实例，滚动到锚点消息（`anchor_message_index`）并使裁决卡高亮闪烁一次（`--color-primary` 背景淡出，`--duration-normal`）。仅面板提供，内联卡天然在上下文中不重复提供。
* **空态**：无记录时显示引导文案「暂无裁决记录」（`--color-text-muted`）；有过滤条件时为空则显示「无匹配记录」。面板不显示进行中任务（裁决由会话流程自动触发，无手动入口）。

### 2. 消息列表内联裁决卡（JudgementCard.svelte，ChatArea 集成）

```
…消息 n（锚点）…
┌─────────────────────────────────────────────┐
│ ◌ complete_scope · 1.2s             ▸      │  ← pending 折叠行：◌ + hook 名 + 动态耗时
└─────────────────────────────────────────────┘
┌─────────────────────────────────────────────┐
│ complete_scope                      ▸      │  ← 终态折叠行：类型锚点（正文色）+ chevron
│ ┌─ 展开 ──────────────────────────────────┐ │
│ │ ✓ 成功 · 1.2s          ← 第一眼：结果行   │ │
│ │ 决策依据摘要（decision / error）          │ │
│ │ 模型 gpt-4o-flash · 1 次 · 486ms         │ │
│ │ payload / attempts_detail / raw / …      │ │
│ └─────────────────────────────────────────┘ │
└─────────────────────────────────────────────┘
```

* **锚点语义**：卡片渲染在触发裁决的锚点消息（`anchor_message_index = round.startIndex + mi`）块下方，与消息共享左右内边距；**不进入 `rounds` 消息数组**，作为该消息附属渲染块（`{@render}` / 独立组件插槽）挂载，因此不改变消息序号、不影响虚拟滚动。
* **信息层级（用户确认）**：折叠行锚定「类型 + 是否执行中」——终态只显示 hook 名（正文色 `--color-text`，与其他卡片折叠行主体一致，无徽标底色）；`pending` 时额外显示静态 ◌（muted）+ 动态耗时，保证执行进度在折叠行实时可见。结果留给展开第一眼。
* **展开第一眼 = 结果行**（`.verdict`，`border-bottom` 分隔）：状态字符（✓/↻/⚠/◌，小号非加粗）+ 状态文字（语义色）+ pending 动态耗时；紧随其后为决策依据（decision 摘要正文色 / error 错误色），再往下才是元信息与各详情折叠块（payload / attempts_detail / raw_response / decision / error，`--color-elevated` 底 + `--radius-sm`，与面板详情同源同格式）。
* **两阶段实时进度**（由 `StateChange::HookJudgements` 事件驱动，`pending` → 终态二选一推送）：pending 折叠行 = 「◌ hook 名 · 耗时」（静态字符，无 spinner 动画）；终态事件到达后原子替换为纯类型折叠行，不做动画过渡。
* **悬停语义提示**（桌面）：结果行 title：`ok` = 「一次成功」；`retried_ok` = 「首次失败，重试后成功」；`downgraded` = 「重试后仍失败，使用降级值兜底」。
* 视觉基线：工具类应用克制原则——中性表面 `--color-surface` + 淡边框 + 圆角 `--radius-sm`，**无**左侧彩色 accent、无整卡染色、无动画闪烁；语义色仅用于小号状态字符（✓/↻/⚠，非加粗）；折叠条 = `summary` 按钮 + `toggle-icon` chevron（展开旋转 90°）；左右 `margin` 对齐消息正文（`--space-4`），正文信息（状态文字 / 决策摘要）用 `--color-text` 与消息正文一致，`--color-text-muted` 仅用于时间戳等次要信息。
* 消息区卡片统一规范（适用于 JudgementCard / NudgeBlock / ThinkingBlock / ToolCallBlock / ToolResultBlock 全部内联块）：`--color-surface` 底 + `--color-border` 边框 + `--radius-sm`，无 accent 竖条、无混合底色、无装饰动画；折叠条为整行可点 `summary`（div，`role="button"`，padding `space-1 space-2`、fs-xs、hover 背景），行内唯一独立按钮是 CopyButton（`stopPropagation`），行尾 chevron 为**纯装饰 span**（非按钮，旋转指示展开态）；正文信息 `--color-text`，类型标签/元信息 `--color-text-muted`；展开详情 `border-top` 分隔 + `space-2` padding。代码输出块（JSON/stdout）保留深色 oklch 底（功能性）。交互模型与课题面板一致：整行可点 + 行内独立操作按钮。
* **折叠行统一布局**（5 卡一致）：`[图标?] 主体文字(flex:1 + truncate) → CopyButton → chevron`。CopyButton 为 22px 定高，即折叠行高度基准（上下 padding `space-1` → 总高 30px，所有卡片一致）；JudgementCard 同样提供 CopyButton（复制完整裁决记录 JSON），`.summary` 均含 `width:100%` + `border-radius`。工具卡图标去 emoji：工具执行（ToolCallBlock）= lucide `terminal` 14px stroke SVG、工具返回（ToolResultBlock）= lucide `monitor` 14px stroke SVG；图标 muted（装饰性），主体工具名正文色。
* **详情折叠块（`details.field`）统一**：隐藏原生 marker（`list-style:none` + `::-webkit-details-marker{display:none}`），summary 为 flex 行 + 前置 `.field-chevron`（与折叠行 `toggle-icon` 同一 chevron 词汇：14px 容器 / 12px svg / muted，`[open]` 旋转 90°），杜绝浏览器默认 🔻 三角。JudgementCard 与 HookJudgementPanel 两处 `.field` 全部对齐。

### 3. 状态汇总 token 映射表

| 状态 | 语义 | 色 token | 徽标字符 | 面板条目 | 内联卡 | 典型场景 |
| ---- | ---- | -------- | -------- | -------- | ------ | -------- |
| pending | 执行中 | `--color-text-muted` | ◌（静态） | —（列表展示为进行中） | 折叠行 `◌ hook 名 · 耗时`；展开结果行「裁决中 + 耗时」 | 首条事件已发、裁决未返回 |
| ok | 一次成功 | `--color-success` | ✓ | `✓ 成功` | 展开结果行 `✓ 成功` | 首次调用即结构化解析成功 |
| retried_ok | 重试后成功 | `--color-primary` | ↻ | `↻ 重试成功` | 展开结果行 `↻ 重试成功` | 首次失败、带反馈重试 1 次后成功 |
| downgraded | 降级兜底 | `--color-warning` | ⚠ | `⚠ 已降级` | 展开结果行 `⚠ 已降级` + 原因 | 重试后仍失败，使用 neutral_fallback |

* 次要信息（时间戳 / 元信息 / 面板 hook 类型标签）：`--color-text-muted`；消息卡折叠行主体（hook 名）为正文色 `--color-text`，无徽标底色。
* 降级原因 / 错误字段：`--color-warning`（原因）与 `--color-error`（仅渲染于展开详情错误块，条目行不使用）。
* 字体：列表与卡片正文 `--fs-sm`；时间戳/元信息/展开 JSON `--fs-xs`；JSON 与 hook 名 `--font-mono`。

## Icon / SVG Component Export / Icon 与 SVG 组件导出

- 导出目标路径：不新增独立 SVG 文件；视图 icon 内联注册于 [views.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/layout/views.ts) `viewRegistry`（对齐 sessions/files/git 等既有条目）。
- 命名规则：视图 id `hook-judgements`，i18n key `views.hookJudgements`（沿用 `views.*` 前缀约定）。
- 颜色策略：`currentColor`（跟随容器文字色，不写死色值）。
- 尺寸策略：14×14，`stroke-width="1.8"`，`fill="none"`，`stroke-linecap/linejoin="round"`（对齐既有 14px 视图 icon 规范）。
- 可访问性属性：`aria-hidden="true"`（纯装饰图标），语义信息由相邻文字/徽标承担。
- 状态徽标字符（✓ ↻ ⚠ ⟳）用文本字符渲染，不导出为 SVG，避免图标文件膨胀。

| Icon | 用途 | SVG 文件名 | 组件名 | 尺寸 | 颜色策略 | 状态 |
| ---- | ---- | ---------- | ------ | ---- | -------- | ---- |
| 判决天平 | sidebar「Hook 判定」视图 tab | 内联（views.ts） | viewRegistry["hook-judgements"] | 14×14 | currentColor | 待导出（实现时内联） |
