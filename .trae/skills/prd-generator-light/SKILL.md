---
name: prd-generator-light
description: >-
  生成「仅功能与交互」的轻量 PRD（Light）：不写背景、目标、用户场景、里程碑。
  在用户说「Light PRD」「交互需求文档」「只要功能说明」「prd-light」「快捷需求」时使用。
  产出 docs/prd/prd-<slug>-light.md。
---

# PRD Light — 交互级需求文档

## 何时用本技能

- 需要**可评审、可拆 Story** 的 Markdown，但**不需要**商业叙事与指标长文。
- 与完整 **`prd-generator`** 的区别：本技能**只**写入口、步骤、状态、验收；禁止单独成章的背景 / 目标 / 用户故事 / 里程碑 / 依赖。

## 工作原则

1. **缺省不编造**：未定交互写入文末 **「待定」**，不假装已决策。
2. **一轮澄清封顶**：仅在缺关键交互点时提问，**≤5 问**。
3. **路径**：`docs/prd/prd-<kebab-slug>-light.md`；若已存在则**增量修订**并更新 **修订记录**。
4. **与工程 spec 分工**：`docs/specs/` 里的契约用属性表 **链接**，正文 **不重复** schema/API。
5. **可定位**：提到按钮/面板位置时，写明仓库内 **组件路径**（便于研发对齐）。

## 禁止写入独立章节的内容

- 背景与问题、目标与成功指标、用户与场景（角色/故事/旅程长表）、里程碑与依赖、展开版非功能需求章节。

（与**交互直接相关**的 loading、禁用态、错误 Toast、焦点等可写在对应功能小节内。）

## 文档结构（写入文件时）

```markdown
# <功能名> — PRD（Light）

| 属性 | 内容 |
|------|------|
| 状态 | 草稿 |
| 完整版 PRD | （可选链接 docs/prd/prd-<slug>.md） |
| 技术契约 | （可选链接 docs/specs/...） |

## 范围（极简）
<!-- 包含 / 不包含，若干 bullet -->

## 功能与交互
<!-- 分段：入口 → 容器页/Dialog → 列表 → 行操作 … -->
<!-- 每段：表格「项目 | 说明」+ ### 验收 checklist -->

## 数据与展示约定
<!-- 仅用户可见字段、排序；schema 见 spec -->

## 待定

## 修订记录
```

## 用户可复制的一句话指令

将 `<主题>` 换成你的功能名即可发起任务：

```text
@prd-generator-light 主题：<一句话>；补充：<交互要点或「无」>
```

## Agent 自检（写入前）

- [ ] 已去掉背景、目标、用户场景、里程碑等章节？
- [ ] 每条核心交互都有可勾选验收项？
- [ ] In/Out 是否写清，避免 scope creep？
- [ ] 待定是否集中、可评审拍板？

## 与完整 PRD 并存

- 同一主题可先迭代 **`prd-*-light.md`**，再按需补 **`prd-*.md`**（完整 `prd-generator`）。
- Light 文首可链完整版；不必双向重复大段文字。

## 延伸阅读（非必读）

- 与完整版对比表、附录语气示例：[`../prd-generator/references/requirements-light-generator.md`](../prd-generator/references/requirements-light-generator.md)
