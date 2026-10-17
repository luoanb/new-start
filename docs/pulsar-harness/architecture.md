# pulsar-harness · 架构

本文件描述 `packages/pulsar-harness` 的分层边界、模块地图与扩展点。
抽取过程与逐文件层归属见 `docs/specs/2026-10-17_20-21_tauri-skeleton-extraction.md`。

## 1. 分层边界

**基建 = 「换掉全部业务需求它依然存在」的代码。** 三问判据（缺一即不是基建）：

1. 需求清零后它还需要吗？
2. 名字里有没有产品/业务名词？
3. 删掉它是「起不来」（基建）还是「少功能」（功能）？

| 层 | 定义 | 本包 |
|---|---|---|
| L1 基建 | 只服务于「这是一个 app」本身：外壳、存储、日志、事件通道抽象、布局机制、i18n、主题、API 抽象、工程接线 | 保留 |
| L2 通用能力 | 有用但非必需的可选能力：HTTP 远程通道、多入口（CLI/TUI/headless）等 | 不保留 |
| L3 功能 | 具体业务：会话、主题、神经元、服务商与模型等 | 不保留 |

保留「机制」而非「业务变体」：例如 `StateChange` 事件枚举只留 `AppReady` 一个基建变体，
`viewRegistry` 只留 `logs` / `example` 两条注册项，`MainPanelType` 只留 `example`——
但**分发机制、契约机制、注册机制原样保留**，业务包在此扩展。

## 2. 后端模块地图（`src-tauri/src`）

```
core/
  events.rs      StateChange 枚举 + STATE_CHANGED_EVENT 事件名 + StateEmitter 类型
  log_phase.rs   日志 phase 注册表（PhaseInfo + ALL + all_phases()），前端筛选器的唯一数据源
  error.rs       AppError（InvalidInput / StorageError / RuntimeError）+ AppErrorPayload 序列化
infra/
  config.rs      ConfigStore：数据根 → config.json 读写 + config_generation 版本号
  elevate.rs     平台提权命令构造（需 root 的命令经用户授权后由系统原生提权程序重跑，
                 密码由系统弹窗收集，不进入本进程）
  platform.rs    平台差异（如 Linux Wayland IME 环境预处理）
  time.rs        时间工具
sinks/
  app_log.rs     日志汇：内存环形缓冲 + 文件（<数据根>/logs/pulsar-harness.log）+ app://logs 事件
  log_redact.rs  日志脱敏（密钥等敏感字段）
  state_sinks.rs StateEventSink：把 StateChange 广播到前端
stores/
  storage.rs     StorageBase：数据根选择（Project / AppData / Cwd）+ 旧目录迁移
lib.rs           run()：setup（数据目录解析 → 日志初始化 → panic hook → 清 WebKitCache →
                 注入 ConfigStore / StateEmitter → 手动建窗 + no-store 响应头 → emit AppReady）
                 + 5 个命令（见白名单）+ invoke_handler
main.rs          prepare_linux_wayland_ime_env() + run()
```

### 关键机制

- **单例**：日志 controls 用 `OnceLock<LogControls>` 持有；`ConfigStore` / `StateEmitter` 经 `app.manage` 注入。
- **原子写**：配置写入走 tmp + rename，避免半写坏文件。
- **手动建窗**：`tauri.conf.json` 主窗口 `"create": false`，由 `setup` 里
  `WebviewWindowBuilder::from_config` 显式创建，以便挂 `on_web_resource_request`
  注入 `Cache-Control: no-store`（WebKitGTK 磁盘缓存跨重启复用旧 bundle 会导致样式错乱）。
- **启动清缓存**：每次启动删除 `<app_data_dir>/WebKitCache`（WebKitGTK 不完全遵守 no-store）。

## 3. 前端模块地图（`src`）

```
lib/api/
  types.ts       ApiClient 接口 + LogEntry / LogLevel / PhaseInfo / LogListResult / StateChangePayload
  contracts.ts   Contract<P,R> + def<P,R>(cmd)；c 为全部命令契约（与后端 invoke_handler 一一对应）
  tauriClient.ts 唯一实现：invoke（IPC）+ listen（app://state-changed）+ health
  index.ts       api 实例 + isTauriEnv + 类型 re-export
lib/layout/
  layoutTypes.ts   LayoutState / MainPanelType / DEFAULT_LAYOUT / LAYOUT_VERSION / normalizePaneTarget / BOUNDS / clamp
  layoutStorage.ts LayoutStorage 接口 + localStorage 实现（key 迁移：pulsar:layout → pulsar-harness:layout）
  LayoutStore.svelte.ts  布局状态与全部变更操作（容器视图、分栏面板、grow 权重、持久化）
  views.ts        viewRegistry / mainViews / activityItems / mainPanelMeta / canMoveTo（视图注册表）
  viewContext.ts  ViewContext + setViewContext / useViewContext（容器与内容解耦）
  ViewContainer.svelte  视图容器（tab、⋯显隐菜单、跨容器拖拽换容器、同容器重排）
  ViewHost.svelte       纯动态挂载器（registration.component + 可选 panel context）
  EditorTabs.svelte     主区 tab 栏（多选、右键批量关闭、拖拽重排/换分栏）
  Splitter.svelte + useResizable.svelte.ts  拖拽分割条（rAF 节流、pointer capture）
  ActivityBar.svelte    左侧 icon 轨
  WindowEdgeResize.svelte  无边框窗口的边缘 resize 热区（仅 Tauri 环境挂载）
  （自绘标题栏）    `tauri.conf.json` 为 `decorations: false`，窗口 chrome 由 `+page.svelte` 的 header 承载：
                 空白区拖拽 / 双击最大化 + 最小化 / 最大化⇄还原 / 关闭（仅 Tauri 环境渲染）
lib/components/  通用组件：Select / Toggle / Tooltip / ContextMenu / ConfirmDialog /
                 CopyButton / ErrorBanner / LogPanel / ThemeSwitcher / LocaleSwitcher / ExampleView
lib/i18n/        词典（translations.ts）+ t()/tMap()/setLocale/getLocale（模块级 runes，响应式）
lib/stores/dataStore.svelte.ts  事件订阅消费端（幂等 bootstrap + ready/lastKind + dispose）
lib/hotkey/hotkeyService.ts     单例热键服务（DOM 根 + 忽略规则约定，未命中不 preventDefault）
lib/theme.ts      主题偏好唯一来源（applyTheme / applyThemeOnBoot / readTheme）
lib/utils/        CopyToClipboard（Clipboard API + execCommand 兜底）/ formatInvokeError
routes/+page.svelte  组合根：外壳布局（header 兼作自绘标题栏）+ ViewContext 注入 + 启动接线（主题/订阅/热键/诊断）
```

### 关键机制

- **命令契约收束**：业务层只允许 `api.call(c.xxx, params)`；参数与返回类型由 `Contract<P,R>` 编译期校验，
  禁止裸写 `invoke("cmd", …)` 魔法字符串。
- **容器与内容解耦**：`ViewContainer` 只消费注册表（id → component），视图组件经 `useViewContext()`
  自取数据/命令，组合根不逐个传 props。**新增视图 = 注册表加一条，`+page.svelte` 零改动。**
- **布局持久化与版本迁移**：`LAYOUT_VERSION` 变更时在 `layoutStorage.ts` 的 `normalize()` 迁移链补一支分支；
  跨包改名通过一次性 key 迁移（`LEGACY_KEY`）承接。
- **保活视图**：注册项带 `keepAlive: true` 时首次激活后常驻挂载，切走仅 `visibility` 隐藏
  （让持有连接/缓冲的实例不被销毁且仍能量到真实尺寸）。

## 4. 前后端契约

- **命令**：Tauri IPC，`invoke_handler` 全量白名单 5 条——`debug_storage_path` / `logs_list` /
  `logs_phases` / `logs_clear` / `logs_set_level`。新增命令须同时改后端注册与 `contracts.ts`。
- **事件**：
  - `app://state-changed`：状态变更广播（当前只有 `{kind: "app_ready"}`）。
  - `app://logs`：实时日志流（Tauri 专属；`isTauriEnv === false` 时前端跳过订阅）。

## 5. 扩展点

| 需求 | 改动位置 |
|---|---|
| 新增视图（侧栏/信息栏/底部面板） | `lib/layout/views.ts` 的 `viewRegistry` |
| 新增主区分栏面板类型 | `layoutTypes.ts` 的 `MainPanelType` + `views.ts` 的 `mainViews` / `mainPanelMeta` |
| 新增后端命令 | 后端 `#[tauri::command]` + `invoke_handler`；前端 `contracts.ts` 加 `def` |
| 新增状态变更域 | 后端 `core/events.rs` 的 `StateChange` 加变体；前端 `types.ts` 的 `StateChangePayload` 同步 + `dataStore` 分发 |
| 新增日志 phase | 后端 `core/log_phase.rs`（`ALL` 加一条）；前端下拉自动跟随 `logs_phases` |
| 新增文案 | `lib/i18n/translations.ts` 的 `Translations` 类型 + `en` / `zh` 同步 |
