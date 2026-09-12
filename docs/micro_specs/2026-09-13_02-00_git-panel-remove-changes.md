# Git 面板更改区：移除单个 / 批量移除文件更改

日期：2026-09-13
状态：已实现（工作区未提交）
分支 / worktree：`feature/code` @ `/home/lab/Documents/trae_projects/new-start`

## 背景

Git 面板「更改（Changes）」区此前只支持「暂存（＋）」与「全部暂存」，缺少把文件
从更改列表中移走的手段（VSCode SCM 的 `Discard Changes` / 删除未跟踪文件）。
本次补齐**单个移除**与**批量移除**两个入口。

## 语义（关键设计决策）

「更改」区 = `unstaged` + `untracked`。两类文件的「移除」在 git 层语义不同，
不能用一个 `git restore` 通吃：

| 类别 | status | 移除动作 | 效果 |
|---|---|---|---|
| tracked | ` M` / ` D` / ` R` … | `git restore -- <paths>` | 丢弃工作区改动，**文件保留** |
| untracked | `??` | `git clean -f -d -- <paths>` | **删除文件本体**（不可还原） |

因此新增后端操作 `remove_changes(tracked, untracked)`，两组路径一次调用处理，
而非复用既有 `restore`（后者只覆盖 tracked）。前端按 `status.trim() === "??"`
分组后传入。

## 改动清单

### 后端（Rust）

| 文件 | 内容 |
|---|---|
| `fileops/gitops/mod.rs` | `GitBackend` trait 新增 `remove_changes(repo, tracked, untracked)` 声明 |
| `fileops/gitops/repo.rs` | `CliGitBackend` 实现；tracked 走 `restore --`，untracked 走 `clean -f -d --`；两组合空 → `InvalidInput`（不静默成功）；路径逐个过 `validate_rel_path` 防逃逸/选项注入 |
| `lib.rs` | `#[tauri::command] git_remove`，走 `GitConfirmService`（`GitOpKind::Checkout`，标题「移除文件更改」），确认后执行并 `StateChange::Git`；注册进 `generate_handler!` |
| `net/rpc.rs` | `"git_remove"` 派发分支 + `GitRemoveParams { tracked, untracked }` |

### 前端（Svelte / TS）

| 文件 | 内容 |
|---|---|
| `lib/api/contracts.ts` | `gitRemove: def<{ tracked: string[]; untracked: string[] }, void>("git_remove")` |
| `lib/stores/dataStore.svelte.ts` | `gitRemove(tracked, untracked)`：两组合空时直接 return 不发请求；否则调用并 `refreshGit()` |
| `lib/components/GitPanel.svelte` | `isUntracked` / `splitRemovable`（去重、剔除空路径）；`removeChange(e)` 单条；`removeAllChanges()` 批量；UI 加单条 `✕`（`.op.danger`）与分组头批量 `✕`（`.group-act.danger`，`disabled={allChanges.length === 0}`） |
| `lib/components/GitConfirmHost.svelte` | `Checkout` 分支识别 `tracked`/`untracked`，渲染 `removeConfirmTitle/Body`，并追加未跟踪告警行 |
| `lib/i18n/translations.ts` | 新增 `removeChange`、`removeAllChanges`、`removeConfirmTitle`、`removeConfirmBody`、`removeConfirmUntracked`（zh + en） |

复用既有样式：`.op.danger:hover { color: var(--color-error) }` 已存在，未新增 CSS。

## 边界情况处理

- **空输入**：前端提前 return；后端返回 `InvalidInput`（双保险，不静默失败）。
- **空列表 / 全选**：按钮 `disabled={allChanges.length === 0}`，点击函数内再判一次。
- **去重**：`splitRemovable` 用 `Set` 去重，避免同一路径在 tracked/untracked 重复传入。
- **路径安全**：`validate_rel_path` 拒绝绝对路径与 `..` 逃逸；`clean` 用 `-f -d --`，`--` 之后全为路径。
- **失败提示**：经 `run()` 捕获写入 `error-bar`（`formatInvokeError`），非静默。
- **确认门禁**：不可逆操作，强制走全局确认弹窗，`danger` 样式 + 未跟踪文件额外提示。

## 验证

### 自动化（已通过）

```bash
cd packages/pulsar-app/src-tauri
cargo test --lib fileops::gitops      # 38 passed / 0 failed
npx --prefix packages/pulsar-app svelte-check --threshold error   # 0 errors
```

新增 5 个集成用例（`repo.rs::tests`）：

| 用例 | 覆盖 |
|---|---|
| `remove_changes_restores_tracked_file` | 单文件 tracked：内容回退 HEAD，文件仍存在 |
| `remove_changes_deletes_untracked_file` | untracked：文件被删除，状态清空 |
| `remove_changes_handles_tracked_and_untracked_together` | 批量：两类混用一次生效 |
| `remove_changes_rejects_empty_paths` | 空输入报错（非静默） |
| `remove_changes_rejects_escaping_paths` | `../`、绝对路径被拒（tracked/untracked 两路） |

### 手动验证步骤

准备：在一个有改动的仓库打开 Git 面板，「更改」区应含 1 个 modified + 1 个 untracked。

1. **单文件移除（tracked）** — 点该 modified 条目的 `✕`。
   预期：弹确认框「移除文件更改」，列出 1 个文件；确认后条目消失，文件仍在磁盘且内容回到 HEAD。
2. **单文件移除（untracked）** — 点 untracked 条目的 `✕`。
   预期：确认框额外含「其中 N 个为新增未跟踪文件…」；确认后条目消失，**文件被删除**。
3. **批量移除** — 有 ≥2 个更改时，点分组头「更改 (N)」右侧的批量 `✕`。
   预期：确认框列出全部路径（tracked + untracked），确认后「更改 (0)」，显示 `git.clean`。
4. **空列表防护** — 更改区为空时批量 `✕` 应 `disabled`，点击无反应、无报错。
5. **取消确认** — 任一步弹窗点取消。
   预期：无任何变更，列表不变（确认服务返回 Approved 才执行）。
6. **失败提示** — 断开仓库/置于只读。
   预期：顶部 `error-bar` 显示错误，列表不变（非静默失败）。

## 注意（环境坑）

本仓库存在两个 worktree：

| 路径 | 分支 |
|---|---|
| `/home/lab/Documents/trae_projects/new-start` | `feature/code` ← **本次改动在此** |
| `/home/lab/Documents/trae_projects/new-start-wt` | `main` |

两者是**独立工作目录**，同分支名的文件内容互不影响。shell 默认落在
`new-start-wt`，而 IDE 文件工具落在 `new-start`，曾导致「代码丢失」的误判。
核对功能代码时请显式 `cd` 到目标 worktree，或 `git -C <path>`。
