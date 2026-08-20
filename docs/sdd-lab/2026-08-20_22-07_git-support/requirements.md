# Requirements / 需求文档: Git Support

## Restated Understanding / 需求复述

- 我理解当前需求是：为 pulsar-app（Tauri + Rust + Svelte 的 agent 应用）增加**完整的原生 Git 支持**，分两层——AI 可调用的原生 git 工具，以及面向用户的 git UI 面板，一期同时交付。
- 当前核心目标是：让 AI 能查看仓库状态 / diff / log / 分支并执行提交与撤销等写操作；让用户能在文件视图中看到 git 状态、查看行内 diff、解决 merge conflict、执行暂存与提交、管理 stash 与多仓库；所有 git 操作限定在 active workspace 边界内，写操作分级护栏 + 用户确认。
- 当前边界是：git 支持以「active workspace 内发现的 git 仓库」为操作对象；工具复用现有 `ToolRegistry` / `inserts/<name>.md` 门禁 / 工作区越界护栏；UI 挂靠现有文件管理视图。
- 暂不处理：submodule 的识别与操作、应用内置凭据存储与远端账户管理（push/pull 认证复用系统 git 凭据）、git 服务端能力（托管/权限/hook 管理）、二进制文件内容 diff 预览。

## Background / 背景

- 文件域已具备完整 AI 原生文件工具（list/read/write/search_replace/delete/glob/grep/info/create_dir/rename），带工作区护栏与已读清单；文档（`workspace-file-management/requirements.md`）明确将「Git 集成」列为暂不处理，本次迭代将其移入范围。
- 目前 git 能力仅能通过配置驱动的动态命令工具（如 `git status --porcelain` 模板）经 `cmd_exec` 护栏间接使用：无结构化结果、无写操作控制、无 UI。AI 无法可靠地感知仓库状态并执行受控提交。

## Scope / 范围

- In:
  - AI 原生 git 工具（经 `ToolRegistry` 注册，沿用 insert 门禁与工作区护栏）：
    - 只读：`git_status` / `git_diff` / `git_log` / `git_branch` / `git_show` / `git_blame` / `git_stash_list`
    - 写操作：`git_add` / `git_restore` / `git_commit` / `git_reset` / `git_checkout`（含 checkout 文件与切换分支）/ `git_push` / `git_pull` / `git_stash`
  - 工具分级治理：
    - 只读工具默认随对话可用（Core 标签）
    - 局部写（add/restore/commit/stash/pull/push 等）走 insert 门禁，commit 前展示 staged diff 摘要并要求用户确认；push/pull 为常规写操作，不默认禁用，但执行前仍需用户确认
    - 高危写（reset/clean/checkout 丢弃改动）默认由独立配置开关关闭，开启后仍要求二次确认
  - UI git 面板（挂靠文件管理视图，一期）：
    - 文件/目录 git 状态徽标（M / A / D / ?? 等）与变更汇总
    - 当前分支展示与分支切换
    - staging 勾选 + commit 弹窗（提交前展示 staged diff 摘要，用户确认）
    - **行内 diff 视图**：hunk 渲染、行号、前后文、staged/unstaged 切换与导航
    - **merge conflict 解决 UI**：冲突文件列表、逐 hunk 选择 ours / theirs / both、解决后重新标记
    - **git blame 视图**：行级提交归属与信息展示
    - **stash UI**：stash 列表、创建 / 应用 / 删除
    - **LFS 支持**：仓库内 LFS 文件的识别与展示（依赖系统 git-lfs，未安装时降级为指针展示）
    - **多仓库管理 UI**：同一 workspace 内多个独立 git 仓库的发现、切换与当前操作仓库指示；repo 发现仅向 workspace 内扫描（自 workspace 根向内搜索 .git），禁止向上/向外越界查找
    - 危险操作（reset/clean/checkout 丢弃改动）确认弹窗
  - 凭据能力：push/pull 等需认证操作复用系统 git 凭据（credential helper / SSH agent），应用不存储凭据
  - 远程模式（rpc）下 git 工具与文件工具同通道可用
- Out:
  - 应用内置凭据存储、远端账户/Token 管理 UI（明确复用系统 git 凭据，应用不管理凭据）
  - git 服务端能力：仓库托管、权限管理、hook 管理、远端浏览
  - submodule 的识别与操作（本次迭代不处理）
  - 二进制文件内容 diff 预览（LFS 指针 vs 大文件内容对比不做渲染）

## User Interaction / 用户交互

- 触发入口：
  - AI 会话：模型调用 `git_*` 工具（需工具名称在会话工具集中）
  - UI：文件视图的 git 面板；文件树上的状态徽标；变更文件点击打开 diff
- 用户操作路径：
  - 只读：打开 git 面板查看分支、变更汇总；AI 侧自动在需要时调用只读工具
  - 提交：在面板勾选要暂存的文件 → 填写 commit message → 预览 staged diff 摘要 → 确认提交
  - 行内 diff：点击变更文件 → diff 视图（行号/高亮/hunk 导航，staged/unstaged 切换）
  - merge conflict：冲突文件列表 → 逐 hunk 选择 ours/theirs/both → 完成后重新标记 resolved
  - blame：在 diff/文件视图中开启 blame → 行级提交归属展示
  - stash：面板创建 stash（含 message）→ 列表查看 → 应用/删除
  - 多仓库：面板顶部仓库切换器，当前操作仓库高亮
  - 高危写：触发 reset/clean/checkout 丢弃改动 → 弹窗展示影响（reset 前 dry-run 列出将丢失的改动）→ 二次确认；push/pull 为常规写操作，执行前同样经确认弹窗
  - 确认交互：本需求确认弹窗仅覆盖 GUI；Rust 侧写操作确认接口按稳定接口设计，为将来 TUI 接入预留（Q4）
- 系统反馈：
  - 只读工具返回结构化 JSON（解析后的 status/diff/log/blame）
  - 写操作返回执行结果（exit code / stdout / stderr 摘要）
  - 提交/暂存/分支切换后文件徽标与状态即时刷新
- 状态变化：
  - 工具执行成功 / 失败均以错误信息返回；commit 等待用户确认期间不落盘
- 异常/边界交互：
  - 非 git 仓库或无 active workspace：明确报错，不静默失败
  - workspace 含多仓库：UI 需指示当前操作仓库；repo 发现规则见 Q2
  - LFS 文件未安装 git-lfs：diff 显示指针而非内容，并给出提示
  - 输出超限：diff/log 结果截断并提示（复用现有截断约定）
- 不应发生的交互：
  - git 命令越出 workspace 边界
  - 危险写操作在开关关闭时仍可执行
  - 写操作不经用户确认直接生效（commit/push/reset）

## Acceptance Criteria / 验收标准

- [ ] AI 可在 active workspace 内通过工具查看 git status / diff / log / branch / blame，返回结构化 JSON
- [ ] AI 可通过工具执行 add / commit；commit 前模型输出必须经用户确认（UI 弹窗），确认前不产生提交
- [ ] reset / clean / checkout 丢弃改动等高危写操作默认开关为关；开启后仍需二次确认；push/pull 为常规写操作，不默认禁用但执行前经用户确认
- [ ] 所有 git 命令执行根限定在 workspace 内的 repo 根，越界路径一律拒绝
- [ ] 文件视图显示 git 状态徽标，暂存/提交/分支切换后即时刷新
- [ ] UI 提供 commit 面板：勾选暂存 → 写 message → 展示 staged diff 摘要 → 确认提交
- [ ] 行内 diff 视图可渲染 hunk、行号、高亮与前后文，支持 staged/unstaged 切换与 hunk 导航
- [ ] merge conflict 解决 UI 可列出冲突文件、逐 hunk 选择 ours/theirs/both，并更新状态标记
- [ ] blame 视图展示行级提交归属与信息
- [ ] stash UI 支持创建（带 message）、列表查看、应用与删除
- [ ] workspace 内多个 git 仓库可被向内扫描发现与切换，操作对象明确且不串库；禁止向外越界查找
- [ ] LFS 文件可被识别；LFS 未安装时 diff 显示指针并提示；submodule 不在本次范围
- [ ] push/pull 等认证操作复用系统 git 凭据完成认证，应用不存储凭据
- [ ] 远程模式下 git 工具可经 rpc 转发执行，行为与本地一致
- [ ] 非 git 仓库 / 无 active workspace 时报错清晰且不崩溃

## Constraints / 约束

- 业务约束：
  - 操作对象限定 active workspace；git 是用户的本地工具，行为应与用户命令行 `git` 一致
  - 写操作必须可被用户打断/拒绝；高危操作默认关闭
- 技术约束：
  - 复用现有 `ToolRegistry` / `inserts/<name>.md` 门禁 / 工作区越界护栏 / `cmd_exec` 并发超时截断基建
  - 命令执行不得经过 shell 拼接（防注入），执行根固定
  - 前端 UI 复用现有文件管理视图与 api 通道（tauri command / rpc）
  - 写操作确认等 Rust 侧接口按稳定接口设计（GUI 为当前唯一消费方，接口需兼容未来 TUI 接入）
- 时间/兼容性约束：
  - 本机需已安装 git（缺失时给出可读错误）；LFS 能力依赖系统 git-lfs（缺失时降级为指针展示）
  - 不引入对远端 git server 的依赖；认证完全交给系统 git 凭据

## Open Questions / 开放问题

- [x] Q1 diff 视图深度（已关闭）：UI 一期包含行内 diff 视图，hunk 渲染与导航（用户确认）
- [x] Q2 repo 边界与多仓库发现规则（已关闭）：repo 发现仅向 workspace 内扫描（向内搜索 .git），禁止向上/向外越界查找；支持 workspace 内多个独立 git 仓库，UI 提供仓库切换器（用户确认）
- [x] Q3 危险写分级（已关闭）：push 不归入「高危默认关」；高危默认关 = reset / clean / checkout 丢弃改动；push/pull 为常规写操作，不默认禁用但执行前经用户确认（用户确认）
- [x] Q4 写操作确认交互（已关闭）：本需求确认弹窗仅覆盖 GUI；Rust 侧写操作确认接口按稳定接口设计，兼容未来 TUI 接入（用户确认）
- [x] Q5 工具粒度（已关闭）：按行业设计走——`git_diff` 为独立工具（支持 `--cached` 区分 staged/unstaged），UI 展示与 commit 确认均基于其输出（用户确认）
- [x] Q6 submodule / LFS 深度（已关闭）：submodule 本次不处理；LFS 保留识别与展示，依赖系统 git-lfs，未安装时降级为指针展示（用户确认）

## Requirement Decisions / 需求决策

- 2026-08-20 22:07:
  - 决策：范围 = AI 原生 git 工具 + UI git 面板，一期同时交付；写操作全量开放（含 push/reset），但分级护栏 + 独立开关 + 二次确认
  - 原因：agent 应用需打通「改代码 → 看状态 → 提交」闭环；全量写操作是用户明确要求，用分级治理而非一刀切禁用换取能力与安全的平衡
  - 决策：技术路线讨论倾向 spawn git CLI（行为与用户一致、零依赖、输出天然可喂 LLM），正式选型与技术细节留待技术方案阶段固化
  - 原因：本机 git 为唯一行为真相；CLI 输出（porcelain / unified diff）无需二次结构化即符合 LLM 消费需求
- 2026-08-20 22:12:
  - 决策：原「暂不处理」项全部纳入一期范围——行内 diff 视图（hunk 渲染与导航）、merge conflict 解决 UI、blame / stash / submodule / LFS 的 UI、凭据能力（复用系统凭据）、多仓库管理 UI
  - 原因：用户明确要求完整 Git 能力一次到位；UI 从「仅状态与提交」扩展为完整 Git 工作台
  - 决策：凭据能力以「复用系统 git 凭据（credential helper / SSH agent）且应用不存储凭据」为边界，仍不做应用内置凭据/远端账户管理 UI
  - 原因：认证交给系统 git 是 CLI 路线的自然行为，避免引入凭据存储安全面
- 2026-08-20 22:20:
  - 决策：open questions 全部关闭——Q2 repo 发现仅向 workspace 内扫描，禁止外查，支持多仓库；Q3 push 不归高危默认关（高危 = reset/clean/checkout 丢弃改动）；Q4 确认仅 GUI，Rust 侧确认接口按稳定接口设计预留 TUI；Q5 `git_diff` 独立工具按行业设计；Q6 submodule 本次不处理、LFS 保留识别与展示
  - 原因：用户逐一确认；需求边界收敛为「完整 Git 工作台（GUI）+ AI 原生工具，submodule 与二进制 diff 除外」
  - 决策：Rust 侧接口（git backend / 工具 / 写操作确认）按稳定接口设计，GUI 为当前唯一消费方但须兼容未来 TUI 接入
  - 原因：用户明确要求 Rust 侧代码稳定接口设计，方便以后扩展
