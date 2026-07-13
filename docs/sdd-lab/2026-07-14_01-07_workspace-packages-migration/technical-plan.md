# Technical Plan / 技术方案: workspace packages migration

## Requirement Baseline / 需求基线

- 对应需求文档：`docs/sdd-lab/2026-07-14_01-07_workspace-packages-migration/requirements.md`
- 需求确认状态：核心迁移方向已确认，等待用户批准执行。
- 本方案覆盖范围：将当前根目录单包工程迁移为 workspace 根目录 + `packages/mini-claw` 单包结构，并保持现有开发命令的可用性与语义稳定。

## Current Project Facts / 当前项目事实

- 已读取文件/模块：
  - 根目录 `package.json`、`tsconfig.json`、`pnpm-workspace.yaml`、`README.md`
  - 运行入口 `src/index.ts`
  - 主编排模块 `src/gateway.ts`
  - 类型定义 `src/types.ts`
- 当前实现事实：
  - 仓库根目录当前直接承载应用源码，`package.json` 中脚本直接指向 `src/index.ts`。
  - `tsconfig.json` 的 `rootDir` 指向根目录 `src/`，构建输出为根目录 `dist/`。
  - `pnpm-workspace.yaml` 已存在，但尚未体现多包结构。
  - README 仍以“根目录单包工程”描述项目结构。
- 相关接口/数据结构：
  - CLI 入口从 `src/index.ts` 创建 `Gateway` 实例并加载环境变量。
  - `Gateway` 组合 `Agent`、`Memory` 和 `SkillManager`，当前这些能力仍处于单包内部模块级关系。
  - `GatewayConfig`、`AgentConfig`、`LLMProvider` 等类型目前由单一 `src/types.ts` 对外提供。
- 约束与风险：
  - `dev:watch` 需要继续忽略 `.mini-claw`，否则会重现 watch 循环问题。
  - 目录迁移后若根目录脚本、包内脚本和构建输出路径未统一，容易出现运行入口失效。
  - 当前项目尚小，不适合本轮同步拆成多个独立包，否则会扩大 import、发布边界和配置改造面。

## Open Questions / 开放问题

- [ ] 当前无必须由用户先确认的阻塞问题。
  - 触发来源：需求与代码现状已足够支撑本轮方案拟定。
  - 无法确定的内容：无。
  - 影响范围：无。
  - 候选处理：按最小 workspace 迁移执行。
  - 用户回答/确认：已确认“单包迁入”。
  - 状态：已关闭

## Solution Options / 方案候选

### Option A / 方案 A

- 推荐：是
- 方案摘要：将当前工程整体迁移到 `packages/mini-claw`，根目录保留 workspace 清单、统一脚本和文档入口。
- 涉及模块：
  - 根目录 `package.json`
  - 根目录 `README.md`
  - 根目录 `pnpm-workspace.yaml`
  - 根目录 `tsconfig` 体系
  - `packages/mini-claw/package.json`
  - `packages/mini-claw/src/**`
- 优点：
  - 改动最小，当前源码几乎可原样迁移。
  - 风险集中在工程配置与路径调整，便于验证。
  - 为后续继续拆成 `core + cli` 或更多包保留演进空间。
- 缺点：
  - 当前包内仍是“库逻辑 + CLI 入口”混合形态，边界不会在本轮被彻底理顺。
  - 根目录与子包之间需要维护一层脚本转发。
- 风险：
  - 若脚本转发、构建输出或路径引用处理不当，可能导致用户命令习惯中断。

### Option B / 方案 B

- 推荐：否
- 方案摘要：本轮直接拆成 `packages/core` 和 `packages/cli`，把框架逻辑与 CLI 入口彻底分离。
- 涉及模块：
  - 当前全部源码文件
  - workspace 根脚本
  - 包间依赖与构建配置
- 优点：
  - 架构边界更清晰，后续更适合发布和复用。
  - `core` 与 `cli` 的职责更明确。
- 缺点：
  - 对当前项目体量来说改造面偏大。
  - 会同时引入 import 重组、包间依赖、构建顺序与对外 API 固化等额外复杂度。
- 风险：
  - 在没有先完成一次稳定 workspace 化之前，过早拆包容易放大回归范围。

## Decision / 方案决策

- Selected / 选定方案：Option A，先迁移到 `packages/mini-claw`
- Why / 选择原因：这是当前用户已确认的方向，且最符合“先完成 workspace 化，再决定是否继续拆包”的最小风险路径。
- Decision Owner / 决策人：用户
- Decision Time / 决策时间：2026-07-14 01:07
- Open Questions 状态：全部关闭

## API Design / API 设计

- 变更类型：无
- 消费方：CLI 运行入口与未来工作区维护者
- 真相源文件：本轮无新增独立运行时 API；现有类型与模块边界保持在 `packages/mini-claw` 包内

### Compatibility Notes / 兼容说明

- 与现有 API 的关系：本轮不主动改变运行时模块签名，主要变更目录结构与工程配置。
- 明确不做的能力：不在本轮定义新的外部可发布 SDK API，也不承诺根目录继续作为应用源码包存在。

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：
  - 用户批准按本方案开始迁移。
  - 当前工作区没有与迁移文件路径直接冲突的未处理改动。
- 若执行前需求、API、范围或交互规则变化：
  - 先回写 `requirements.md` 或 `technical-plan.md`，再继续实现。

### Step 1. 建立 workspace 根职责

#### 文件：`package.json`

- 改动类型：修改
- 改动内容：
  - 将根包从“应用源码包”调整为“workspace 根入口”。
  - 保留统一脚本入口，通过 `pnpm --filter` 或等价方式转发到 `packages/mini-claw`。
  - 移除根目录对 `src/index.ts` 的直接绑定。
- 设计约束：
  - API：无运行时 API 变化。
  - 状态：根目录成为 workspace 协调层。
  - 交互：开发者继续可在根目录运行统一命令。
  - 样式：无。
  - 资源：无。
- 验收点：
  - 根目录 `pnpm dev`、`pnpm dev:watch`、`pnpm build`、`pnpm start` 能转发到目标包。

#### 文件：`pnpm-workspace.yaml`

- 改动类型：修改
- 改动内容：
  - 明确纳入 `packages/*`。
- 设计约束：
  - API：无。
  - 状态：workspace 能识别新包目录。
  - 交互：根目录安装依赖时包含子包。
  - 样式：无。
  - 资源：无。
- 验收点：
  - `pnpm install` 后 workspace 正常解析 `packages/mini-claw`。

### Step 2. 迁移当前应用到 `packages/mini-claw`

#### 文件：`packages/mini-claw/package.json`

- 改动类型：新增
- 改动内容：
  - 承接当前根目录应用包元数据、依赖和脚本。
  - 保留 `tsx`、`typescript`、`openai`、`chalk`、`dotenv` 等现有依赖配置。
- 设计约束：
  - API：不变。
  - 状态：包内可以独立开发和构建。
  - 交互：包内脚本语义与迁移前保持一致。
  - 样式：无。
  - 资源：无。
- 验收点：
  - 在包目录中可以独立运行 `pnpm dev`、`pnpm build`。

#### 文件：`packages/mini-claw/src/index.ts`

- 改动类型：新增（由现有文件迁移）
- 改动内容：
  - 迁移现有 CLI 入口与模块文件。
  - 保持 `.mini-claw/memory` 默认相对路径语义可用，必要时验证其相对包目录与根目录执行方式的行为一致性。
- 设计约束：
  - API：遵循现有内部模块契约。
  - 状态：CLI 启动逻辑不变。
  - 交互：命令行交互与现有帮助命令保持一致。
  - 样式：无。
  - 资源：保留 `.env` 读取方式。
- 验收点：
  - CLI 能正常启动并创建 `Gateway`。
  - 会话记忆目录行为符合预期。

#### 文件：`packages/mini-claw/src/*.ts`

- 改动类型：新增（由现有文件迁移）
- 改动内容：
  - 将 `agent.ts`、`gateway.ts`、`memory.ts`、`skills.ts`、`types.ts` 等迁入包内。
  - 修正因目录层级变化产生的构建与路径配置差异。
- 设计约束：
  - API：维持现有模块签名。
  - 状态：内部模块关系不重构。
  - 交互：无直接用户交互变化。
  - 样式：无。
  - 资源：无。
- 验收点：
  - TypeScript 编译通过。
  - 运行期模块导入不报错。

#### 文件：`packages/mini-claw/tsconfig.json`

- 改动类型：新增
- 改动内容：
  - 将当前编译配置下沉到包内，`rootDir` 指向包内 `src/`，`outDir` 指向包内 `dist/`。
- 设计约束：
  - API：无。
  - 状态：包内独立编译。
  - 交互：开发者可直接在包目录调试。
  - 样式：无。
  - 资源：无。
- 验收点：
  - `pnpm --filter mini-claw build` 或等效命令输出包内 `dist/`。

### Step 3. 清理根目录源码直连并更新文档

#### 文件：`README.md`

- 改动类型：修改
- 改动内容：
  - 将项目结构说明改为 workspace 形态。
  - 明确根目录与 `packages/mini-claw` 的职责分工。
  - 更新运行命令说明，避免根目录与包目录描述矛盾。
- 设计约束：
  - API：无。
  - 状态：README 成为迁移后的结构真相源。
  - 交互：开发者首次进入仓库即可理解新结构。
  - 样式：无。
  - 资源：无。
- 验收点：
  - README 中不存在“源码仍位于根目录 `src/`”的过时表述。

#### 文件：根目录 `src/`、根目录 `tsconfig.json`

- 改动类型：修改 / 删除 / 替换
- 改动内容：
  - 移除根目录对旧源码布局的依赖。
  - 视实现方式决定根目录是否保留共享 `tsconfig.base.json` 或直接删除旧 `tsconfig.json`。
- 设计约束：
  - API：无。
  - 状态：避免根目录残留旧入口引发歧义。
  - 交互：开发者不会误以为根目录 `src/` 仍是实际源码位置。
  - 样式：无。
  - 资源：无。
- 验收点：
  - 仓库结构中没有与新布局冲突的陈旧入口。

### Step 4. 检查与回写

#### 命令

- 运行：
  - `pnpm install`
  - `pnpm dev`
  - `pnpm build`
  - 必要时在包目录运行 `pnpm dev:watch`
- 修复：
  - 若 watch 行为异常，优先检查 `.mini-claw` 忽略配置是否在新脚本位置仍然生效。

#### 文件：`docs/sdd-lab/2026-07-14_01-07_workspace-packages-migration/lifecycle.md`

- 回写执行记录：
  - 记录实际迁移的文件与脚本变更。
- 记录实际改动摘要：
  - 记录最终目录结构、脚本入口和验证结果。
- 记录验证结果：
  - 记录 `dev`、`build`、必要的 watch 验证是否通过。
- 记录下一步状态：
  - 若迁移完成，则进入 `done`；若发现仍需进一步拆包，则回写为下一轮新需求。

## Risk And Mitigation / 风险与缓解

- 风险：根目录脚本改造后，开发者入口发生混淆。
  - 缓解方式：在根 `package.json` 和 README 中显式声明“根目录只做 workspace 转发”。
- 风险：迁移后 `.mini-claw` 存储路径相对位置发生变化。
  - 缓解方式：执行阶段验证根目录启动与包目录启动两种路径行为，并在必要时改成基于工作目录更稳定的策略。
- 风险：遗留旧 `src/` 或旧 `dist/` 造成误判。
  - 缓解方式：迁移完成后清理陈旧目录，并以文档说明最终真相源。

## Execute Checkpoint / 执行检查点

- 当前理解：本轮先把当前应用整体迁移到 `packages/mini-claw`，不做更细粒度拆包。
- 核心目标：完成一次低风险 workspace 化迁移，同时保持开发命令和 CLI 行为稳定。
- 下一步动作：等待用户批准后，按本方案开始实际文件迁移、脚本调整和验证。
- 风险：主要在路径、脚本转发和 watch 忽略配置回归。
