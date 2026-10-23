# pulsar-tauri-template · 数据目录契约

数据目录选择的唯一决策点在 `src-tauri/src/stores/storage.rs` 的 `StorageBase`，
日志落盘位置由 `sinks/app_log.rs` 决定。本文件是这两处代码的契约说明。

## 1. 数据根

数据根 = `<基座>/.pulsar-tauri-template`，目录名常量 `STORAGE_DIR_NAME = ".pulsar-tauri-template"`。

| 入口 | 基座 | 数据根 |
|---|---|---|
| GUI 开发态（debug 构建） | `StorageBase::Project` | `<repo>/packages/pulsar-tauri-template/.pulsar-tauri-template` |
| GUI 发布态（release 构建） | `StorageBase::AppData` | `<app_data_dir>/.pulsar-tauri-template` |
| `StorageBase::Cwd`（脚本 / 测试） | 进程 cwd | `<cwd>/.pulsar-tauri-template` |

- GUI 基座由 `StorageBase::for_gui(app_data_dir)` 按 `cfg!(debug_assertions)` 选择：
  **开发态落项目目录**（数据可直接检查），**发布态落系统应用数据目录**。
- 发布包不能沿用项目目录：安装位置不可写，且 `CARGO_MANIFEST_DIR` 只记录构建机路径
  （CI 上为 `/home/runner/work/...`），运行期写配置会报权限错误。
- `app_data_dir` 由 Tauri `app.path().app_data_dir()` 提供（注册于 `identifier = com.pulsar.harness`）。

## 2. 目录内容

```
<数据根>/
├── config.json           # 应用配置（infra/config.rs）
└── logs/
    └── pulsar-tauri-template.log # 日志文件（sinks/app_log.rs）
```

- `config.json`：`AppConfigFile` 只强约束版本号字段，其余键以 `#[serde(flatten)] extra`
  透传保存——业务包扩展配置项**无需改基座结构**。写入走 tmp + rename 原子替换。
  配置文件内不含任何业务 section（`server` / `poller` / `hooks` 等已随 L2 剥离）。
- 日志文件：文件名 `pulsar-tauri-template.log`，日志目录相对数据根为 `logs/`；
  同时有一条内存环形缓冲（供 `logs_list` 快照）与 `app://logs` 实时事件流。
  文件日志级别可通过环境变量 `pulsar_tauri_template_LOG` 覆盖，运行期可经 `logs_set_level` 调整。

## 3. 旧目录迁移

`StorageBase::resolve()` 内调用 `migrate()`：

- 若 `<基座>/.pulsar-tauri-template` **不存在**且 `<基座>/.pulsar` **存在**，则把旧目录整体 `rename` 为新目录；
- 新目录已存在时**不动**旧目录（不合并、不删除）；
- 迁移失败**不阻断启动**：降级为空白数据目录，下次运行重新生成。

## 4. 变更约束

- 目录名变更必须**同时**改 `STORAGE_DIR_NAME` 与前端 localStorage 前缀约定
  （布局 key：`layoutStorage.ts` 内 `pulsar-tauri-template:layout` + 一次性迁移自 `pulsar:layout`）。
- 调整数据根位置时，更新本文件第 1 节的表格。
- 迁移逻辑只允许「旧 → 新」单向整体搬迁；如需二次改名，先把当前新名降级为 `LEGACY_STORAGE_DIR_NAME`。
