# Spec: 对话用户轮目录（右侧悬浮索引条）

## Goal

- 要解决什么问题：长会话只有一个滚动流，想回到「第 N 个问题」只能一路往上滚；会话内部没有自己的导航入口。
- 验收结果：会话消息区右缘出现一条「用户轮目录」——每个用户介入轮一个刻度，hover / 点击展开可读的轮列表，点击任一项跳到该轮（对齐顶部 + 既有 `locate-flash` 高亮），滚动时当前轮自动高亮。

## Done Contract

- 什么算完成：目录按已加载窗口内的轮次渲染（**只要 ≥1 轮就渲染**）；点击项定位到该轮首条消息并对齐容器顶部；scrollspy 高亮当前轮；「滚动到末尾」按钮**无条件渲染**（0 轮 / 空会话也展示）。
- 由什么证明：`pnpm --filter pulsar-app check` 0 error；App 内长会话（≥3 轮）手动验证跳转、高亮、hover 展开、滚动到末尾；空会话确认末尾按钮可见。
- 哪些情况仍算未完成：目录点击后不跳转或跳错轮；只渲染了已加载窗口却声称覆盖全量历史；窄窗口下遮挡消息正文不可读；「滚动到末尾」按钮点了不落到最新消息；0 轮时按钮消失。

## Scope

- In：`ChatArea.svelte`（目录视图 + scrollspy + 复用定位）；`i18n/translations.ts` 新增文案；纯展示 CSS。
- Out：后端接口 / 落库（目录只覆盖**已加载消息窗口**）；AI 生成轮标题；滚动虚拟化；绑定窗口（`chat:<id>`）之外的会话列表导航。

## Facts / Constraints

- 轮次定义已存在：[ChatArea.svelte](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src/lib/components/ChatArea.svelte#L181-L207) 的 `rounds`（以 `role=user` 且 `body.kind` 非 `nudge` / `role_context` 为轮起点），目录与消息区必须同源，不得另立口径。
- 消息是**分页窗口**：`messagesOffset` = 窗口首条消息的绝对下标，`hasMore` / `loadOlderMessages` 支持前插。`rounds[i].startIndex` 是**窗口内**下标；绝对下标 = `messagesOffset + startIndex`。
- 定位能力已存在：`locateToMessage(absIndex)`（最多 20 帧轮询 DOM，命中后 `scrollToTopOf` + `locate-flash`；未命中且 `hasMore` 时续拉更早页），目录点击直接复用，不重复实现。
- 消息 DOM 锚点是 `data-message-index={anchorIndex}`（[ChatMessage.svelte](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src/lib/components/ChatMessage.svelte#L107)）。
- `.chat-area` 是 flex 纵列（`.messages` 滚动容器 + `ChatInput`）；目录必须是**浮层**（absolute 定位在消息区右缘），不能进入 `.messages` 参与滚动。
- 窗口首组可能是「前导非 user 消息」（历史开头被截断），该组没有用户输入，目录需跳过。
- i18n 为强类型多语言（`translations.ts` 中 `chatArea` 段 + zh/en 等多份），新增 key 需同步补齐。

## Open Questions

- [x] Q1 形态：**右侧悬浮索引条**——常驻细刻度条，hover / 点击展开轮列表；不占布局宽度。
- [x] Q2 范围：**仅当前已加载窗口**——复用前端 `rounds`，零后端改动；上滑加载更早消息后目录自动补齐。
- [x] Q3 条目文本：**用户输入首行截断**（单行省略号），不生成 AI 标题。

## Restated Understanding

- 我理解当前任务是：给会话消息区加一个**轮次导航器**，把已加载的每个用户介入轮列成可点击的索引。
- 当前核心目标是：让用户「一眼看到这段对话问了哪些问题，并能一步跳过去」。
- 当前边界是：纯前端展示层；不改消息数据、不改分页策略、不新增后端接口；`nudge` / `role_context` 仍不作为轮起点。
- 暂不处理：全量历史目录（需后端轻量接口）、AI 标题、跨会话导航、移动端专门适配。

## 接口契约设计

- 前端（`ChatArea.svelte`）新增派生数据与状态：

  ```ts
  /** 目录项：一对一映射可导航的轮（跳过无用户输入的前导分组）。 */
  type OutlineItem = {
    absIndex: number; // 窗口偏移补偿后的绝对下标，供 locateToMessage 使用
    roundIndex: number; // rounds 内下标，供 scrollspy 与 DOM 对应
    text: string; // 用户输入首行，截断展示
  };
  const outlineItems = $derived.by<OutlineItem[]>(...);

  let activeRoundIndex = $state(-1); // scrollspy 当前轮（rounds 下标）
  let scrollbarW = $state(0); // 覆盖层避让滚动条的像素偏移
  ```

- 行为约定：
  - 点击目录项 → `void locateToMessage(item.absIndex)`（复用既有高亮与续拉，不新增滚动逻辑）。
  - scrollspy：在既有 `handleScroll` 中按 `.message-round` 的 `getBoundingClientRect().top` 与容器顶部比较，取「最后一个顶部已越过容器顶部」的轮为当前轮；不新增独立滚动监听。
  - 展开态：默认细刻度条（每轮一个刻度 + 当前轮高亮）；展开由 CSS 驱动——`.outline-rail:hover ~ .outline-panel`、`.outline-rail:focus-within ~ .outline-panel`、`.outline-panel:hover`，浮层 `right: 100%` 与刻度条紧贴以免指针移入时 hover 断链；悬停「滚动到末尾」按钮不展开。触屏点刻度即直接跳转。浮层内列表可滚动，条目为单行截断。
  - 渲染条件：刻度条与浮层在 `outlineItems.length > 0` 时渲染（仅 1 轮也展示）；「滚动到末尾」按钮**无条件渲染**——`0` 轮 / 空会话时容器只剩该按钮（`aria-label` 随之切换为末尾按钮文案）。
  - 「滚动到末尾」：目录列底部独立图标按钮，点击 `el.scrollTo({ top: scrollHeight, behavior: "smooth" })` 落到最新消息；不判断「是否已在底部」（保持简单，不做按压态以外的状态机）。
  - 布局：新增 `.messages-wrap`（`position: relative`）作为定位上下文，刻度条贴消息区右缘，`right` 由 JS 按滚动条占宽（`offsetWidth - clientWidth`）内联避让；指针事件仅作用于刻度与浮层，不遮挡正文与滚动条拖拽。刻度条与末尾按钮构成垂直居中列，刻度多时由刻度条自身收缩裁切，按钮始终可见。

- i18n：`chatArea` 段新增 `outlineTitle` / `outlineUntitled` / `outlineJumpEnd`（en / zh 同步）。

## Checkpoint Summary

- 当前任务理解：给会话加用户轮目录（右侧悬浮索引条），覆盖已加载窗口，条目取用户输入首行。
- 当前核心目标：长会话可一步跳回任意已加载轮。
- 当前进度：代码完成，静态检查通过，待 App 内人工确认。
- 下一步 1：用户运行 App，用 ≥3 轮会话验证跳转 / 当前轮高亮 / hover 展开。
- 涉及文件 / 模块：`packages/pulsar-app/src/lib/components/ChatArea.svelte`、`packages/pulsar-app/src/lib/i18n/translations.ts`。
- 风险：滚动条避让依赖 `offsetWidth - clientWidth` 实测值；scrollspy 依赖 `.message-round` DOM 顺序与 `rounds` 一致；刻度过多时超出可视高度会被裁掉（未做分页/缩放）。
- 验证方式：`pnpm --filter pulsar-app check`；App 内长会话手动验证跳转、高亮、展开收起。
- Execution Approval: `Approved`（2026-09-15）

## Change Log

- 2026-09-15: 初始记录。口径定为「右侧悬浮索引条 + 仅已加载窗口 + 用户输入首行截断」。
- 2026-09-15: 实现落地。与 spec 的两处偏差（Reverse Sync 记录）：
  1. 不再引入 `outlineOpen` 状态——浮层显隐改由 CSS `:hover` / `:focus-within` 驱动（避免在非交互元素上挂点击语义与 a11y 告警）；触屏点刻度即为跳转。
  2. 目录项空文本 i18n key 定为 `outlineUntitled`（原计划的 `outlineEmpty` 未使用）。
  另：为承载浮层新增 `.messages-wrap`（`position: relative`）包裹 `.messages`，`.messages` 自身样式不变。
- 2026-09-15（需求变更，用户指令）：① 渲染阈值从 `>= 2` 放宽为 `> 0`（**仅 1 轮也展示目录**）；② 目录列尾新增「滚动到末尾」图标按钮（`scrollToLatest`，`chatArea.outlineJumpEnd` 文案）。
  与之相关的实现细节：`.turn-outline` 由横排改竖排（刻度条 + 按钮）；刻度条 `padding` 左右加宽到 8px 使浮层 `right: 100%` 与之紧贴；展开选择器由 `.turn-outline:hover` 收窄为「刻度条 hover / 聚焦 + 浮层自身 hover」，避免悬停末尾按钮时弹开轮列表。
- 2026-09-15（需求变更，用户指令）：③ 容器 `nav` 改为**无条件渲染**，刻度条与浮层各自按 `outlineItems.length > 0` 条件渲染——**0 条（含空会话）时只保留「滚动到末尾」按钮**；此时 `nav` 的 `aria-label` 切换为末尾按钮文案。

## Validation

- Self-check: 已按方案实现（`outlineItems` / scrollspy / `locateToMessage` 复用 / 刻度条 + 浮层 / 滚动条避让 / 「滚动到末尾」按钮无条件渲染 / i18n）。
- Static checks: `pnpm --filter pulsar-app check` → 0 errors（20 条既有 warning，`ChatArea.svelte` 无新增问题）。
- Runtime / Test: 未跑自动化（纯前端展示层，无单测覆盖点）。
- Human confirmation: 待用户 App 内确认。
- 核心目标是否已由证据证明完成：否（差人工运行确认）。
- 剩余风险：全量历史目录需要后端轻量接口，本次明确不做；刻度条在轮次极多时会被裁切。

## Resume / Handoff

- 当前状态：实现完成，静态检查通过，待人工确认。
- 当前卡点：App 内人工确认。
- 下一步唯一动作：启动应用——空会话 / 0 轮时确认右缘只剩「滚动到末尾」按钮且可见；有轮次时 hover 刻度条看轮列表、点击任一项看是否跳到该轮并闪烁高亮、滚动看当前轮是否跟随高亮、点末尾按钮看是否落到最新消息。
- 下一轮核心目标：若窗口外历史也需要出现在目录中，再单开一轮做后端轻量目录接口。
