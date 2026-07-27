# Agent Bootstrap

本文件是跨 IDE / 跨 Agent 的最小入口，只负责把不识别 Cursor 规则目录的工具引导到本仓库的真实项目规则。

在本仓库执行实质性编码、文档、协作、审查或调试任务前，先阅读并遵守：

1. `.cursor/rules/project.mdc`
2. `.cursor/rules/` 下与当前任务匹配的其他规则
3. `.cursor/skills/` 下与当前任务匹配的 `SKILL.md`
4. 当前任务涉及的 `docs/`、spec、设计文档或技术方案

如果本文件与 `.cursor/rules/project.mdc` 冲突，以 `.cursor/rules/project.mdc` 为准。

## Project Skills

当任务命中某项能力时，先读取对应 `SKILL.md`，再进入计划或执行：

- 日常 checkpoint-driven coding / 文档协作：`.cursor/skills/sdd-riper-one-light/SKILL.md`
- SDD 需求迭代与 `docs/sdd-lab` 管理：`.cursor/skills/sdd-lab/SKILL.md`
- 技术方案落盘与最小实现桥接：`.cursor/skills/sdd-exec-scheme/SKILL.md`
- 轻量 PRD / 交互需求文档：`.cursor/skills/prd-generator-light/SKILL.md`
- 前端界面设计、审美、可用性与视觉优化：`.cursor/skills/impeccable/SKILL.md`

## Baseline Rules

- 默认使用简体中文回复与编写面向用户的说明。
- `No Spec, No Code`：没有文档或最小 spec，不进入代码实现。
- `No Plan Approved, No Execute`：计划未获用户确认，不进入开发执行。
- `Spec is Truth`：文档和代码冲突时，优先认为代码需要修正。
- `Reverse Sync`：发现 Bug 或实现偏差时，先同步文档，再修代码。
- 不擅自覆盖用户改动，不执行破坏性 Git 操作，不提交密钥或本地私密配置。
