# Technical Plan / 技术方案: Agent App 运行日志

## Requirement Baseline / 需求基线

- 对应需求文档：`docs/sdd-lab/2026-08-01_01-11_agent-app-runtime-logs/requirements.md`
- 需求确认状态：Q1–Q3 已关闭（滚动按大小；实时 emit；默认 info + GUI 可调 verbosity）
- 本方案覆盖：Rust 日志设施、滚动文件、内存环缓 + Tauri emit、Logs 面板 UI/过滤/级别、神经元初始化打点
- 不覆盖：远程采集、完整 prompt 落盘、TUI 内嵌日志视图

## Current Project Facts / 当前项目事实

- `Cargo.toml` 无 `tracing` / `log`；仅有零散 `eprintln!`
- GUI 底部 Panel 已有 `logs` tab，UI 为 `Logs placeholder`（`+page.svelte`）
- Tauri `lib.rs`：`Gateway` 在 `Mutex` 中；尚无 `AppHandle` emit 日志通道
- 存储根：`.agent-app/`（与 config / app.db 同级）
- 神经元 bootstrap 失败目前只在 TUI/CLI stderr warning；GUI 未调用 bootstrap

## Open Questions / 开放问题

全部已关闭：

| Q | 决策 |
| --- | --- |
| Q1 滚动 | A：单文件约 5–10MB，保留最近 5 个 |
| Q2 刷新 | A：Tauri 实时 `emit` |
| Q3 级别 | 默认 `info`；GUI 可切换 verbosity |

## Solution Options / 方案候选

### Option A / 方案 A：tracing + Layer 双通道（推荐）

- 推荐：是
- 摘要：`tracing` + `tracing-subscriber`；`tracing-appender` 按大小滚动写文件；自定义 `Layer` 写入有界环缓并 `AppHandle::emit("app://logs", entry)`；前端 Logs 面板订阅 + 过滤 + 调级别
- 优点：标准生态、可过滤字段、文件与 UI 同源、级别可动态刷新
- 缺点：新增依赖；需在 Tauri setup 注入 `AppHandle`
- 风险：高频 emit 压 UI → 环缓上限 + 前端批量合并缓解

### Option B / 方案 B：仅文件 + 前端读尾

- 推荐：否
- 缺点：难实时、过滤弱、与 Q2（emit）不符

## Decision / 方案决策

- Selected：Option A
- Decision Owner：用户（需求三点 + Q1–Q3）
- Decision Time：2026-08-01 01:14
- Open Questions：已关闭  
- 等待：用户明确批准本技术方案后执行

## API Design / API 设计

### 日志条目（前后端契约）

```ts
type LogLevel = "error" | "warn" | "info" | "debug" | "trace";

interface LogEntry {
  ts: string;          // RFC3339
  level: LogLevel;
  target: string;      // e.g. agent_app_lib::core::neuron_manager
  message: string;
  fields?: Record<string, string>; // phase, system_type, error_code...
}
```

### Tauri 事件 / 命令

| 名称 | 类型 | 说明 |
| --- | --- | --- |
| `app://logs` | event | 推送单条或小批量 `LogEntry[]` |
| `logs_snapshot` | command | 返回环缓当前快照（面板打开时补历史） |
| `logs_set_level` | command | 设置全局过滤级别：`error\|warn\|info\|debug\|trace` |
| `logs_get_level` | command | 读当前级别 |
| `logs_clear_buffer` | command | 清空环缓（不影响文件） |
| `logs_dir` | command | 返回日志目录路径（可选揭示） |

### 文件

- 目录：`{storage_root}/logs/`
- 文件名：`agent-app.log` + 滚动后缀（由 appender 约定）
- 策略：单文件 **8MB**，保留 **5** 个历史（合计约 40MB 上限）
- 环境变量：`RUST_LOG` / `AGENT_APP_LOG` 可覆盖默认 filter（默认 `info`）

### 前端 Logs 面板

- 替换 placeholder：工具条（level 下限、target 下拉/输入、keyword 输入、清空、显示当前 verbosity）
- 列表：虚拟滚动可选；本期普通列表 + 上限渲染（如只显示过滤后最近 500 条）即可
- 订阅 `app://logs`；挂载时 `logs_snapshot` 灌入
- 过滤在前端对缓冲做（level ≥ 所选、target contains、message/fields keyword）

### 核心打点（首批）

`neuron_manager` / `gateway`：

- `bootstrap_ready` start/ok/err
- `ensure_creator_neuron` cache/hit/create
- `ensure_system_neuron` exists/create/phases
- `select_candidates` existing/fill count
- `create_generated_neuron` / `generate_draft` start/ok/err(+code)
- `select_one` / llm_select fallback

不记录完整 prompt；可记 `phase`、`system_type`、`candidate_count`、`error_code`。

### GUI bootstrap（附带最小修复，可选同迭代）

- 在 Tauri `setup` 中 `bootstrap_neurons().await`（失败 warn 入日志），否则 GUI 永远看不到初始化轨迹。  
- 若希望本迭代只做日志、bootstrap 另开，可在批准时说明；**推荐同做**，否则 Logs 面板对当前问题帮助有限。

## Execution Steps / 执行步骤

1. 依赖：`tracing`、`tracing-subscriber`、`tracing-appender`、`parking_lot`（或 `std::sync` 环缓）
2. 新模块 `core/app_log.rs`：init、RingBuffer Layer、set_level、snapshot
3. `lib.rs` setup：init logger、持有 `AppHandle` 供 emit、注册 logs_* commands、（推荐）bootstrap_neurons
4. neuron / gateway 关键路径 `tracing::info!/warn!/error!`
5. Svelte：`LogPanel.svelte` + 接入 `+page.svelte`；listen + filter + level 控件
6. 文档：`docs/agent-app/` 短说明；回写 lifecycle
7. 验证：启动后 Logs 可见 bootstrap 阶段；人为断模型可见失败阶段；造大日志确认滚动

## Risk And Mitigation / 风险与缓解

- emit 风暴：环缓 2000 条；Layer 侧可合并或限频（如 50ms 批量）
- TUI 无 AppHandle：文件日志照常；无 emit
- 动态级别：用 `reload::Handle` 或原子最小 level 在 Layer 内过滤
- 日志 init 失败：降级 stderr，不 panic 启动

## Execute Checkpoint / 执行检查点

- 当前理解：文件 8MB×5 + 环缓 emit + Logs 过滤 + 默认 info/GUI 可调；优先服务 bootstrap 诊断
- 核心目标：前台能快速定位 `assistant_select_neuron` 卡在哪一步
- 下一步：批准后按 Step 1–7 实施
- 风险：高频日志与 UI 性能
- 验证：面板可见 + 过滤可用 + 文件滚动 + bootstrap 打点齐全
