# git_status

## 工具

查看活动仓库的工作区状态：当前分支、领先/落后远端提交数、已暂存 / 未暂存 / 未跟踪 / 冲突文件清单。只读操作，等同 `git status`。

参数：无。

返回 JSON：`branch`（可空）、`ahead`、`behind`、`staged` / `unstaged` / `untracked` / `conflicted` 四组条目（每条含 `path`、`status`、`is_dir`）。

## 对模型的期待

- 动手提交 / 暂存 / 撤销前先调用本工具摸清工作区状态。
- 依据 `staged` / `unstaged` 判断改动归属，再决定后续操作（add / commit / restore / reset）。
- 存在 `conflicted` 条目时优先提示用户处理冲突，不要贸然提交。

## 忌用

- 不要在无仓库的工作区硬调（会报错「no git repo found」）。
- 不要用本工具判断历史提交信息——历史用 `git_log`。

## 注意

- 作用于当前 active 仓库（`git_set_active_repo` 指定；未指定时回落工作区内第一个仓库）。
- `status` 为 X/Y 组合码（如 `MM`、`??`、`UU`）；`is_dir` 由前端目录聚合使用。
- 只读，不会改动任何文件。
