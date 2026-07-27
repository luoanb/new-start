# Copilot Instructions

本文件是 GitHub Copilot 的仓库级入口适配。

在生成代码、解释代码、Review 或辅助修改前，先参考：

1. `AGENTS.md`
2. `.cursor/rules/project.mdc`
3. `.cursor/rules/` 下与当前任务匹配的其他规则
4. `.cursor/skills/` 下与当前任务匹配的 `SKILL.md`

不要在本文件中复制完整项目规范。若规则冲突，以 `.cursor/rules/project.mdc` 为准。

Copilot 应遵守：

- 默认使用简体中文说明。
- 优先按现有项目结构、命名和依赖方向补全代码。
- 不主动引入新抽象、新依赖或大范围重构。
- 修改实现前，先确认已有 spec、设计文档或接口契约。
