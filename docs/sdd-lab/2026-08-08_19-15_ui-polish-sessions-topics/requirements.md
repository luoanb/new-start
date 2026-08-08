# requirements.md — UI 打磨：SessionList 会话列表 + TopicPanel 课题面板

> 迭代：ui-polish-sessions-topics
> 创建：2026-08-08
> 状态：done（已执行并验证）

## Background / 背景

用户反馈：会话列表与课题面板是应用的「门面」，但当前样式存在硬伤：

- SessionList 列表项只显示缩写 ID（`a1b2c3d4..x9y8`），用户无法辨认会话内容；active 态与 hover 视觉相同；icon-btn 带边框，与其他面板按钮词汇不一致；时间无日期分级；空状态单薄。
- TopicPanel 状态 badge 为大面积彩色背景块 + 硬编码色值（`#f59e0b` 等），违背 DESIGN.md「克制即优雅」与 Restrained 色彩策略；`--color-danger` token 不存在（实际为 `--color-error`）；状态标签/过滤器/错误消息未 i18n；删除入口按钮误用「Confirm/确认」文案；`.btn` 组件内重复定义、hover/focus/active/disabled 状态不全。

设计依据：[PRODUCT.md](/home/lab/Documents/trae_projects/new-start-wt/PRODUCT.md)（简洁/流畅/可靠、模式可见即所得、克制即优雅、反馈先于形式）、[DESIGN.md](/home/lab/Documents/trae_projects/new-start-wt/DESIGN.md)（OKLCH 双主题 tokens、无自定义图标、Motion 仅 opacity/transform）、impeccable product register（词汇一致性、Restrained 色彩、状态完备）。

## Goals / 目标

1. 会话列表成为可辨识的门面：标题化、时间分级、active 态突出、按钮词汇统一、空状态可引导。
2. 课题面板回归克制风格：状态用语义色 tint（非色块），token 统一，i18n 完整，删除语义正确，按钮状态完备。

## Non-Goals / 非目标

- 不新增图标系统（保持 Unicode/内联 SVG 现状）。
- 不迁移其他组件（ToolPanel/PollerPanel/NeuronDetail 等）的本地 `.btn` 定义（记录为后续候选）。
- 不改后端数据模型与协议；会话标题仅由前端从 messages 提取。

## Acceptance Criteria / 验收标准

### SessionList

- [x] 列表项显示会话标题：首条 user/assistant 文本消息截断单行；无消息时显示占位「新会话」。
- [x] 会话 ID 仍可复制（copy-btn 保留），hover 辅助按钮显示。
- [x] 时间分级：今天 → `HH:MM`；昨天 →「昨天/Yesterday」；今年 → `M/D`；更早 → `YYYY/M/D`。
- [x] active 会话：淡 primary 背景 tint + 3px 指示条 + 标题加粗，与 hover 态可区分。
- [x] header 图标按钮去掉边框，改为无边框方形（26×26），与全项目 icon-btn 词汇一致。
- [x] 空状态：居中的引导符号 + 文案 + 主按钮（符合 DESIGN empty states）。
- [x] 运行中指示保留 pulsing dot；hover 辅助按钮背景改用 `--color-hover`（去除 `rgba(0,0,0,0.1)` 硬编码）。
- [x] 所有硬编码文案（Collapse/Expand/Copy/Close/Running/新会话/昨天）走 `t()`。

### TopicPanel

- [x] 状态 badge 改为「淡 tint 背景 + 语义色文字 + 圆点」模式（对齐 SessionList 的 mode-badge 词汇），不再用大面积色块。
- [x] 状态色全部使用语义 token：todo→`--color-text-muted`、in_progress→`--color-primary`、paused→`--color-warning`、done→`--color-success`、cancelled→`--color-error`；删除所有硬编码 hex 与不存在的 `--color-danger`。
- [x] 状态标签、过滤器、错误消息全部 i18n（新增 `topicPanel.topicStatus` Record 与 createFailed 等错误键，zh/en 完整）。
- [x] 删除入口按钮文案为「删除」，点击后进入确认态（确认/取消），不再以「确认」作为删除入口。
- [x] `.btn` 基础样式抽取到 app.html 全局，TopicPanel 删除本地重复定义；hover/focus/active/disabled 状态完备。
- [x] 过滤器按钮紧凑化（chips 间距统一），展开卡片间距/呼吸感微调。
- [x] 状态筛选由 6 个状态 chips 改为**三段式分段控件**：「进行中 / 全部 / 已完成」，单行不换行；聚合规则进行中 = todo+in_progress+paused、已完成 = done+cancelled；纯前端本地过滤（后端 `list_topics` 单状态参数与前端 bootstrap 全量拉取的数据流不变）。
- [x] `pnpm check` 0 errors、`pnpm build` 通过；zh/en 切换后文案即时更新。

## Requirement Decisions / 需求决策

- 2026-08-08：用户确认两个面板一起做、全面打磨。
- 会话标题提取规则：首个 `body.kind === "text"` 且 role 为 user/assistant 的消息内容（trim 后截断单行），优先 user；纯工程 ID 不再作为主显示，仅保留复制入口。
- 状态 badge 采用「点 + tint 底」而非彩色块：accent 仅承担「选中/状态指示」语义，符合 Restrained 策略。
- 2026-08-08（增量）：筛选控件由 6 个状态 chips 改为三段式。产品角度：按活跃度两级模型（进行中/已完成）符合用户「找活」心智；交互角度：筛选为高频常驻操作，三段式单步切换、状态常驻可见、误操作恢复成本≈0，优于 Select 的两步展开。文案用活跃度命名（「进行中」明确含 paused）规避合并语义的预期违背；「进行中」置首（默认选中仍为「全部」）。聚合纯前端 `includes` 完成，后端零改动。

## Design References / 设计参照

- mode-badge（SessionList 现有，chat/agent/assistant）作为状态 badge 的词汇基准。
- DESIGN.md「Empty states」作为空状态基准（居中、极简、单动作按钮）。
