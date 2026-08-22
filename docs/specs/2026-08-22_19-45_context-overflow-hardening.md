# Spec: 上下文溢出加固（Context Overflow Hardening）

> 事故：`conv_1787253076882845861` 单条 grep 工具结果 3MB → 模型 400 `maximum context length is 1048576 tokens, requested 1089772` → poller 每 ~5s 无限重试空转，当日 9+ 次同错误，每次空转烧 ~100 万 token。

## Goal

- 要解决什么问题：任何单条消息（尤其工具结果）都不允许撑爆模型上下文；压缩机制对「单条超大消息」必须能兜底；poller 对同一失败不得无限重试。不只修 grep 一处，而是堵住整类「上下文被单条巨型内容打爆 + 无防线 + 无限空转」的洞。
- 验收结果：三层防线落地且可测——①源头：grep 遵守 ignore、不返回产物内容，工具结果超预算即截断；②兜底：压缩在单条超大消息下触发强制降级；③防空转：poller 连续失败退避并最终熔断暂停会话。存量会话停止烧 token。

## Done Contract

- 什么算完成：L1 源头截断 + L3 压缩兜底 + L4 轮询熔断全部落地；`conv_1787253076882845861` 的 3MB 结果被处理（截断/暂停），poller 不再对它空转。
- 由什么证明：`cargo test --lib` 全绿（新增截断/压缩/熔断单测）；存量会话恢复后连续多轮无 400 上下文超限；日志不再出现持续 `poll step failed`。
- 哪些情况仍算未完成：grep 仍能返回 build 产物内容；单条工具结果仍超限进入 wire；压缩仍因消息数少而静默放弃；poller 对同一错误无限重试。

## Scope

- In:
  - L1 工具层：grep 复用 ignore 规则（与 list_directory 对齐）、`default_ignore()` 补产物目录、grep 匹配行/结果总量截断
  - L2 会话层：工具执行结果统一大小预算（公共落库点预防式截断 head/tail + 截断标记 + 完整结果可选落盘）
  - L3 压缩层：`compaction_boundary` 体量感知（不再被消息条数拦截）、单条超限强制降级、`estimate_tokens` 估算修正、阈值对齐社区 80% 经验
  - L4 轮询层：会话级连续失败计数 + 指数退避 + 熔断暂停、错误分类（400 上下文超限 → 先触发 L3 再试一次）
  - L0 存量：处理 `conv_1787253076882845861`（3MB 结果截断或暂停该会话）
- Out:
  - 跨会话持久记忆（Mem0 类，独立迭代）
  - 模型侧 server-side compaction（Claude Compact API 类）
  - Poller 调度机制重构（保留现有多话题并发模型）
  - 非 pulsar-app 链路、前端视觉改动

## Facts / Constraints

- 已确认事实（代码）：
  - grep 工具用 WalkDir 直扫 [fs.rs:573-638](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/fileops/fs.rs#L573-L638)，**不应用** `is_ignored`（list_directory 在 [fs.rs:214](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/fileops/fs.rs#L214) 应用了）；`MAX_GREP_MATCHES=2000` 只限条数不限单行大小
  - `default_ignore()` = `[".git","node_modules","target","dist",".pulsar",".DS_Store"]`，**缺 `build`**（[workspace.rs:43-46](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/fileops/workspace.rs#L43-L46)）
  - 压缩 hook 挂 IP-2 AfterPersistInput（[gateway.rs:419](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/gateway.rs#L419) 注册、[conversation_runner.rs:174](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/conversation_runner.rs#L174) 触发），poller 路径必经；但 `ensure_fits` 被 `compaction_boundary` 拦截（`total ≤ keep_last×2` → 返回 0，[compactor.rs:85-108](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/compactor.rs#L85-L108)），日志 0 次 compaction = 执行了但静默放弃
  - `estimate_tokens = chars/4`（[compactor.rs:13-15](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/compactor.rs#L13-L15)）对 ASCII 低估；`CompactionConfig` 默认 `enabled=true, threshold_ratio=0.7, keep_last=10`
  - poller 无失败退避/熔断：`step_poller` Err → 记日志 → unregister → 下个 tick 重试（[assistant_session.rs:666-686](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L666-L686)）
  - 结构性缺口：即使压缩放行，3MB 结果是最新消息、永远在保留区间 → 压缩覆盖不到 → 必须在源头/单条层截断
- 社区经验（预防式而非反应式）：
  - 80% 阈值触发压缩，留 20% 给压缩操作本身与下一轮（过早 60% 频繁失忆，过晚 95% 压缩死锁）
  - 工具结果**执行时即截断**：超阈值（如 8-12K 字符）→ 保留 head/tail + 中段提示「已截断，请用更精确参数重试」，完整结果落盘供追溯
  - 压缩通用模式：保护 head、摘要 middle、保留 tail 逐字
  - 双保险层：agent 压缩层 + 网关安全网（错峰触发），不能都设同一阈值
  - 裸截断（直接丢最老消息）会破坏多步推理，必须带摘要/标记
  - 错误分类是重试前提：瞬时（429/5xx/超时）→ 指数退避+jitter 重试（base×2^n，上限 max_delay，≤3 次）；永久（认证/参数 400/404/422）→ 不重试直接修/上报；`context_length_exceeded` → **不可原样重试**，必须先改上下文（截断/摘要）再试
  - 熔断器三态：CLOSED（正常，失败计数）→ 连续失败≥阈值（社区约 5 次/30s）→ OPEN（直接拒绝）→ 冷却期过 → HALF_OPEN（放一次探测）→ 成功回 CLOSED / 失败回 OPEN；预算 cap：80% fail-open、100% fail-closed
  - 重试陷阱：无限重试（retry loop 与 DoS 无异）、对永久错误重试（纯烧钱）
- 技术约束：真相源原则——落库的会话消息是真相源，截断需明确「落库即截断」还是「wire 层投影截断」；改动不破坏既有 376 个测试
- 已知风险：截断阈值过小丢关键上下文；grep ignore 误伤用户显式指定的搜索路径；熔断误伤正常会话（需连续失败才触发）

## Open Questions

- [x] Q1 工具结果统一上限与截断位置：**已确认**——所有工具的返回结果都应上下文可控（有统一上限，从工具设计时就应自带上限意识），统一在**落库时截断**。落点：统一工具结果预算（公共落库点 `cap_tool_result`）+ 各工具自带约束（grep 行截断、read_file 上限收紧等）。
- [x] Q2 完整工具结果是否落盘：**已确认**——不落盘（选项 A）。截断即烧：对话只保留 head/tail + 截断提示，完整内容直接丢弃。理由：实现最简、无磁盘与清理负担；用户同时要求另行复盘全部工具设计（见新技术方案）。
- [x] Q3 poller 熔断触发时机：**已确认方向**——结合社区经验（错误分类 → 指数退避 → 熔断三态 → budget cap），具体参数与状态机见接口契约设计第 5 节。
- [x] Q4 修复范围：**已确认**——三层防线（L1+L3+L4）+ 存量处理一次全做。

## Restated Understanding

- 我理解当前任务是：把「单条工具结果撑爆上下文」从事故级问题，变成有源头截断、有压缩兜底、有轮询熔断三层防护的可防问题，并处理存量事故会话。
- 当前核心目标是：任何单条消息都不撑爆上下文；任何会话都不无限空转；存量会话止损。
- 当前边界是：只改 pulsar-app 会话链路（工具层/压缩层/轮询层/配置），不做持久记忆、不改 poller 调度骨架、不动前端。
- 暂不处理：Mem0 类持久记忆、server-side compaction、跨项目链路。

## 接口契约设计

```rust
// ── 1. 配置（config.json 新增 context 节，默认值内置于代码）──
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextSafetyConfig {
    /// 单条工具结果最大字符数（超过则 head/tail 截断，社区参考 8-12K）
    pub tool_result_max_chars: usize,        // 默认 12_000
    /// 完整工具结果落盘开关（true 时写 storage_root/tool-results/<ts>-<tool>-<id>.txt）
    pub persist_full_tool_results: bool,     // 默认 true
    /// grep 单条匹配行最大字符数
    pub grep_line_max_chars: usize,          // 默认 2_000
    /// 压缩触发阈值（窗口比例，社区 0.8，留 20% 给压缩本身）
    pub compress_threshold_ratio: f64,       // 默认 0.8（替代 CompactionConfig.threshold_ratio=0.7）
    /// 单条消息估算 token 预算（超过则 wire 层强制 head/tail 截断）
    pub single_message_token_budget: usize,  // 默认 32_000
    /// poller 连续失败退避/熔断（社区：错误分类 → 指数退避 → 熔断三态 → budget cap）
    pub poll_backoff_after: usize,           // 默认 3（连续失败开始退避）
    pub poll_pause_after: usize,             // 默认 6（连续失败熔断暂停，需人工恢复）
    pub backoff_max_skips: u32,              // 默认 8（单次最多跳过 tick 数，退避上限）
}

// ── 2. 工具结果截断（L1+L2 公共入口，execute_tools → persist 前调用）──
pub fn cap_tool_result(tool_name: &str, content: String, cfg: &ContextSafetyConfig) -> String;
// 行为：len ≤ tool_result_max_chars 原样返回；否则 head(1/3) + 截断提示 + tail(其余)，
// 提示含原始长度、省略量、落盘路径（若开启）、建议缩小参数重试。

// ── 3. grep ignore 对齐（L1）──
// fs::grep 复用 workspace.ignore_rules + is_ignored（与 list_directory 同源），
// 并在遍历时跳过 default_ignore() 中的产物目录；匹配行按 grep_line_max_chars 截断。

// ── 4. 压缩体量感知（L3，compactor.rs 改造）──
pub fn compaction_boundary(&self, conversation: &Conversation, budget: usize) -> usize;
// 改判据：估算 token ≥ threshold 即触发（不再被 total ≤ keep_last×2 拦截）；
// keep_last 仍保证 tail 完整性（社区：保护 head / 摘要 middle / 保留 tail）。
pub fn force_fit_single_message(&self, msgs: &mut [Message], budget: usize) -> bool;
// 扫描单条估算 token > single_message_token_budget → wire 层 head/tail 截断 + 标记
// （真相源由 L2 落库截断保护；此处兜底覆盖历史会话/第三方注入）。

// ── 5. poller 熔断（L4，assistant_session.rs）──
// 触发时机设计（社区共识：先分类再重试，不是所有失败都值得重试）：
//   错误分类：
//     - context_length_exceeded（400 上下文超限）→ 「可修复但不可原样重试」：
//       下一轮先走 L3 强制降级（force_fit_single_message / 压缩）再试；降级后仍失败 → 计入连续失败
//     - 瞬时错误（429 / 5xx / 网络超时）→ 退避重试（指数 + jitter，社区 base×2^n，上限 max_delay）
//     - 永久错误（认证 / 参数类 400 / 404）→ 不可修复，不重试，直接计入熔断
//   状态机（CLOSED → BACKOFF → COOLDOWN，对齐社区熔断三态，简化为本 poller 可表达的两级）：
//     CLOSED：正常轮询；失败计数归零于成功
//     BACKOFF：连续失败 ≥ poll_backoff_after → 该会话跳过 2^n 个 poll tick（n 从 1 起，封顶 backoff_max_skips）
//     COOLDOWN：连续失败 ≥ poll_pause_after → 该会话熔断暂停（跳过轮询），等价社区 OPEN，
//       恢复方式 = 用户手动恢复（等价 HALF_OPEN 探测，避免自动探测浪费 token）
pub struct SessionFailureState {
    consecutive_failures: u32,     // 连续失败计数（成功即归零）
    backoff_skips_remaining: u32,  // 当前退避剩余跳过 tick 数
    last_error_class: ErrorClass,  // 最近一次错误分类
}
enum ErrorClass { ContextLengthExceeded, Transient, Permanent }
// step_poller Err 时：ErrorClass::ContextLengthExceeded → 标记会话需强制降级；
// 其余按分类走 BACKOFF / COOLDOWN；COOLDOWN 会话在 poll 循环顶部跳过（扩展 skip_polling）。
```

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是——三层防线均直接对应「不撑爆 / 能兜底 / 不空转」三个验收点。
- 若否，偏差在哪里：无。
- 是否需要调整本轮目标或范围：待 Q1-Q4 用户回答后收敛。

## Checkpoint Summary

- 当前任务理解：`conv_1787253076882845861` 事故是「grep 产物结果 3MB → 上下文超限 400 → poller 无熔断无限空转」，压缩 hook 本身已接线但被消息数守卫拦截且结构性覆盖不到最新超大单条；需按三层防线系统性修复。
- 当前核心目标：源头截断 + 压缩兜底 + 轮询熔断 + 存量止损，全部可测。
- 当前进度：✅ 全部执行完成——L1 工具层截断（grep/read_file/git_blame/neuron）、L2 统一落库兜底（cap_tool_result）、L3 压缩体量感知（boundary 守卫 + force_fit_single_message + estimate_tokens 修正）、L4 poller 熔断（错误分类 + 指数退避 + 熔断暂停）、配置节（config.json `context` 节）、存量止损（启动时 sanitize_oversized_messages）。`cargo test --lib` 384 全绿。
- 涉及文件 / 模块：`fileops/fs.rs`、`fileops/workspace.rs`、`fileops/gitops/repo.rs`、`core/neuron/store.rs`、`core/neuron/tools.rs`、`core/context_safety.rs`（新）、`core/round_executor.rs`、`core/compactor.rs`、`core/models.rs`、`core/assistant_session.rs`、`core/gateway.rs`、`core/config.rs`、`core/conversation_store.rs`、`.pulsar/config.json` 文档
- 风险：截断丢上下文；grep ignore 误伤显式路径；熔断误伤正常会话——均由「连续失败才触发 + 可配置 + 可手动恢复」缓解。
- 验证方式：`cargo test --lib`（新增截断/压缩/熔断/配置/存量清理单测）；存量会话恢复后观察日志无持续 400。
- Execution Approval: `Approved`

## Change Log

- 2026-08-22 19:45: 故障定位与压缩链路核查完成（见 lifecycle 记录，spec 承接）；社区经验调研并纳入方案；spec 落盘。状态：待用户确认 Open Questions 与批准执行。
- 2026-08-22 20:xx: Q1-Q4 用户确认（落库截断 / 完整结果不落盘 / 熔断结合社区经验 / 三层防线+存量一次全做）；Execution Approval 改为 Approved。
- 2026-08-22 22:xx: 全部实现落地并回归——L1（grep ignore+行截断、read_file 字节封顶、git_blame 行数上限、neuron 网络/候选上限）、L2（cap_tool_result 落库兜底）、L3（compaction_boundary 体量感知、force_fit_single_message、estimate_tokens 修正）、L4（ErrorClass + SessionFailureState 指数退避/熔断）、配置节（ContextSection/ContextSafetyConfig）、存量止损（sanitize_oversized_messages 启动幂等清理）。`cargo test --lib` 384 全绿。

## Validation

- Self-check: 三层防线与三个验收点一一对应；改动均为增量（新配置节 + 单测），不破坏既有测试。
- Static checks: ✅ `cargo check --lib` 通过（无警告级错误）。
- Runtime / Test: ✅ `cargo test --lib` 384 通过（新增 cap_tool_result/cap_text、SessionFailureState、config context 节、sanitize 幂等、compactor 体量感知等单测）。
- Human confirmation: 待存量会话恢复后观察日志无持续 `poll step failed`。
- 结果汇总：三层防线 + 配置化 + 存量止损全部落地，测试全绿。
- 核心目标是否已由证据证明完成：是（代码落地 + 回归通过）。
- 若未完成，当前剩余差距：存量会话实际恢复观察（依赖用户重新启用 poller）。
- 剩余风险：截断阈值、熔断参数需实际运行校准；grep ignore 对显式路径的误伤需真实场景观察。

## Resume / Handoff

- 当前状态：✅ 实现与回归完成，`cargo test --lib` 384 全绿；存量会话已具备启动自动止损与 L3/L4 运行时兜底。
- 当前卡点：无。
- 下一步唯一动作：用户重启应用/重新启用 poller，观察 `conv_1787253076882845861` 不再出现 400 上下文超限与持续 `poll step failed`。
- 下一轮核心目标：存量会话实际恢复验证；如有需要校准 `context` 节参数。

## 参考资料（社区经验）

- AI Agent 对话太长怎么办：三种压缩策略和一个自动兜底（80% 阈值、Auto/Micro/Manual 三策略）
- Hermes / Claude 上下文压缩实践（双保险层错峰：压缩层 + 网关安全网）
- OpenClaw ToolResultCompactor RFC（工具结果执行时截断 + 落盘 + 提示，预防式而非反应式）
- lite_agent PR：工具结果长度上限 12K 字符 + head/tail 截断 + 引导更精确重试
- Why Your AI Agent Keeps Losing Context（token 预算、loop breaker、裸截断破坏多步推理）

## 关联技术方案

- 工具全量审计与加固（用户新增要求，独立 spec）：`docs/specs/2026-08-22_21-22_tool-context-safety-audit.md`——逐工具返回上限审计（高风险 git_blame/read_file/grep/neuron 4 项）+ 统一发送链路兜底，与主 spec 三层防线合并为完整方案。
