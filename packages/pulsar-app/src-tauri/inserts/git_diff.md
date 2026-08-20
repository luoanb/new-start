# git_diff

## 工具

查看活动仓库的 diff：默认未暂存（工作区）改动，传 `cached=true` 看已暂存改动；可指定单文件路径。只读操作。

参数（均为可选）：

- `path`：限定到单个文件（相对仓库根）。
- `cached`：`true` 表示查看暂存区 diff（`--cached`）；缺省 / `false` 为未暂存 diff。

返回 JSON：`files`（每条含 `path`、`status`、`is_binary`、`hunks`——hunk 含行号与逐行 `kind`：`context` / `add` / `del`）、`truncated`（输出超限被截断标记）。

## 对模型的期待

- 提交前先看暂存区 diff（`cached=true`）确认将要提交的内容。
- 修改代码前看未暂存 diff 理解改动上下文。
- 二进制 / LFS 指针文件会标记 `is_binary`，正文为空，不要误以为无改动。

## 忌用

- 不要用 diff 替代 `git_status` 查看整体改动分布（diff 重在内容，status 重在分类）。
- 超大仓库输出会被截断（`truncated: true`），不要据此断言「无改动」。

## 注意

- `path` 必须相对仓库根；绝对路径与 `..` 逃逸会被拒绝。
- 只读，不会改动任何文件。
