# git_stash

## 工具

对活动仓库执行 stash 操作。`action`：`push`（保存当前改动，可带 `message`）与 `apply`（应用最新 stash）直接执行；`pop`（应用并移除最新 stash）与 `drop`（丢弃最新 stash）需用户确认。

参数：

- `action`（必填）：`push` / `pop` / `drop` / `apply`。
- `message`（可选）：`push` 时附加的说明消息。

返回 JSON：直接执行 `{"action": "...", "ok": true}`；需确认场景通过后 `{"cancelled": false, "result": {"action": "..."}}`，拒绝为 `{"cancelled": true}`。

## 对模型的期待

- 需要暂时收起当前改动（如切换分支前）用 `push`。
- 执行 `pop` / `drop` 前先 `git_stash_list` 确认最新 stash 存在。
- 收起改动时给 `message` 说明内容，便于日后识别。

## 忌用

- `drop` 会永久丢弃 stash，仅当用户明确要求时使用（本工具已强制确认）。
- 不要在冲突未解决时 `pop` / `apply`（可能叠加冲突）。

## 注意

- `pop` / `drop` 作用于最新一条 stash（stash@{0}）。
- 用户拒绝返回 `cancelled: true`，尊重决定。
