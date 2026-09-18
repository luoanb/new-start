# Spec: main 区新面板默认落第 2 个分栏

## Goal

- 要解决什么问题：`insertPanel` 的默认 target 是 0，导致新打开的面板（文件 / git-diff / commit-diff / neurons / tool-editor / provider-manager）都塞进第 1 个分栏，与对话挤在同一个分栏里。
- 验收结果：非对话面板默认落到第 2 个分栏（索引 1，不存在则新建）；对话面板仍默认第 1 个分栏（索引 0）；显式传 target 的调用方行为不变。

## Done Contract

- 什么算完成：`insertPanel(type)` 未显式传 target 时，`type === "chat"` → 索引 0，其余 → 索引 1；`insertPanel("neurons", 2)` 硬编码清理为走默认。
- 由什么证明：`pnpm --filter pulsar-app check` 0 error；App 内 main 只有对话栏时点开文件，确认文件出现在新建的第 2 栏而非对话栏的 tab。
- 哪些情况仍算未完成：新面板仍落进对话所在分栏；显式 target 的调用（会话窗口 `SessionList`）行为被改坏；拖拽 `movePanelToNewPane` / `movePanel` 受影响。

## Scope

- In：`packages/pulsar-app/src/lib/layout/LayoutStore.svelte.ts`（`insertPanel` 默认 target 分流 + 注释）；`packages/pulsar-app/src/routes/+page.svelte`（`insertPanel("neurons", 2)` 去硬编码、两处注释同步）。
- Out（用户明确否决）：**对话栏锚定最左**（新建对话栏时 unshift 到索引 0）——本次不做，因此「main 为空时先开文件、后开对话」场景下第 1 栏不独占给对话；老布局的归一化迁移——不做，只对新插入生效。

## Facts / Constraints

- 落栏唯一入口是 [LayoutStore.svelte.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/layout/LayoutStore.svelte.ts#L175-L214) 的 `insertPanel`；[normalizePaneTarget](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/layout/layoutTypes.ts#L86-L97) 第 90 行 `target === undefined → 0`。
- 分栏顺序即数组顺序（索引 0 在最左）；空分栏会被移除（[MainPane 注释](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/layout/layoutTypes.ts#L27-L37)），所以「预留空栏」不可行。
- 显式传 target 的调用方：`SessionList.svelte` 第 147 行（会话窗口，传当前激活分栏索引）——必须保持原语义，只给 `undefined` 兜底。
- `insertPanel` 命中 `existing`（面板已存在）时仅激活、不新建，与默认落栏无关，不受本次改动影响。
- 既有同类硬编码：[+page.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/routes/+page.svelte#L389-L394) 第 393 行 `insertPanel("neurons", 2)`；dataStore 另两处 `insertPanel("neurons")` 走默认，两者当前不一致，本次统一。
- 布局持久化：`panes` 落盘，重启恢复；本次不做迁移，老布局维持原样。

## Restated Understanding

- 我理解当前任务是：把「新面板默认落第 1 栏」改成「默认落第 2 栏起」，第 1 栏留给对话。
- 当前核心目标是：新开的面板不再挤进对话所在分栏。
- 当前边界是：只改默认值分流与一处硬编码清理；不动分栏锚定、不动老布局、不动拖拽语义。
- 暂不处理：main 为空时先开文件后开对话导致的第 1 栏非对话问题（方案 2 范畴，用户明确本次不做）。

## Checkpoint Summary

- 当前任务理解：`insertPanel` 默认 target 按类型分流（chat→0，其余→1）。
- 当前核心目标：非对话面板默认落第 2 栏。
- 当前进度：实现完成，静态检查通过，待人工确认。
- 涉及文件 / 模块：`packages/pulsar-app/src/lib/layout/LayoutStore.svelte.ts`、`packages/pulsar-app/src/routes/+page.svelte`。
- 风险：`normalizePaneTarget` 在 `panes.length === 0` 时会把 1 收敛为 0（main 为空时首个非对话面板仍占第 1 栏）——已知且接受；`insertPanel("neurons", 2)` 改为默认后落点由第 3 栏变第 2 栏。
- 验证方式：`pnpm --filter pulsar-app check`；App 内手动验证落栏。
- Execution Approval: `Approved`（2026-09-18，用户选定方案 1 / 老布局不动 / 清理 neurons 硬编码）

## Change Log

- 2026-09-18: 初始记录并实现。无与 spec 的偏差。
  落地细节：默认值内联在 `insertPanel`（`const fallbackTarget = type === "chat" ? 0 : 1;`），未新增导出常量——只有这一处消费点。同步改了 `insertPanel` 的 JSDoc 与 `+page.svelte` 两处注释（tool-editor「默认第 0 栏」→「默认第 2 栏」；`insertPanel("neurons", 2)` 去硬编码）。

## Validation

- Self-check: 已实现（默认 target 分流 + neurons 硬编码清理 + 注释同步）。
- Static checks: `pnpm --filter pulsar-app check` → 0 errors / 20 warnings（与既有基线一致）。
- Runtime / Test: 未跑自动化（`packages/pulsar-app` 无 `insertPanel` / `normalizePaneTarget` 的测试文件）。
- Human confirmation: 待用户 App 内确认。
- 核心目标是否已由证据证明完成：否（差人工确认）。

## Resume / Handoff

- 当前状态：实现完成，静态检查通过，待人工确认。
- 下一步唯一动作：App 内确认——清空 main 后只开对话，再点开一个文件，文件应出现在**新建的第 2 栏**，而不是变成对话栏里的 tab；再点开第二个文件，应成为第 2 栏的 tab。
