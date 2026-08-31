# 🦞 Pulsar（星脉）

一个**自主推进的 AI Agent 桌面客户端**：Rust 核心 + Tauri GUI，同时提供 CLI、TUI 与 headless 网络服务三种附加入口，四种对话模式覆盖从普通对话到自主课题推进的完整光谱。

## 特性

- **四种对话模式**
  - **Chat** — 普通对话，一问一答，不调用工具
  - **Agent** — 可调用工具的对话，按需执行 tool-calling 循环
  - **Assistant** — 自主推进模式，Poller 按 tick 调度，Neuron 驱动课题深入
  - **System** — 系统模式，附加 System 标签工具，用于系统管理类会话
- **Neuron 神经元系统** — 知识/行为节点，支持创建、进化、连接与权重调整、邻域选型、容量回收，模型参与自主评分
- **Topic 课题管理** — 课题 CRUD、scope item 验收、状态机（todo / in_progress / paused / done / cancelled）
- **Poller 后台推进** — 未完成课题按 tick 自主推进，并行度可调，会话状态实时跟踪
- **统一 Provider / Model 管理** — 内置 + 自定义服务商，模型注册表，保存即热重载，API key 掩码回显
- **工具系统** — 原生工具 + 动态工具（本机命令 / HTTP）+ MCP Server 统一装配，后台渐进加载不阻塞启动
- **工作区文件管理** — 多工作区、文件树、读/写/改/删/移动、glob / grep / **语义搜索**
- **Git 面板** — 仓库发现、diff、提交历史、分支、blame、stash、暂存/提交/推送/拉取/冲突解决，危险写操作二次确认
- **终端面板** — 跨平台 PTY（Unix pty / Windows ConPTY），支持浏览器端 /ws 访问
- **Hook 裁决记录** — 打分 / 课题匹配 / 验收等裁决全量留痕，可回溯过滤
- **工程体验** — 中英双语 i18n、深浅主题、全局快捷键、运行日志面板、流式消息 + 思考过程展示

## 技术栈

| 层 | 技术 |
|----|------|
| 后端核心 | Rust（Tauri v2、tokio、axum、rusqlite/SQLite、reqwest、mlua、tree-sitter、portable-pty） |
| 前端 | SvelteKit 2 + Svelte 5 + TypeScript + Vite 6 |
| 存储 | `.pulsar/` 目录：`config.json` + `sessions/*.json` + SQLite `app.db` |

## 仓库结构

pnpm workspace monorepo，应用包位于 `packages/pulsar-app`：

```
.
├── packages/
│   └── pulsar-app/
│       ├── src/                      # 前端（SvelteKit）
│       │   ├── routes/               # 单页入口（+page.svelte）
│       │   └── lib/
│       │       ├── api/              # API 客户端：tauriClient（IPC）/ httpClient（HTTP+SSE）
│       │       ├── stores/           # dataStore：监听状态事件按 kind 重拉
│       │       ├── components/       # ChatArea / NeuronNetworkGraph / GitPanel / ToolPanel ...
│       │       ├── layout/           # 面板布局系统（views / resizable）
│       │       ├── features/neuron/  # 网络图布局、系统类型配色
│       │       ├── i18n/ hotkey/     # 国际化 / 全局快捷键
│       │       └── ...
│       ├── src-tauri/                # Rust 核心
│       │   ├── src/
│       │   │   ├── lib.rs            # Tauri Commands + 分域 State（默认 GUI 入口）
│       │   │   ├── core/             # 业务核心：Gateway + 各领域模块
│       │   │   │   ├── gateway.rs    # 编排器（可 Clone，无外层 Mutex）
│       │   │   │   ├── neuron/       # Neuron 域（manager / store / selection ...）
│       │   │   │   ├── topic_store.rs / topic_manager.rs
│       │   │   │   ├── providers.rs  # Provider 域
│       │   │   │   ├── tool_registry.rs / mcp.rs / dynamic_tool.rs
│       │   │   │   ├── assistant_session.rs / poller.rs / session_tracker.rs
│       │   │   │   └── storage.rs / config.rs / app_log.rs / events.rs
│       │   │   ├── fileops/          # 文件 / Git / 语义搜索
│       │   │   ├── net/              # 远程模式：axum /healthz /rpc /events + WS
│       │   │   ├── terminal/         # 终端面板（PTY + WS bridge）
│       │   │   ├── tui/              # TUI 界面（ratatui）
│       │   │   └── bin/              # pulsar-cli / pulsar-tui / pulsar-server
│       │   ├── inserts/              # 工具自描述契约（markdown）
│       │   ├── Cargo.toml
│       │   └── tauri.conf.json
│       ├── .env.example              # dev 端口见 tauri.conf.json；本文件仅后端代理覆盖
│       └── package.json
├── docs/                             # 设计文档 / specs / micro_specs / sdd-lab
├── pnpm-workspace.yaml
└── README.md
```

> 仓库根为 workspace 管理入口，应用源码位于 `packages/pulsar-app`。

## 多入口

业务逻辑在 Rust core 实现一次，通过多个入口暴露：

| 入口 | 位置 | 说明 |
|------|------|------|
| **Tauri GUI**（默认） | `src-tauri/src/lib.rs` | 桌面客户端，前端走 Tauri IPC |
| **pulsar-cli** | `src-tauri/src/bin/pulsar-cli.rs` | 命令行：chat / skills / providers / models / call-model 等 |
| **pulsar-tui** | `src-tauri/src/bin/pulsar-tui.rs` | 交互式终端界面（ratatui） |
| **pulsar-server** | `src-tauri/src/bin/pulsar-server.rs` | headless 网络服务：RPC + SSE + WS + 前端静态托管（需 `embed-static` 特性） |

## 快速开始

### 环境要求

- Node.js（LTS）+ [pnpm](https://pnpm.io/)（CI 使用 pnpm 11）
- Rust stable
- 各平台 Tauri 依赖（见 [Tauri v2 官方文档](https://v2.tauri.app/start/prerequisites/)；Linux 需 `libwebkit2gtk-4.1-dev` 等）

### 1. 安装依赖

```bash
pnpm install
```

### 2. 配置环境变量

```bash
cp packages/pulsar-app/.env.example packages/pulsar-app/.env
```

- 前端 dev server 端口**唯一来源**为 `packages/pulsar-app/src-tauri/tauri.conf.json` 的 `build.devUrl`（默认 `http://localhost:1432`）；后端 dev 端口唯一来源为 `PULSAR_PORT`（默认 `8899`）。Vite 从环境变量读取这两个端口，Tauri 由 `tauri.conf.json`（前端）与 Rust `core::config`（后端）读取，始终一致。

  `pnpm tauri:dev` 支持临时自定义前后端端口（在 `packages/pulsar-app` 下）：

  ```bash
  pnpm tauri:dev                                        # 默认（前端 1432 / 后端 8899）
  pnpm tauri:dev --frontend-port 1450 --backend-port 9000
  DEV_FRONT_PORT=1450 PULSAR_PORT=9000 pnpm tauri:dev
  ```
- Provider API Key 等凭据可配置在 `.pulsar/config.json`，或通过环境变量（如 `OPENAI_API_KEY`）注入，优先于配置文件且不入库

### 3. 运行

```bash
# 桌面应用开发（推荐）
cd packages/pulsar-app && pnpm tauri:dev

# 仅前端开发
cd packages/pulsar-app && pnpm dev
```

## 常用命令

在 `packages/pulsar-app` 目录下执行：

| 命令 | 说明 |
|------|------|
| `pnpm dev` | 仅前端 dev server（vite，端口 1432） |
| `pnpm tauri:dev` | 完整桌面开发（Tauri + 前端热更新，可用 `--frontend-port`/`--backend-port` 自定义端口） |
| `pnpm build` | 构建前端产物 |
| `pnpm check` | Svelte 类型检查（svelte-check） |
| `pnpm cli` | 运行 `pulsar-cli` |
| `pnpm tui` | 运行 `pulsar-tui` |
| `pnpm server:dev` | 本地 headless server（`127.0.0.1:8899`，内嵌前端静态资源） |
| `pnpm server:run` | release headless server（端口取 config / 默认 9999） |
| `pnpm server:prod` | 局域网生产模式（`0.0.0.0:9999`） |
| `pnpm server:build` | 构建 headless server（release + embed-static） |

> `server:*` 与 GUI 共享同一套 core 与存储，方便浏览器前端通过 RPC + SSE 连接同一实例。

## 数据存储

数据根目录为 `<storage_root>/.pulsar/`（旧版 `.agent-app` 目录首次启动自动迁移）：

```text
.pulsar/
  config.json          # providers / models / poller / neurons.bootstrap / server 等配置
  sessions/<id>.json   # 会话与消息（JSON）
  app.db               # SQLite：Topic + Neuron 存储
  dynamic_tools.json   # 动态工具配置
  mcp_servers.json     # MCP server 配置
```

## 发布

一键发布（在 `packages/pulsar-app` 目录下）：

```bash
pnpm release        # 小版本 patch（默认）
pnpm release minor  # 中版本
pnpm release major  # 大版本
```

脚本自动完成：累加版本号（同步 `package.json` / `tauri.conf.json` / `Cargo.toml` / `Cargo.lock`）→ 提交并推送 `main` → `release` 分支快进推送 → 打 `pulsar-v<版本>` annotated tag 并推送。要求当前在 `main` 分支且工作区干净。

推送 `release` 后，GitHub Actions（`.github/workflows/publish-pulsar-app.yml`）自动构建并发布：
Windows x64、Linux x64 / Arm64、macOS x64 / Arm64，产物以 `pulsar-v<版本>` 标签发布为 GitHub Release（draft）。

## 文档索引

| 文档 | 内容 |
|------|------|
| [docs/pulsar/architecture.md](docs/pulsar/architecture.md) | 架构设计、数据流、锁纪律 |
| [docs/pulsar/storage.md](docs/pulsar/storage.md) | 存储布局与配置字段 |
| [docs/pulsar/roadmap.md](docs/pulsar/roadmap.md) | 里程碑规划 |
| [docs/specs/](docs/specs/) | 正式规格 |
| [docs/micro_specs/](docs/micro_specs/) | 微规格 |
| [docs/sdd-lab/](docs/sdd-lab/) | SDD 需求迭代记录 |
