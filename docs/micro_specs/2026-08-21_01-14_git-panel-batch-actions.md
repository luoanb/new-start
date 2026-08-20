# Micro Spec: GitPanel 批量暂存/取消暂存

## Goal

* 要解决什么问题：Git 面板「更改」「暂存区」分组中，勾选框每次只对单个文件立即执行 stage/unstage，无法一次勾选多个文件批量操作。
* 验收结果：勾选多个文件后可一键批量暂存/取消暂存；分组头提供「全部暂存」「全部取消暂存」；操作后选择自动清理、状态刷新正常。

## Done Contract

* 什么算完成：
  1. 「暂存区」「更改」两个分组栏右侧新增**批量按钮**：更改 →「全部暂存」；暂存区 →「全部取消暂存」。
  2. 批量操作复用现有后端：`gitAdd([], true)` = `git add -A`；`gitUnstage([])` = `git restore --staged -- .`（已确认 repo.rs 行为）。
  3. 分组计数为 0 时按钮 disabled；点击后依赖 StateChange::Git 自动刷新。
  4. 勾选框维持现有「单文件立即操作」语义，不做多选改动（用户确认：栏上按钮即可批量）。
  5. i18n 新增中/英 key（stageAll / unstageAll）。
* 由什么证明：`pnpm --filter pulsar-app check` 0 error；App 内点击「全部暂存/全部取消暂存」→ 分组计数与状态即时刷新。
* 哪些情况仍算未完成：多选子集批量（本期不做）；「丢弃所选」批量撤销（不做）。

## Scope

* In：`packages/pulsar-app/src/lib/components/GitPanel.svelte`、`packages/pulsar-app/src/lib/i18n/translations.ts`。
* Out：后端 Rust 不动（`git_add`/`git_unstage` 已支持批量）；`GitDiff.svelte` / `FileExplorer.svelte` 不动；不新增「丢弃所选」。

## Facts / Constraints

* 后端已支持：`gitAdd(paths, all=true)` 全部暂存；`gitUnstage([])` 取消全部暂存；`gitAdd([...])` 批量暂存。
* 冲突分组勾选框为禁用态（冲突文件必须逐个处理），不改。
* 选择集用 `$state<string[]>`；刷新后经 `$effect` 按当前分组路径裁剪失效项。

## Change Log

- 2026-08-21 01:14: 初始 micro-spec。
- 2026-08-21（实现）：用户确认「分组栏上提供批量按钮」方案，多选子集不做。`GitPanel.svelte`「暂存区/更改」分组头新增 `.group-row` 布局 + `全部取消暂存/全部暂存` 按钮（`gitUnstage([])` / `gitAdd([], true)`，计数为 0 时 disabled）；i18n 新增 `stageAll` / `unstageAll`（中/英）。
- 2026-08-21（迭代）：用户反馈文字按钮不好，改为 icon——复用 `.op` 图标按钮 + inline SVG 14px（暂存区=双减号，更改=双加号），`title`/`aria-label` 用 i18n 文案。

## Validation

- Static checks：`pnpm --filter pulsar-app check` 0 error（20 既有 warning，GitPanel 无新增）。
- Runtime：待 App 内验证——git 面板点击「全部暂存/全部取消暂存」→ 分组计数与状态即时刷新；空分组按钮 disabled。
- 核心目标是否已由证据证明完成：静态检查通过；交互验证待用户确认。
