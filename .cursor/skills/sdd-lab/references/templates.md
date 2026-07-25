# SDD Lab Templates / SDD Lab 模板

本文件只在创建或补齐 `docs/sdd-lab/<iteration>/` 文档时读取。需求文档生成阶段只使用 `lifecycle.md` 和 `requirements.md` 模板；需求确认后，若涉及 Figma、视觉稿、页面还原、Icon 导出或设计稿文档化，先使用 `visual-design.md` 模板；完成必要的视觉设计文档后，才进入技术方案生成阶段并使用 `technical-plan.md` 模板。

## `lifecycle.md`

````markdown
# Lifecycle / 生命周期: <Iteration Name>

```yaml
status: draft
result: pending
created_at: YYYY-MM-DD hh:mm
updated_at: YYYY-MM-DD hh:mm
owner: user
```

## Current Summary / 当前摘要

- 批准状态：
- 当前状态：
- 当前核心目标：
- 下一步唯一动作：

## Execution Log / 执行记录

- 1. YYYY-MM-DD hh:mm: 简述本次动作、状态变化或偏差。
````

## `requirements.md`

```markdown
# Requirements / 需求文档: <Iteration Name>

## Restated Understanding / 需求复述

- 我理解当前需求是：
- 当前核心目标是：
- 当前边界是：
- 暂不处理：

## Scope / 范围

- In:
- Out:

## User Interaction / 用户交互

- 触发入口：
- 用户操作路径：
- 系统反馈：
- 状态变化：
- 异常/边界交互：
- 不应发生的交互：

## Acceptance Criteria / 验收标准

- [ ] 验收标准 1：

## Constraints / 约束

- 业务约束：
- 技术约束：
- 时间/兼容性约束：

## Referenced Designs / 引用设计稿

> 可选。仅当用户在需求阶段提供了 Figma / 视觉稿链接时创建本小节；不要创建空占位。需求正文只引用用途，完整链接集中在此；视觉事实展开写入 `visual-design.md`。

| 用途 | Node | 链接 |
| ---- | ---- | ---- |
|      |      |      |

## Open Questions / 开放问题

- [ ] Q1 待确认问题：

## Requirement Decisions / 需求决策

- YYYY-MM-DD hh:mm:
  - 决策：
  - 原因：
```

## `technical-plan.md`

```markdown
# Technical Plan / 技术方案: <Iteration Name>

## Requirement Baseline / 需求基线

- 对应需求文档：
- 需求确认状态：
- 本方案覆盖范围：

## Current Project Facts / 当前项目事实

- 已读取文件/模块：
- 当前实现事实：
- 相关接口/数据结构：
- 约束与风险：

## Open Questions / 开放问题

- [ ] Q1 需要向用户确认的问题：
  - 触发来源：需求 / 当前代码现状 / 方案拟定
  - 无法确定的内容：
  - 影响范围：
  - 候选处理：
  - 用户回答/确认：
  - 状态：待用户确认 / 已关闭

## Solution Options / 方案候选

### Option A / 方案 A

- 推荐：是/否
- 方案摘要：
- 涉及模块：
- 优点：
- 缺点：
- 风险：

### Option B / 方案 B

- 推荐：是/否
- 方案摘要：
- 涉及模块：
- 优点：
- 缺点：
- 风险：

...

## Decision / 方案决策

- Selected / 选定方案：
- Why / 选择原因：
- Decision Owner / 决策人：（等待用户决策）
- Decision Time / 决策时间：
- Open Questions 状态：全部关闭 / 仍有待确认项（说明：）

## API Design / API 设计

> 可选。仅当本次交付会新增或变更对外 / 跨模块契约时填写；纯内部实现改动可省略，并在 Decision 中说明“无独立 API 变更”。

### Contract Scope / 契约范围

- 变更类型：新增 / 扩展 / 破坏性变更 / 无
- 消费方：组件调用方 / 页面 / 其他模块
- 真相源文件：（如 `types.ts`、路由配置、服务接口）

### `<TypeOrContractName>`

- `field: Type`：含义与约束

### Compatibility Notes / 兼容说明

- 与现有 API 的关系：
- 明确不做的能力：

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：
- 若执行前需求、API、范围或交互规则变化：

### Step 1. <交付目标>

#### 文件：`path/to/file`

- 改动类型：新增 / 修改 / 删除 / 重命名
- 改动内容：
- 设计约束：
  - API：
  - 状态：
  - 交互：
  - 样式：
  - 资源：
- 验收点：

#### 文件：`path/to/another-file`

- 改动类型：新增 / 修改 / 删除 / 重命名
- 改动内容：
- 设计约束：
- 验收点：

### Step 2. <交付目标>

#### 文件：`path/to/file`

- 改动类型：
- 改动内容：
- 设计约束：
- 验收点：

### Step N. 检查与回写

#### 命令

- 运行：
- 修复：

#### 文件：`docs/sdd-lab/<iteration>/lifecycle.md`

- 回写执行记录：
- 记录实际改动摘要：
- 记录验证结果：
- 记录下一步状态：

## Risk And Mitigation / 风险与缓解

- 风险：
  - 缓解方式：

## Execute Checkpoint / 执行检查点

- 当前理解：
- 核心目标：
- 下一步动作：
- 风险：
```

## `visual-design.md`

```markdown
# Visual Design / 视觉设计文档: <Iteration Name>

## Source / 来源

- Figma 链接：
- 文件 / 页面：
- Frame / Node：
- 版本 / 更新时间：

## Page Design / 页面设计

## Icon / SVG Component Export / Icon 与 SVG 组件导出

- 导出目标路径：
- 命名规则：
- 颜色策略：固定色 / currentColor / 主题变量
- 尺寸策略：
- 可访问性属性：

| Icon | Figma Node | SVG 文件名 | 组件名 | 尺寸 | 颜色策略 | 状态   |
| ---- | ---------- | ---------- | ------ | ---- | -------- | ------ |
|      |            |            |        |      |          | 待导出 |
```

## 使用要求

- 新建迭代处于需求文档生成阶段时，只创建 `requirements.md` 和必要的 `lifecycle.md`。
- 需求阶段只能完善 `requirements.md` 和 `lifecycle.md`，不要创建 `technical-plan.md` 占位文件。
- 用户在需求阶段提供的 Figma / 视觉稿链接，必须写入 `requirements.md` 的可选章节 `Referenced Designs / 引用设计稿`，不要只写“以 Figma 为准”而丢掉具体链接；也不要把长链接列表内嵌进 Scope 正文。
- `Referenced Designs` 只做追溯清单（用途 / Node / 链接）；不在此展开视觉事实。无设计稿链接时不要创建该空章节。
- 只有需求涉及 Figma、视觉稿、页面还原、Icon 导出或设计稿文档化时，才创建 `visual-design.md`；视觉设计文档生成前置于技术方案生成，不要创建空占位文件。
- `visual-design.md` 的 `Source / 来源` 应承接 `requirements.md` 的 `Referenced Designs`，并在此展开设计事实；不要依赖聊天记录作为设计链接真相源。
- `visual-design.md` 只记录设计事实和导出要求，不记录代码落点、状态管理、接口变更等技术方案。
- `Source / 来源` 只记录核心追溯信息；`Page Design / 页面设计` 内部结构由视觉稿内容决定，不强制套固定字段。
- 视觉稿疑问就近记录在相关章节；影响需求或技术决策的问题同步到 `requirements.md` 或 `technical-plan.md`。
- 设计稿包括 Icon 时，必须在 `Icon / SVG Component Export` 中记录需要导出的 SVG 组件清单、命名、颜色策略、目标路径和状态。
- 技术方案阶段必须先读取项目现状，再创建或更新 `technical-plan.md`。
- 若需求涉及视觉稿，技术方案生成前必须先完成必要的 `visual-design.md`；若存在 `visual-design.md`，技术方案必须引用它作为页面设计和 Icon 导出的设计基线。
- `technical-plan.md` 中使用 `Execution Steps / 执行步骤` 承接组件设计、文件改动和实现映射；步骤按交付顺序组织，步骤内部以文件路径为轴记录改动内容、设计约束和验收点，不重复抄写 `visual-design.md` 的页面设计事实。
- `API Design / API 设计` 是可选章节，放在 `Decision` 之后、`Execution Steps` 之前；仅在新增或变更对外 / 跨模块契约时填写。`Current Project Facts` 只记录现有接口事实，目标契约写在 `API Design`。
- `API Design` 按类型 / 契约单元组织（如 Props、Column、Context、Api），不要强制拆成 Props / Callbacks / Instance 等固定分类；内容确实成块时，可在该迭代内自增 `###` 子节。
- `Execution Steps` 的 `设计约束 / API` 只写“遵循 `API Design` + 本文件特有约束”，不重复抄写完整字段表；执行中签名变化时，先回写 `API Design`，再改代码。
- 技术拟定阶段支持多方案并行记录；小改动可以只保留一个方案，但需要写明跳过多方案对比的原因。
- `technical-plan.md` 的 `Open Questions` 只记录 Agent 在读取需求、设计文档和项目现状后仍无法确定、必须向用户提问确认的内容；不要记录已经明确的问题、事实或结论。
- `Open Questions` 必须使用稳定序号，例如 `Q1`、`Q2`，问题表述要能直接发给用户回答，便于后续引用、回答、关闭和回写。
- `Open Questions` 不只来自需求阶段；整理代码现状、拟定方案、做技术决策时出现无法自行判断的内容，也必须先提问并交由用户回答或确认后关闭。
- 方案决策必须由用户完成；Agent 可以整理事实、列出候选方案、给出推荐和依据，但不能替用户拍板。
- 方案确认前，`Decision / 方案决策` 必须说明用户选定方案、选择原因，以及 `Open Questions` 是否关闭或仍需用户确认。
- 执行阶段发现偏差时，先更新对应文档，再继续实现。
