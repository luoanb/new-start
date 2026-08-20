# git_pull

## 工具

从远程拉取并合并改动到活动仓库当前分支。需用户确认。等同 `git pull`。

参数：无。

返回 JSON：确认通过后 `{"cancelled": false, "result": {"ok": true}}`；用户拒绝为 `{"cancelled": true}`。

## 对模型的期待

- 拉取可能产生冲突：执行后建议再 `git_status` 检查是否存在 `conflicted` 条目。

## 忌用

- 工作区有大量未提交改动时先提醒用户，必要时先提交 / 暂存再拉取。

## 注意

- 需要远端凭据时复用系统 git 凭据。
- 用户拒绝返回 `cancelled: true`，尊重决定。
