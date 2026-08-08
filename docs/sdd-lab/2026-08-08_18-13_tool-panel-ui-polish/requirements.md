# Requirements / 需求文档: tool-panel-ui-polish

## Restated Understanding / 需求复述

- 用户反馈工具配置 UI 三个问题：
  1. **表单 select 未用封装组件**——编辑弹窗里 transport / method 用了原生 `<select>`，项目已有封装的 `Select.svelte`（键盘导航 + portal 浮层 + 视口钳制，其他面板在用），视觉与交互不一致。
  2. **disabled 开关语义不明、显示不全**——MCP server 的 `disabled`（停用）checkbox 标签在 `.field-row` 布局中被挤压，只露出首字母「d」；用户无法理解其作用。
  3. **多语言缺失**——项目已有完整 i18n（`$lib/i18n` 的 `t()` + `translations.ts` zh/en），但 ToolPanel 整个面板 + 编辑弹窗全部硬编码中英混合，未接入。
- 本次目标：整个 ToolPanel（列表区 + 编辑弹窗）统一接入 i18n；transport / method 换封装 `Select`；disabled 开关改为 toggle 开关。

## Scope / 范围

- In:
  - ToolPanel 列表区与编辑弹窗全部用户可见文案接入 `t()`（新增 `toolPanel` 命名空间，zh/en 完整）。
  - 编辑弹窗 transport / method 改用 `Select.svelte`。
  - MCP server `disabled` 开关改为 toggle（`role="switch"`），布局完整显示、语义清晰。
  - 前端 `pnpm check` / `pnpm build` 验证与文档回写。
- Out:
  - 后端配置 schema / 命令不变（纯前端展示改进）。
  - 其他面板的 i18n 补充（本次仅 ToolPanel）。
  - 「连接中」状态文案已在上迭代接入，不在本次范围。

## User Interaction / 用户交互

- 触发入口：工具面板（左侧列表区）；工具配置编辑弹窗。
- 用户操作路径（编辑弹窗）：打开弹窗 → transport/method 用统一下拉（键盘可用）→ 停用某 MCP server 用 toggle 开关（开 = 停用）→ 保存。
- 系统反馈：语言切换（LocaleSwitcher）后，工具面板与弹窗文案即时切换中/英。
- 异常/边界：toggle 开关有 `aria-checked` 语义，键盘可操作；窄弹窗下 toggle 标签不截断。
- 不应发生的交互：checkbox 标签被裁切只显示首字母；面板文案不随语言切换。

## Acceptance Criteria / 验收标准

- [x] 编辑弹窗 transport / method 使用 `Select.svelte` 封装组件（无原生 `<select>`）。
- [x] MCP server 停用开关为 toggle（`role="switch"` + `aria-checked`），标签完整显示、不截断，语义清晰（中文「停用」/ 英文「Disabled」）。
- [x] ToolPanel 列表区 + 编辑弹窗全部用户可见文案走 `t()`；`toolPanel` 命名空间 zh/en 完整（类型强制）。
- [x] 语言切换后 ToolPanel 即时刷新中/英文案。
- [x] 工具栏手动刷新按钮改名「重新加载 / Reload」（图标 + 文字，图标换 `↻`），语义与 `reassemble_tools` 重载动作一致。
- [x] 工具栏与弹窗内操作按钮统一为方形图标 + tooltip（新增 Tooltip.svelte）；保存/取消保留文字主按钮。
- [x] `pnpm check` 0 errors；`pnpm build` 通过。

## Constraints / 约束

- 技术约束：沿用 `$lib/i18n` 的 `t()` / `tMap()` 机制；沿用现有设计 token（--color-* / --space-* / --radius-*）；select 用现有 `Select.svelte`；toggle 为新增小组件（可复用、符合 a11y）。
- 业务约束：native / config / mcp、stdio / http 等为技术标识符，不翻译；服务端错误消息原样透传。
- 兼容性约束：不改变 `McpServerConfig` / `HttpToolConfig` / `CommandToolConfig` 结构；编辑保存语义不变。

## Open Questions / 开放问题

- [x] Q1 改进范围：**整个 ToolPanel 统一改**（列表区 + 弹窗全量 i18n + select 换封装）（已确认 2026-08-08 18:13）。
- [x] Q2 disabled 开关形式：**toggle 开关**（已确认 2026-08-08 18:13），非 checkbox。

## Requirement Decisions / 需求决策

- 2026-08-08 18:13:
  - 决策：整个 ToolPanel 接入 i18n（新增 `toolPanel` 命名空间）；transport/method 换封装 `Select`；disabled 改 toggle。
  - 原因：用户明确反馈三问题；项目已有 Select 封装与 i18n 系统，工具配置面板未接入属于遗漏；toggle 语义更清晰。
- 2026-08-08 18:30:
  - 决策：工具栏手动刷新按钮**改名保留**为「重新加载 / Reload」，图标由 `⟳` 换 `↻`，按钮形态改为图标 + 文字。
  - 原因：用户指出「刷新」语义过轻（实际是 `reassemble_tools` 子系统级重载，异步、超时、可能失败）；竞品（Cline / Cherry Studio）无全局刷新入口，用词为 Restart / Reload。保留全局入口以覆盖「外部改配置文件后手动重载」场景。
  - 备注：竞品对齐的「失败时显示重试」「server 行内重连」暂不做，列为后续候选。
- 2026-08-08 18:40:
  - 决策：工具栏「重新加载」「编辑配置」与弹窗内「添加」「删除」「关闭」按钮统一为**方形图标按钮 + tooltip**（新增 `Tooltip.svelte` 组件，hover/focus 显示，z-index 高于弹窗遮罩）；「保存」「取消」保留文字主按钮。
  - 原因：用户要求统一按钮视觉（方 icon + tooltip）；表单主操作（保存/取消）图标化会弱化语义，故保留文字。
