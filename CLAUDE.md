# Claude Instructions

本文件是 Claude / Claude Code 的入口适配。

在本仓库工作前，先阅读并遵守：

1. `AGENTS.md`
2. `.cursor/rules/project.mdc`
3. `.cursor/rules/` 下与当前任务匹配的其他规则
4. `.cursor/skills/` 下与当前任务匹配的 `SKILL.md`

不要在本文件中复制完整项目规范。若规则冲突，以 `.cursor/rules/project.mdc` 为准。

Claude 执行任务时应特别注意：

- 先复述用户意图与任务边界，再进入计划或执行。
- 未获得用户确认前，不进行代码实现或高影响改动。
- 修改代码后，按任务风险运行必要验证，并说明验证结果。
