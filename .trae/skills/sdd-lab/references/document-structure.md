# SDD Lab Document Structure

## Scope Guard

本文件是 `sdd-lab` skill 的内部 reference，仅在当前任务已激活 `sdd-lab`，且正在创建、查找或维护 `docs/sdd-lab` 迭代文档时生效。其他 skill、普通代码任务、非 `docs/sdd-lab` 文档任务或后续无关上下文中，不得把本文件作为约束、流程或输出格式依据。

## 根目录

`sdd-lab` 的文档根目录固定为：

```text
docs/sdd-lab/
```

每个需求迭代必须使用独立文件夹。需求文档生成阶段只创建 `lifecycle.md` 和 `requirements.md`：

```text
docs/sdd-lab/YYYY-MM-DD_hh-mm_<iteration-name>/
├── lifecycle.md
└── requirements.md
```

若需求涉及 Figma、视觉稿、页面还原、Icon 导出或设计稿文档化，在需求确认后、技术方案生成前创建或维护 `visual-design.md`：

```text
docs/sdd-lab/YYYY-MM-DD_hh-mm_<iteration-name>/
├── lifecycle.md
├── requirements.md
└── visual-design.md
```

进入技术方案生成阶段后，再创建 `technical-plan.md`。若当前需求不涉及视觉稿：

```text
docs/sdd-lab/YYYY-MM-DD_hh-mm_<iteration-name>/
├── lifecycle.md
├── requirements.md
└── technical-plan.md
```

若当前需求涉及视觉稿：

```text
docs/sdd-lab/YYYY-MM-DD_hh-mm_<iteration-name>/
├── lifecycle.md
├── requirements.md
├── visual-design.md
└── technical-plan.md
```

## 命名规则

- 时间前缀：`YYYY-MM-DD_hh-mm_`
- `<iteration-name>` 使用简短英文、拼音或稳定短语。
- 文件夹名称聚焦需求目标，不写长句。
- 同一文件夹只承载一个主要需求迭代。

## 文件职责

- `lifecycle.md`：记录状态、当前摘要、执行记录和恢复锚点。
- `requirements.md`：记录需求目标、背景、范围、非目标、验收标准、开放问题；若用户提供了 Figma / 视觉稿链接，用可选章节 `Referenced Designs / 引用设计稿` 集中追溯，不把长链接列表内嵌进 Scope。
- `visual-design.md`：记录 Figma 或视觉设计稿来源、页面设计事实、Icon 导出与 SVG 组件化要求；仅在需求涉及视觉设计稿时创建。其 `来源` 应承接 `requirements.md` 的 `Referenced Designs`。
- `technical-plan.md`：记录基于项目现状的技术方案、涉及模块、接口、步骤和风险；只在技术方案生成阶段创建。若有对外 / 跨模块契约变更，用可选章节 `API Design / API 设计` 固化目标契约（与 `Current Project Facts` 中的现有接口事实分离）。

不要把技术方案写进 `requirements.md` 或 `visual-design.md`；不要把需求讨论堆进 `technical-plan.md`；不要把聊天流水账写进 `lifecycle.md`。不要在需求阶段用“以 Figma 为准”替代具体设计稿链接。

## 视觉设计文档

`visual-design.md` 是可选阶段文件，只在当前迭代需要从 Figma 或视觉稿提取设计事实时创建。它用于沉淀设计稿事实，不用于记录实现方案。

必须包含：

- `来源`：只记录核心追溯信息，例如 Figma 链接、文件名、Frame / Node、版本或更新时间。
- `页面设计`：内部结构由视觉稿内容决定，只记录稿中真实存在且对实现或验收有用的设计事实。
- `Icon / SVG 组件导出`：需要导出的 Icon 清单、SVG 命名、组件命名、尺寸、颜色策略、目标路径、导出状态。

视觉稿疑问就近记录在相关章节；影响需求或技术决策的问题同步到 `requirements.md` 或 `technical-plan.md`。

若技术方案需要引用视觉稿，必须以 `visual-design.md` 为基准，而不是直接依赖聊天记录或临时截图描述。

## 状态字段

`lifecycle.md` 中必须维护：

```yaml
status: draft | planned | executing | done
result: pending | completed | cancelled | rejected
updated_at: YYYY-MM-DD hh:mm
```

`result` 只在 `done` 或接近收尾时有意义；正常推进时保持 `pending`。

## 虚拟分组判断

`进行中` 和 `已完结` 是展示时的虚拟分组，不写入文件：

- 进行中：`draft`、`planned`、`executing`
- 已完结：`done`

默认列表排序：

1. 进行中优先。
2. 同组内按文件夹时间前缀倒序。
3. 时间前缀缺失时，按 `updated_at` 倒序。
4. 仍无法判断时，放在列表末尾并提示文档需要补齐。

## 状态流转

主路径：

```text
draft -> planned -> executing -> done
```

回退路径：

- `planned -> draft`：技术方案阶段发现需求不清、范围变化或验收标准不成立。
- `executing -> planned`：执行阶段发现技术方案不可行或风险显著变化。

状态变化必须同步写入 `lifecycle.md` 的 `Transition Log`，至少记录：

- 变更前状态
- 变更后状态
- 变更原因
- 依据
- 下一步动作

## 默认列表示例

当用户没有输入具体任务时，输出类似：

```text
进行中
- 2026-05-20_11-30_canvas-revision-sync | planned | 下一步：确认技术方案后等待用户继续指令
- 2026-05-18_16-10_auth-error-copy | draft | 下一步：补齐验收标准

已完结
- 2026-05-10_09-00_runtime-compat | done/completed
```

保持短，不输出完整文档内容。
