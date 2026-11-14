# Agent Bootstrap

本文件是跨 IDE / 跨 Agent 的最小入口，只负责把不识别 Cursor 规则目录的工具引导到本仓库的真实项目规则。

在本仓库执行实质性编码、文档、协作、审查或调试任务前，先阅读并遵守：

1. `.cursor/rules/project.mdc`
2. `.cursor/rules/` 下与当前任务匹配的其他规则
3. `.cursor/skills/` 下与当前任务匹配的 `SKILL.md`
4. 当前任务涉及的 `docs/`、spec、设计文档或技术方案

如果本文件与 `.cursor/rules/project.mdc` 冲突，以 `.cursor/rules/project.mdc` 为准。

## 新范式调整

新范式的规则、模板、Skill、指标和工具调整，一律先在机制源仓库 `F:\work\new-paradigm\` 修改并验证，再升级到本项目同步，不得直接在本项目 `.new-paradigm/` 就地改动。

- 源仓库：`F:\work\new-paradigm\.new-paradigm\`
- 同步：在 `F:\work\new-paradigm\.new-paradigm\governance\tools\project-lifecycle` 执行
  `node src/cli.js upgrade --target f:\work\new-start`
- 执行前先加 `--dry-run` 预演；退出码 `1` 表示存在本地修改冲突，需人工处理后重跑。

<!-- new-paradigm:begin -->
本项目使用人机协同编程机制。Agent 开始任务前必须阅读 [.new-paradigm/AGENTS.md](.new-paradigm/AGENTS.md)。
<!-- new-paradigm:end -->
