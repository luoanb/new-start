# Requirements / 需求文档: workspace packages migration

## Restated Understanding / 需求复述

- 我理解当前需求是：将当前位于仓库根目录的单包 `Mini-Claw` 工程迁移到 workspace 的 `packages/` 目录下。
- 当前核心目标是：先完成一轮“单包迁入”，把现有项目落到 `packages/mini-claw`，为后续继续拆分多包保留空间。
- 当前边界是：本轮聚焦目录结构、脚本入口、构建配置、文档和运行方式迁移，不同时推进新的业务能力。
- 暂不处理：把当前代码立即拆成 `core`、`cli`、`memory`、`skills` 等多个独立包；不在本轮引入新的发布流程或远程部署流程。

## Scope / 范围

- In:
  - 将现有 `src/`、包级 `package.json`、`tsconfig.json` 等工程文件迁移到 `packages/mini-claw/`。
  - 调整根目录 workspace 配置，使仓库根目录承担 workspace 管理职责而非应用源码职责。
  - 保持 `pnpm dev`、`pnpm dev:watch`、`pnpm build`、`pnpm start` 的开发语义清晰，必要时由根脚本转发到包内脚本。
  - 更新 README 与目录说明，使“根目录是 workspace，应用位于 `packages/mini-claw`”成为文档真相源。
- Out:
  - 不在本轮把运行时模块拆成多个独立 npm 包。
  - 不新增测试框架、lint 框架或 CI 流程。
  - 不改变现有 LLM provider 能力、会话记忆行为和 CLI 交互功能。

## User Interaction / 用户交互

- 触发入口：开发者在仓库根目录执行 `pnpm` 命令，并通过根目录文档理解 workspace 结构。
- 用户操作路径：
  - 开发者在根目录安装依赖。
  - 开发者继续在根目录使用统一脚本启动、构建或运行 `mini-claw`。
  - 开发者如需查看源码或继续扩展，进入 `packages/mini-claw/` 对应目录工作。
- 系统反馈：
  - 根目录脚本能够明确转发到 `packages/mini-claw`。
  - README 与目录结构说明能够显示新的包位置和运行方式。
- 状态变化：
  - 仓库从“根目录单包工程”变为“根目录 workspace + `packages/mini-claw` 子包工程”。
  - 根目录不再把 `src/` 作为自身源码根目录。
- 异常/边界交互：
  - `dev:watch` 仍需避开 `.mini-claw` 存储目录，避免重复重启问题回归。
  - 根目录脚本与包内脚本必须避免相互冲突，保证开发者入口单一且可预期。
- 不应发生的交互：
  - 开发者在根目录执行原有命令后找不到实际包入口。
  - 构建输出路径与运行入口因目录迁移而失效。
  - 迁移后 README 仍把仓库描述为根目录单包项目。

## Acceptance Criteria / 验收标准

- [ ] 根目录存在清晰的 workspace 结构，当前应用代码位于 `packages/mini-claw/`。
- [ ] 根目录继续支持统一的 `pnpm dev`、`pnpm dev:watch`、`pnpm build`、`pnpm start` 入口，且能指向 `packages/mini-claw`。
- [ ] `packages/mini-claw` 内部可以独立完成开发、构建与运行。
- [ ] `dev:watch` 在迁移后仍明确忽略 `.mini-claw`，不引入 watch 死循环回归。
- [ ] README、目录说明和脚本说明已反映 workspace 新结构。
- [ ] 现有 CLI 交互、provider 配置读取和记忆持久化默认行为在迁移后保持不变。

## Constraints / 约束

- 业务约束：
  - 保持 `Mini-Claw` 现有对话式 CLI 用法不变。
  - 保持当前 provider 配置方式和默认存储目录语义不变。
- 技术约束：
  - 使用 `pnpm workspace` 组织结构。
  - 迁移过程中需保留 `tsx` 运行方式，并继续规避 `.mini-claw` 被 watch 导致的重启问题。
  - 当前仓库已有 `packages/` 相关约定，后续新增包内代码需遵守对应规则。
- 时间/兼容性约束：
  - 优先做最小可行迁移，避免一次性大拆包带来的兼容风险。
  - 根目录开发体验应尽量保持连续，避免用户必须记忆新的复杂命令组合。

## Open Questions / 开放问题

- [ ] 当前无阻塞性开放问题；若进入执行时发现根目录是否保留 package 名称或是否需要新增共享 `tsconfig` 的决策，再回写到技术方案。

## Requirement Decisions / 需求决策

- 2026-07-14 01:07:
  - 决策：本轮采用“单包迁入”，目标包名为 `packages/mini-claw`。
  - 原因：改动最小，能先完成 workspace 化，再为后续多包拆分留出空间。
- 2026-07-14 01:07:
  - 决策：当前阶段只产出需求文档和技术方案，不进入代码执行。
  - 原因：遵守 `No Spec, No Code` 与 `No Approval, No Execute` 约束。
