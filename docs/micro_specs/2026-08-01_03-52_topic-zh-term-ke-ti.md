# Spec: Topic 中文术语统一为「课题」

## Goal

- 要解决什么问题：领域对象 `Topic` 的中文译名不一致（多为「话题」，个别产品文案也混用），与既有真相源（`docs/specs/2026-07-26_14-45_topic-manager.md` 等已用「课题」）冲突。
- 验收结果：面向用户的中文 UI 与活跃产品/需求文档中，凡指代 `Topic` 一律称「课题」；不改动 UI Theme（主题切换）等无关「主题」用语；英文仍为 Topic。

## Done Contract

- 什么算完成：`translations.ts` 中文 `topicPanel.*` 全部用「课题」；仓库内指代 Topic 的「话题」已改为「课题」；UI Theme 相关「主题」保持不变。
- 由什么证明：`rg '话题' --glob '*.{md,ts,svelte,rs}'` 无 Topic 语义命中；`rg '课题'` 覆盖原 Topic 中文文案；必要时目视 GUI 中文面板标签。
- 哪些情况仍算未完成：历史归档若刻意保留旧称（本任务不刻意保留）；英文文案；代码标识符 `topic` / `Topic`。

## Scope

- In:
  - `packages/agent-app/src/lib/i18n/translations.ts`（zh `topicPanel`）
  - `PRODUCT.md` 中 Topic 语义的「话题」
  - 仍使用「话题」指代 Topic 的 sdd-lab / 需求文档（主要：`agent-app-topic-poller-mgmt`、`agent-app-neuron-mgmt`、`agent-app-gui-redesign`）
- Out:
  - UI Theme / 颜色「主题」、PRD「主题」、图表配色「主题」
  - 英文 `Topic` / 代码符号重命名
  - 后端错误消息英文化改造

## Restated Understanding

- 当前任务理解：统一 `Topic` → 中文「课题」。
- 当前核心目标：消除「话题」与「课题」混用，以「课题」为准。
- 当前边界：只改中文用词；不碰 Theme「主题」；不改英文与标识符。
- 暂不处理：历史 commit 文案、非本仓库材料。

## Facts

- 真相源已用「课题」：`docs/specs/2026-07-26_14-45_topic-manager.md`、`docs/micro_specs/2026-07-29_00-34_topic-scope-item-management.md`、`docs/sdd-lab/2026-07-26_21-30_assistant-mode/*`。
- GUI 中文仍用「话题」：`translations.ts` 的 `topicPanel.topics/create/createTitle/noTopics/deleteConfirm`。
- 检索未发现用「主题」指代 `Topic`；用户感知的「主题」多半来自 Theme 或口语混用。本任务不改 Theme「主题」。

## Plan

1. 改 i18n 中文：话题 → 课题。
2. 批量替换文档中指代 Topic 的「话题」→「课题」（含 `Topic（话题）` → `Topic（课题）`）。
3. `rg` 验收；回写本 spec Validation。

## Validation

- `rg '话题' --glob '*.{md,ts,svelte,rs}'`：仅本 micro-spec 在叙述历史问题时仍出现「话题」字样；产品/UI/sdd-lab 中 Topic 语义已无「话题」。
- `translations.ts` zh `topicPanel`：topics/create/createTitle/noTopics/deleteConfirm 均为「课题」。
- Theme「主题」未改动。

## Change Log

- 2026-08-01：创建 micro-spec。
- 2026-08-01：已执行——i18n 中文 + PRODUCT + 相关 sdd-lab「话题」→「课题」；核心目标由检索证据证明完成。
