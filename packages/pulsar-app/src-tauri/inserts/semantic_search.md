# semantic_search

## 工具

在活动工作区内按语义搜索代码块。对应 IDE 的代码语义搜索能力，与 `grep` 互补：`grep` 按正则命中单行，`semantic_search` 返回完整代码单元（函数 / 结构体 / 类 / impl / trait 等）。

参数：

- `query`（必填）：查询语句，可用自然语言或关键词描述要找的代码。
- `top_k`（可选，默认 10，上限 20）：返回的代码块数量上限。
- `path`（可选）：限定搜索的路径前缀（相对工作区根），如 `src/auth`；省略则搜索整个工作区。

返回 JSON 对象，含 `results` 数组（每项含 `path`、`start_line`、`end_line`、`block_type`、`score`、`content` 摘要）、`indexed_blocks`（索引总块数）与 `index_duration_ms`（本次索引耗时）。

## 对模型的期待

- 需要定位"某个概念/功能实现在哪"但不知道确切标识符时调用，如"哪里处理登录鉴权"。
- 想找确切符号（如特定函数名）时优先用 `grep`；`semantic_search` 适合模糊/语义查询。
- 用 `path` 收窄搜索范围可减少噪音；结果有上限，命中过多会截断。
- 依据返回结果的 `path` + 行范围定位代码，再按需用 `read_file` 读取全文。

## 忌用

- 不要用 `semantic_search` 替代 `glob` 做文件名搜索。
- 首次搜索会构建索引（大仓库可能秒级），不要在同一轮内重复无谓搜索相同 query。
- `query` 过短（如单字符）会被拒绝，需提供有区分度的词。

## 注意

- 只作用于当前 active 工作区；ignore 规则默认排除（如 `node_modules`）。
- 索引按项目独立存储，项目移动后首次搜索自动重建。
- `content` 是截断摘要，需要完整上下文时用 `read_file` 按行范围读取。
