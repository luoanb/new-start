# git_resolve_conflict

## 工具

解决活动仓库中单个文件的合并冲突。`take`：`ours`（保留我方）/ `theirs`（保留对方）/ `both`（合并双方并保留冲突标记）。写操作，无需确认。等同 `git checkout --ours/--theirs` 或手工合并 + `git add`。

参数：

- `path`（必填）：冲突文件路径（相对仓库根）。
- `take`（必填）：`ours` / `theirs` / `both`。

返回 JSON：`{"path": "...", "take": "...", "ok": true}`。

## 对模型的期待

- 仅在 `git_status` 显示 `conflicted` 条目、且用户明确选择保留哪一侧时调用。
- `both` 会写入带冲突标记的合并内容（保留双方），适合需要人工再编辑的场景。
- 解决后建议 `git_status` 确认冲突清空，再让用户 `git_add` + `git_commit` 收尾。

## 忌用

- 冲突未解决时不要 `git_commit` / `git_add` 其他文件一起提交。
- 不确定该保留哪侧时先询问用户，不要自作主张。
- 路径不得为绝对路径或含 `..`（会被拒绝）。

## 注意

- 本工具只解决单文件冲突；多个冲突文件需逐个处理。
- `both` 的产物仍需用户 review，别直接当作最终结果。
