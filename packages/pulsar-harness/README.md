# pulsar-harness

只含「与功能无关」基建的 Tauri v2 应用骨架：把 `packages/pulsar-app` 中的 L1 基建抽出，
剥离全部通用能力（L2）与业务功能（L3），得到可直接作为新应用起点的最小壳。

## 判定标准

**基建 = 「换掉全部业务需求它依然存在」的代码。** 三问判据：

1. 需求清零后它还需要吗？（不需要 → 不是基建）
2. 名字里有没有产品/业务名词？（有 → 不是基建）
3. 删掉它是「起不来」还是「少功能」？（「少功能」→ 不是基建）

## 保留范围

| 层 | 内容 | 处置 |
|---|---|---|
| L1 基建 | 应用外壳与窗口 chrome（自绘标题栏：拖拽 / 最小化 / 最大化 / 关闭 + 边缘缩放）、配置存储、日志（含脱敏）、事件通道抽象、布局系统（容器 / 分栏 / 折叠 / 持久化）、i18n、主题、通用组件、API 抽象层与契约、工程接线 | **保留**（本包） |
| L2 通用能力 | `net/`（HTTP 远程通道）、`bin/` 三个入口、终端、git 集成等 | **不保留** |
| L3 功能 | 会话 / 主题 / 神经元 / 服务商与模型等业务 | **不保留** |

## 目录

```
packages/pulsar-harness/
├── src/                     # SvelteKit 5 前端（runes）
│   ├── lib/api/             # ApiClient 接口 + Contract 契约（唯一 IPC 通道）
│   ├── lib/layout/          # 布局机制：ViewContainer / EditorTabs / Splitter / 注册表
│   ├── lib/components/      # 通用组件（Select / Tooltip / ContextMenu / LogPanel …）
│   ├── lib/i18n/            # 中英词典与 t()
│   ├── lib/stores/          # dataStore（事件订阅消费端）
│   └── routes/+page.svelte  # 组合根：外壳 + ViewContext 注入
└── src-tauri/               # Rust 后端
    └── src/
        ├── core/            # events / log_phase / error
        ├── infra/           # config / elevate / platform / time
        ├── sinks/           # app_log（内存缓冲 + 文件 + 事件）/ state_sinks / log_redact
        └── stores/          # storage（数据根 + 版本迁移）
```

## 快速开始

```bash
pnpm install --no-frozen-lockfile     # 仓库根目录
cd packages/pulsar-harness
pnpm tauri:dev                        # 起开发窗口（前端 dev 端口 1442）
pnpm check                            # svelte-check
cargo check --manifest-path src-tauri/Cargo.toml
```

无需先 `pnpm build` 再 `cargo check`：本包不含 `rust-embed` / `embed-static`，
前端静态资源由 Tauri 的 `frontendDist` 在打包阶段消费。

## 后端命令白名单（`invoke_handler` 全量）

| 命令 | 用途 |
|---|---|
| `debug_storage_path` | 返回后端数据根（诊断用） |
| `logs_list` | 日志条目 + 当前级别 + 日志目录 |
| `logs_phases` | 日志 phase 注册清单（前端筛选器数据源） |
| `logs_clear` | 清空内存日志缓冲 |
| `logs_set_level` | 运行时调整日志级别 |

## 扩展方式

- **新增视图**：在 `src/lib/layout/views.ts` 的 `viewRegistry` 加一条（或 `mainViews` / `mainPanelMeta`），
  `+page.svelte` 无需改动。视图所需数据 / 命令经 `useViewContext()` 自取（见 `ExampleView.svelte`）。
- **新增命令**：后端加 `#[tauri::command]` 并在 `invoke_handler` 注册，前端在 `src/lib/api/contracts.ts`
  加一条 `def<Params, Result>("cmd_name")`，调用一律 `api.call(c.xxx, params)`（禁止裸 `invoke` 字符串）。
- **新增翻译**：在 `src/lib/i18n/translations.ts` 的 `Translations` 类型与 `en` / `zh` 两份词典同步添加；
  `t()` 查不到 key 时回落为 key 原文。

## 文档

- 架构与分层边界：`docs/pulsar-harness/architecture.md`
- 数据目录契约：`docs/pulsar-harness/storage.md`
- 抽取过程 spec：`docs/specs/2026-10-17_20-21_tauri-skeleton-extraction.md`

## 发布

`.github/workflows/publish-pulsar-harness.yml`，推 `release` 分支或手动触发；
产物 tag 前缀 `pulsar-harness-v`。
