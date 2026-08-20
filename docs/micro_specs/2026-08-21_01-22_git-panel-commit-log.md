# Micro Spec: GitPanel 提交记录列表 + 提交 diff

## Goal

* 要解决什么问题：Git 面板看不到提交记录（commit history）。后端 `git_log` 与 `state.git.log` 数据已就绪，但 UI 未渲染。
* 验收结果：面板新增「提交记录」折叠区段，展示最近提交（短哈希 / subject / 作者 / 日期）；点击提交展开该次提交的变更文件列表（+n/-n）；点击文件就地渲染该文件在该提交的 unified diff。

## Done Contract

* 什么算完成：
  1. `GitPanel.svelte` 新增「提交记录」折叠区段（默认收起），渲染 `git?.log`：短哈希（等宽）、subject 主文字、author + date 次要信息。
  2. 点击提交（懒加载 `git_show_files`）展开变更文件列表：path + `+n -n` 统计（二进制标记）；再点击文件（懒加载 `git_show_diff`）就地渲染 unified diff（hunk 头 + 增删行着色）。
  3. 切换仓库 / 收起提交时重置展开状态；空态与加载态提示。
  4. i18n 新增中/英 key（groupLog / logEmpty / logLoading）。
* 由什么证明：`cargo check` + `cargo test`（28 passed，含新增 numstat 解析与 rev 校验测试）+ `pnpm --filter pulsar-app check` 0 error；App 内展开「提交记录」看到历史提交与逐文件 diff。
* 哪些情况仍算未完成：分支图/提交图；提交 diff 复用 main 区 GitDiff 面板（本期就地渲染）。

## Scope

* In：
  - 后端：`src-tauri/src/fileops/gitops/mod.rs`（`GitShowFile` 结构 + trait 方法）、`repo.rs`（`show_files` / `show_diff` + `parse_numstat` + `validate_rev`）、`lib.rs`（`git_show_files` / `git_show_diff` command）、`net/rpc.rs`（rpc 分发 + params）。
  - 前端：`lib/types.ts`（`GitShowFile`）、`lib/stores/dataStore.svelte.ts`（`gitShowFiles` / `gitShowDiff`）、`lib/components/GitPanel.svelte`（提交记录区段）、`lib/i18n/translations.ts`。
* Out：不改 AI 工具注册（`tools.rs` 的 `git_show` 留待 AI 侧需求）；不做分支图；diff 不打开 main 区面板。

## Facts / Constraints

* 后端复用现有 `parse_diff` 解析 `git show`（unified 同格式）→ `GitFileDiff`；`git_show_files` 用 `--numstat --no-renames`。
* `validate_rev` 拒绝空 / 前导 `-` / 空白（防选项注入）；文件路径复用 `validate_rel_path`。
* 数据源：`state.git.log` 随 `refreshGit()` 刷新；文件列表与 diff 懒加载不入全局 state。
* 复用现有折叠区段与 `--color-success/error` token 着色，不新增 design token。

## Change Log

- 2026-08-21 01:22: 初始 micro-spec。
- 2026-08-21（实现）：用户选择「列表 + 提交 diff」。后端新增 `git_show_files` / `git_show_diff`（tauri + rpc），`GitShowFile` 结构，`parse_numstat` / `validate_rev`；前端 GitPanel 新增「提交记录」区段（提交列表 → 变更文件 → 就地 diff），dataStore 新增懒加载封装；i18n 三 key 中/英。
- 2026-08-21（验证）：`cargo check` 0 error；`cargo test --lib fileops::gitops` 28 passed（新增 `numstat_parses_fields`、`rev_validation_rejects_options_and_whitespace`）；真实 `git show --numstat` 输出与解析器匹配；`pnpm --filter pulsar-app check` 0 error（20 既有 warning，无新增）。
- 2026-08-21（样式迭代，对齐 visual-design §Icon/§1）：分组头文本 `▸` 换 SVG chevron（FileExplorer 同款 polyline）+ 14px 语义 icon（冲突=alert / 暂存=加号方块 / 更改=铅笔 / 提交记录=时钟 / 分支=git-branch / Stash=box）；commit-item 由竖排三行 46px 改为单行 28px（short 等宽 primary + subject 省略 + meta 次要右弱化）；commit-file 文件名（主）在前 + 目录路径（muted 小字）弱化跟随右侧。
- 2026-08-21（样式迭代 2）：①暂存区/更改/冲突条目的文件名统一「basename 在前 + dir muted 弱化跟随」（splitPath 兼容目录尾斜杠）；②分组头字号 `--fs-xs` → `--fs-sm`（与内容主文字持平，600 加粗 + muted 区分层级），消除「内容比分组标题大」的不协调。
- 2026-08-21（徽标可读性）：状态徽标 hover 改用项目 Tooltip 组件（portal 到 body，规避原生 title 在 webview 不显示/慢显示）；`.badge` 显式 `cursor: default`（消除文本 I-beam 光标）；badge span `title=""` 阻止冒泡显示父级 .item 的文件路径提示，避免双提示冲突；i18n 新增 `git.status` 映射（??/M/A/D/R/U/MM/AM/AD/DD/UU），未映射码不显示提示。
- 2026-08-21（徽标可读性 2 / 条目精简）：①所有状态码均有 hover 提示——精炼映射按 trim 后匹配，未命中用 `git.states` 单字符表拆解双字符码兜底（`statusTemplate`："暂存区：{x} / 工作区：{y}"），badge snippet 恒包 Tooltip；②分组头字号已与内容统一为 `--fs-sm`（与 FileExplorer 全面板 fs-sm 一致），无需再调；③"更改"分组更名"工作区"（en: Worktree）；④删除条目 checkbox（与右侧 ＋/− 暂存操作重复），同步删除 `.item input[type="checkbox"]` 样式。
- 2026-08-21（工作区分组为空 Bug 修复）：根因——`push_status_entry` 用 if/else 分支分类，`MM`/`AM` 等「暂存区+工作区都有改动」的双状态文件只进 staged，不显示在工作区（allChanges=unstaged+untracked）。改为暂存区/工作区独立维度判断（VS Code SCM 语义）：`MM` 同时出现在两个分组；reset --hard 预览收集 lost 路径去重（MM 会重复出现）。新增 `status_dual_state_appears_in_both_groups` 测试；`cargo test --lib fileops::gitops` 29 passed。
- 2026-08-21（分组命名 + diff 默认范围）：①分组名 zh「暂存区」→「暂存」、「工作区」→「更改」，en groupChanges 恢复「Changes」；②git-diff 面板 key 扩展为 `git-diff:${repoId}:${relPath}:${range}`，GitDiff 从 key 尾段解析初始 range（旧 key 无尾段 → unstaged 兼容布局恢复）；GitPanel 按来源分组传 range：暂存→staged / 更改→unstaged / 冲突→both；FileExplorer 打开 diff 默认 unstaged；面板内 range Select 仍可手动切换。Blame 按钮（git blame：逐行最后修改的提交/作者/日期）为既有功能，未改动。
- 2026-08-21（文案统一 + blame 死锁修复）：①diff range 选项与状态提示文案统一「暂存 / 更改」（en Changes），diff 面板 rangeSelect 选项「暂存（vs HEAD）」「更改（vs 暂存）」；②根因：`run_git` 先 `child.wait()` 再 `read_to_end` 读 stdout，git blame porcelain 输出（repo.rs ≈300KB）超过管道缓冲（约 64KB）后子进程阻塞无法退出 → wait 超时（App 与浏览器一致报 "timed out after 30000ms"，命令行实测仅 0.007s）。修复：wait 与 stdout/stderr 读取并发（`timeout(async { tokio::join!(...) })`），避免管道死锁；blame 改用 `stdout_bytes` 完整输出解析（truncate 64KB 会丢尾部行）；前端 blame 加载中显示「分析行归属中…」（原为空白=白屏）。`cargo test --lib fileops::gitops` 29 passed。

## Validation

- 核心目标是否已由证据证明完成：静态检查 + 单元测试通过；App 内交互验证待用户确认。
