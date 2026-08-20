# git_reset

## 工具

将活动仓库当前分支重置到目标提交（默认 HEAD）。`mode` 可选 `mixed`（默认）/ `soft` / `hard` / `keep`。需用户确认。等同 `git reset --<mode> [target]`。

参数：

- `mode`（必填）：`mixed` / `soft` / `hard` / `keep`。
- `target`（可选）：提交 / 分支名；缺省 HEAD。

危险提示：`hard` / `keep` 会丢弃工作区与暂存区的改动，属「危险写操作」——默认被开关关闭（需先在设置开启「危险写操作」），且始终需用户确认；确认弹窗会展示将丢失的文件清单。

返回 JSON：确认通过后 `{"cancelled": false, "result": {"lost": [...]}}`；用户拒绝为 `{"cancelled": true}`。

## 对模型的期待

- `soft` / `mixed` 用于撤提交但保留改动（安全，推荐首选）。
- `hard` / `keep` 仅在用户明确要求「放弃改动」时使用；若开关关闭会直接报错，不要绕路。
- 调用前先 `git_status` 说明将受影响的内容。

## 忌用

- 绝不默认使用 `hard`——它不可恢复地丢弃改动。
- 不要用 reset 撤销已推送到远端的提交（需 force push，本工具不提供）。
- 路径参数一律相对仓库根，绝对路径 / `..` 会被拒绝。

## 注意

- `hard` / `keep` 双重门禁：危险写开关 + 用户确认，缺一不可。
- 用户拒绝返回 `cancelled: true`，尊重决定，不要重试。
