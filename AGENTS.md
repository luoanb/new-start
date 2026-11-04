# Agent Bootstrap

本文件是跨 IDE / 跨 Agent 的最小入口，只负责把不识别 Cursor 规则目录的工具引导到本仓库的真实项目规则。

在本仓库执行实质性编码、文档、协作、审查或调试任务前，先阅读并遵守：

1. `.cursor/rules/project.mdc`
2. `.cursor/rules/` 下与当前任务匹配的其他规则
3. `.cursor/skills/` 下与当前任务匹配的 `SKILL.md`
4. 当前任务涉及的 `docs/`、spec、设计文档或技术方案

如果本文件与 `.cursor/rules/project.mdc` 冲突，以 `.cursor/rules/project.mdc` 为准。

<!-- new-paradigm:begin -->

本项目使用人机协同编程机制。Agent 开始任务前必须阅读 [.new-paradigm/AGENTS.md](.new-paradigm/AGENTS.md)。

<!-- new-paradigm:end -->
