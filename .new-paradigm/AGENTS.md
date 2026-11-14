# Agent 项目工作指引

本文件告诉 Agent 在本项目中如何开始工作，以及不同任务需要读取哪些规则、使用哪些 Skill。

具体约束以 `governance/` 中状态为“生效”的规则为准。本文件只提供工作指引，不重复定义规则。

## 开始工作

1. 判断当前任务属于产品定义、需求探索、需求立约、代码契约、代码交付还是治理规则维护。
2. 阅读 [目录结构规范](governance/rules/目录结构规范.md)，确定相关资产的位置。
3. 阅读 [文档治理规则](governance/rules/文档治理规则.md)，确认文档状态、效力和冲突处理方式。
4. 按下方任务指引读取适用规则和 Skill，不必读取与当前任务无关的全部文件。
5. 只依据状态为“生效”的规则工作；草案、待确认和废弃文档不参与当前裁决。

## 按任务选择指引

### 编写或修改治理规则

- 阅读 [治理规则编写规则](governance/rules/治理规则编写规则.md)。

### 探索模糊或项目级诉求

- 适用于尚不能表述为一次可识别功能变化的诉求。
- 阅读 [需求探索规则](governance/rules/需求探索规则.md)。
- 使用 [需求探索 Skill](governance/skills/requirement-exploration/SKILL.md)。
- 没有生效愿景时先立愿景；探索结论获人确认后停止，是否启动需求立约由人另行决定。

### 编写或修改产品定义

- 阅读 [产品定义规则](governance/rules/产品定义规则.md)。
- 使用 [产品定义模板](governance/templates/产品定义模板.md)。
- 产品定义由 `functional/product/` 下的 `产品边界.md`、`产品形态.md`、`能力构成.md` 三份正文组成，是愿景与需求之间的可裁决语义层；三份各自确认。

### 梳理或修改产品需求

- 阅读 [需求契约追溯规则](governance/rules/需求契约追溯规则.md)。
- 使用 [需求立约 Skill](governance/skills/requirement-authoring/SKILL.md)。

### 编写或修改代码契约

- 阅读 [需求契约追溯规则](governance/rules/需求契约追溯规则.md)。
- 阅读 [代码契约规则](governance/rules/代码契约规则.md)。
- 能力契约使用 [代码契约模板](governance/templates/代码契约模板.md)。
- 跨能力关系使用 [协作契约模板](governance/templates/协作契约模板.md)。
- 代码契约变更后，在 `.new-paradigm/governance/tools/contract-overview/` 执行 `node src/cli.js --project <项目根目录>`，更新 `functional/contracts/overview.md`。
- 总览只用于导航。它不参与裁决，也不能代替契约正文。

### 开发、修复或验证代码

- 阅读 [代码交付规则](governance/rules/代码交付规则.md)。
- 阅读 [指标治理规则](governance/rules/指标治理规则.md)。
- 使用 [代码交付 Skill](governance/skills/code-delivery/SKILL.md)。
- 交付前在 `.new-paradigm/governance/tools/contract-overview/` 执行 `node src/cli.js --project <项目根目录> --check`。总览过期时先重新生成，再继续核对契约正文。

## 篇幅约束

Agent 面向人的输出须保持精简，便于人快速阅读和审阅：

- 与人对话的回复控制在 1000 字以内，章节不超过 5 个。
- 交由人审阅的文档控制在 5000 字以内，章节不超过 7 个。
- 内容超出限制时，优先压缩或拆分，不得突破限制；拆分后的每份文档仍须各自满足上述约束。

## 人工操作说明

面向人的使用教程见 [README.md](README.md)。
