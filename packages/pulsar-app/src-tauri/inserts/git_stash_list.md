# git_stash_list

## 工具

列出活动仓库的 stash 记录（序号与消息）。只读操作，等同 `git stash list`。

参数：无。

返回 JSON：数组，每条含 `index`（stash@{n} 的 n）、`message`。

## 对模型的期待

- 执行 `git_stash` 的 pop / drop 前先确认最新 stash 是否存在及内容。

## 忌用

- 不要用本工具执行 pop / drop——那属于写操作，走 `git_stash` 且 pop/drop 需用户确认。

## 注意

- 只读，不会改动任何文件。
