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
│ [类型 ▾] [状态 ▾] [搜索…] (12)  │  ← 过滤工具条（行高 36px，底部 border）
├────────────────────────────────┤
│ 10:24:03                       │  ← 时间线条目（--fs-xs 时间戳）
│ [complete_scope] ✓ ok          │  ← hook 徽标 + 三态终态徽标
│  “分析电池健康度”               │  ← 会话标题摘要（truncate）
│   ▸ 展开详情                    │  ← 点击条目展开/折叠
│   [在会话中定位]                │  ← 次要操作
│ ───────────────────────────────│
│ 10:23:58                       │
│ [complete_scope] ⚠ downgraded  │
│  “对比充电”                     │
└────────────────────────────────┘
```

* 容器：`--color-surface` 背景，`overflow-y: auto`；列表条目行高 28px、`--fs-sm`，hover 时 `--color-hover` 背景。
* **过滤工具条**（最上层固定）：三个过滤控件 + 计数。类型下拉（`hook_defs_list` 驱动，4 个 hook：complete_scope / match_topic / revise_topic / score_feedback + 「全部」）；状态下拉（三态终态 + 「全部」）；会话搜索输入框（按会话标题模糊匹配）。结果计数显示在条末（`--fs-xs` `--color-text-muted`）；`↻` 刷新按钮重拉列表。过滤条件变化即时重渲染，不改变时间倒序主序。
* **时间线条目**：`created_at` 倒序（新在上）。行内容 = 时间戳（`--fs-xs` `--color-text-muted`）+ hook 徽标（类型短名，等宽 `--font-mono`，底色 `--color-elevated`）+ 终态徽标（见 §3 映射）+ 会话标题摘要（单行 truncate）。终态徽标是最强视觉锚点，颜色即语义。
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
│ ⟳ 裁决中 · complete_scope          [⏱ 1.2s] │  ← pending：spinner + 动态耗时
└─────────────────────────────────────────────┘
┌─────────────────────────────────────────────┐
│ ✓ ok · complete_scope                ▸      │  ← 终态：徽标 + 摘要 + 展开箭头
│   ┌─ 展开详情（--elevated 背景）──────────┐  │
│   │ 模型 gpt-4o-flash · 1 次 · 486ms      │  │
│   │ payload: { … }  decision: { … }       │  │
│   └─────────────────────────────────────┘  │
└─────────────────────────────────────────────┘
┌─────────────────────────────────────────────┐
│ ⚠ downgraded · complete_scope       ▸      │  ← 降级态：黄色徽标 + 悬停原因
│  降级原因：JSON 解析失败，已用默认值兜底     │
└─────────────────────────────────────────────┘
```

* **锚点语义**：卡片渲染在触发裁决的锚点消息（`anchor_message_index = round.startIndex + mi`）块下方，与消息共享左右内边距；**不进入 `rounds` 消息数组**，作为该消息附属渲染块（`{@render}` / 独立组件插槽）挂载，因此不改变消息序号、不影响虚拟滚动。
* **两阶段实时进度**（由 `StateChange::HookJudgements` 事件驱动，`pending` → 终态二选一推送）：
  * `pending`：行内 spinner（CSS 旋转圆环，`--color-text-muted`）+ 「裁决中 · <hook 名>」+ 动态耗时（`--fs-xs`）；终态事件到达后原子替换为终态卡，不做动画过渡。
  * 终态三选一：`ok` / `retried_ok` / `downgraded`（徽标映射见 §3）。
* **终态卡结构**：单行 = 状态徽标（字符 + 语义色）+ hook 名 + 右侧 `▸` 展开箭头（点击整卡切换）。悬停（桌面）显示 tooltip：`ok` = 「一次成功」；`retried_ok` = 「首次失败，重试后成功」；`downgraded` = 「重试后仍失败，使用降级值兜底」。
* **展开详情**：点击卡片展开（`--color-elevated` 背景、`--radius-sm`），内容与面板详情同源同格式：元信息行（模型 / 尝试次数 / 耗时）+ `payload` / `decision` 等宽 JSON 折叠块 + `attempts_detail`（重试明细）+ `error`（降级原因，`--color-warning`）。卡片宽度随消息区，内容横向滚动。
* **降级态附加提示**：`downgraded` 卡默认在徽标行下方展示一行降级原因摘要（`--fs-xs` `--color-warning`，截断 + 展开查看全文），让用户无需展开即可感知「裁决未按预期执行」。
* 视觉基线：卡 `--fs-sm`、行高 28px、内边距 `--space-2`、边框 `--color-border` 淡描边、圆角 `--radius-sm`；状态色仅作用于徽标字符与降级提示行，卡片背景保持 surface，不整卡染色（避免喧宾夺主）。

### 3. 状态汇总 token 映射表

| 状态 | 语义 | 色 token | 徽标字符 | 面板条目 | 内联卡 | 典型场景 |
| ---- | ---- | -------- | -------- | -------- | ------ | -------- |
| pending | 执行中 | `--color-text-muted` | spinner（⟳） | —（不落终态列表主色，同列表展示为进行中） | 旋转圆环 + 动态耗时 | 首条事件已发、裁决未返回 |
| ok | 一次成功 | `--color-success` | ✓ | `✓ ok` | `✓ ok · <hook>` | 首次调用即结构化解析成功 |
| retried_ok | 重试后成功 | `--color-primary` | ↻ | `↻ retried_ok` | `↻ retried_ok · <hook>` | 首次失败、带反馈重试 1 次后成功 |
| downgraded | 降级兜底 | `--color-warning` | ⚠ | `⚠ downgraded` | `⚠ downgraded · <hook>` + 原因行 | 重试后仍失败，使用 neutral_fallback |

* 次要信息（时间戳 / 元信息 / hook 名）：`--color-text-muted`；hook 徽标底色：`--color-elevated`。
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
