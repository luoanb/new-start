# Spec: 工具上下文安全审计与加固（Tool Context Safety Audit）

> 背景：`2026-08-22_19-45_context-overflow-hardening` 事故暴露 grep 单条 3MB 撑爆上下文。用户要求**复盘全部工具设计**：任何工具在任何参数组合下，都**绝对不允许**产生撑爆上下文的返回。本 spec 是对全量工具的审计结论 + 加固方案，与主 spec（三层防线）互补：主 spec 管「统一防线」（L1 落库截断 / L3 压缩兜底 / L4 熔断），本 spec 管「工具自身设计」（每个工具自带返回上限意识）。

## Goal

- 要解决什么问题：逐工具审计 pulsar-app 所有 AI 可调用工具，找出所有可能撑爆上下文的返回路径，并为每个工具补上「返回大小自约束」，使「单条工具结果超限」从设计上不可能发生。
- 验收结果：审计清单全部闭环——高风险项修复 + 回归测试；中风险项补截断；审计结论回写本 spec；`cargo test --lib` 全绿。

## Done Contract

- 什么算完成：高风险 4 项（git_blame / read_file / grep / neuron 网络类）全部收敛为「有明确上限的返回」；中风险 3 项（grep context / git_diff / neuron 读写）补截断或参数 clamp；工具结果统一上限 `tool_result_max_chars` 在发送链路强制生效。
- 由什么证明：`cargo test --lib` 新增审计单测全绿；逐工具复查确认无「无上限返回路径」；主 spec 三层防线配合后，单条超限不可能进 wire。
- 哪些情况仍算未完成：任何工具仍存在「返回大小不设限」的分支；工具结果仍可绕过统一截断点进入 wire。

## Scope

- In:
  - `fileops/fs.rs`：grep 匹配行/context 上限、read_file limit 模式字节上限
  - `fileops/gitops/repo.rs`：git_blame 结果行数上限、git_diff 结果体量上限
  - `core/neuron/tools.rs` + `store.rs`：get_network / select_neuron_candidates 结果量上限与参数 clamp
  - `core/round_executor.rs` / `core/model_call_input.rs` / `core/conversation_runner.rs`：统一工具结果上限在发送链路落地（与主 spec L1 同一入口）
  - 各工具单元测试
- Out:
  - 跨会话持久记忆（Mem0 类，独立迭代）
  - 模型侧 server-side compaction
  - Poller 调度机制重构
  - 非 pulsar-app 链路、前端视觉改动

## Facts / Constraints（审计结论）

### 工具全量审计清单

| 工具 | 实现位置 | 返回类型 | 现状大小控制 | 风险等级 |
|---|---|---|---|---|
| grep | fs.rs:546-638 | Vec\<GrepMatch\> | MAX_GREP_MATCHES=2000 只限条数；**匹配行无长度上限**（单行 3MB 整行返回，事故根因）；context 参数无上限（2000 条 × 每边 N 行膨胀） | **高** |
| read_file | fs.rs:237-308 | FsReadResult | MAX_READ_BYTES=64MB 硬拒；默认 256KB 截断；**但传 limit 走行切片不查字节**（fs.rs:275-294），大 limit 可拉回近 64MB | **高** |
| git_blame | repo.rs:823-841 | Vec\<GitBlameLine\> | **故意绕过 64KB 截断**用完整 stdout 解析（repo.rs:831-832 注释），parse_blame 无行数上限，大文件结果可达数 MB | **高** |
| get_network | neuron/tools.rs:206-243, store.rs:1018-1063 | JSON 网络 | BFS 无节点数量上限，max_depth 无硬 cap | **中-高** |
| select_neuron_candidates | neuron/tools.rs:293-342 | 候选 JSON | n 参数 schema 仅 minimum 0，无上限 | **中-高** |
| git_diff | repo.rs:750-763, parse_diff 350-438 | GitDiff JSON | 64KB 截断 + MAX_DIFF_FILES=200/HUNKS=500/LINES=20000 | 中 |
| get_neuron / update_neuron | neuron/tools.rs:59-101,154-204 | neuron JSON | 无显式截断（content 无硬上限） | 中 |
| list_directory | fs.rs:184-233 | Vec\<FsEntry\> | 无条目数上限（单层目录） | 低-中 |
| glob | fs.rs:501-544 | Vec\<FsMatch\> | MAX_GLOB_RESULTS=1000 | 低-中 |
| execute_command / CommandTool / HttpTool | cmd_exec.rs / dynamic_tool.rs | JSON | 各 64KB 截断 + 超时/并发/denylist | 低-中（有护栏，注意累积） |
| McpTool | mcp.rs:317-331 | String | MAX_RESULT_CHARS=64KB 截断；**无每轮多 server 总量 cap** | 低-中（有护栏，注意累积） |
| 其余 git 只读/写工具、write_file/search_replace/delete/file_info、get_current_time、echo | 各处 | 小 JSON | 各 64KB 截断或输出天然小 | 低 |

### 结构性缺口（全链路核查）

- **无统一落库截断**：`conversation_runner.rs:526-539/806-819` 将 ToolResultItem.content 原样 clone 落库，无截断。
- **无发送前统一 cap**：`model_call_input.rs:227-323` 只做 sanitize_tool_pairs 配对清理，不截断内容。
- **执行拼接原样追加**：`round_executor.rs:296-364` 每个工具结果原样放入 ToolResultItem 并追加 output。
- **压缩只覆盖旧消息**：compaction hook 摘要旧消息，不截断单条工具结果——一轮内一条超大结果仍整条进 wire。
- **多工具累积无总量 cap**：单轮 4 个并行 execute_command / 多 MCP server 各 64KB 可累积到数百 KB。

### 技术/业务约束

- 工具返回要保留「结构化信息」（行号/列号/路径/超时标记等），截断只作用于内容正文。
- 截断提示要引导模型「用更精确参数重试」（社区 OpenClaw/lite_agent 经验），不能静默丢内容。
- 所有上限必须可配置（config.json context 节），默认值内置于代码。
- 不破坏既有 376 个测试；改动为增量。

### 已知风险

- 行截断阈值过小会丢关键匹配内容（建议 grep 单行默认 2_000 字符，够代码行，超长行是 minified JS 类产物）。
- read_file limit 模式收紧后，模型显式要求大范围读取会被截断——用截断标记 + total_chars/total_lines 引导继续读。
- neuron 结果量上限需与现有前端分页语义对齐。

## Restated Understanding

- 我理解当前任务是：对 pulsar-app 全部 AI 工具做「返回大小上限」的逐项审计与加固，堵死任何「单条工具结果撑爆上下文」的路径；审计结论落盘本 spec，与主 spec 三层防线合并为一个完整方案。
- 当前核心目标是：高风险 4 项闭环 + 中风险 3 项收敛 + 统一上限在发送链路强制生效。
- 当前边界是：只改 pulsar-app 工具执行层与发送链路；不做持久记忆、不改 poller 调度骨架、不动前端。
- 暂不处理：Mem0 类持久记忆、server-side compaction、跨项目链路。

## 接口契约设计（伪代码）

```rust
// ── A. 配置（复用主 spec 的 ContextSafetyConfig，此处只列工具自约束字段）──
pub struct ToolSafetyConfig {
    pub grep_line_max_chars: usize,          // 默认 2_000：单条匹配行截断
    pub grep_context_max_lines: usize,       // 默认 3：context 每边行数 clamp（原 schema 无上限）
    pub read_limit_max_chars: usize,         // 默认 256_000：read_file 传 limit 模式也受字节上限
    pub blame_max_lines: usize,              // 默认 5_000：git_blame 解析行数上限
    pub network_max_nodes: usize,            // 默认 500：get_network BFS 节点上限
    pub neuron_candidates_max: usize,        // 默认 50：select_neuron_candidates n clamp
}

// ── B. 逐工具加固签名 ──
// grep（fs.rs）
pub fn grep(&self, ..., context: usize, cfg: &ToolSafetyConfig) -> AppResult<Vec<GrepMatch>>;
// context 先 clamp 到 grep_context_max_lines；匹配行 text > grep_line_max_chars 时
// head/tail 截断 + 提示「行过长已截断，原始 N 字符」；total 输出受 MAX_GREP_MATCHES。

// read_file（fs.rs）
pub fn read(..., limit: Option<usize>, cfg: &ToolSafetyConfig) -> AppResult<FsReadResult>;
// limit 模式：先按行切片，再检查累计字符 > read_limit_max_chars → 截断 + truncated=true，
// 保留 total_lines/total_chars 供模型继续读（不再允许一条拉回 64MB）。

// git_blame（repo.rs）
pub async fn blame(&self, repo: &GitRepo, path: &str, cfg: &ToolSafetyConfig) -> AppResult<Vec<GitBlameLine>>;
// parse_blame 后取前 blame_max_lines 行 + truncated 标记；或用 MAX_OUTPUT_CHARS 字节级截断兜底。

// get_network / select_neuron_candidates（neuron）
// BFS 队列到达 network_max_nodes 停止；n clamp 到 [1, neuron_candidates_max]。

// ── C. 统一发送链路兜底（与主 spec L1 同一入口，双保险）──
// round_executor / model_call_input 组装前调用：
pub fn cap_tool_result(tool_name: &str, content: String, cfg: &ContextSafetyConfig) -> String;
// 任何工具结果 > tool_result_max_chars（默认 12_000）→ head(1/3)+提示+tail(其余)。
// 主 spec 在落库点执行，本 spec 在发送链路执行：即使落库点被绕过，wire 层仍兜底。
```

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是——审计清单逐项对应「工具自身返回上限」，与主 spec 的统一防线互补成完整闭环。
- 若否，偏差在哪里：无。
- 是否需要调整本轮目标或范围：审计已完成，方案成形；待用户批准后进入实现。

## Checkpoint Summary

- 当前任务理解：主 spec 是「统一防线」，本 spec 是「工具自身设计」；两者合并后，任何单条工具结果都不可能撑爆上下文。
- 当前核心目标：高风险 4 项 + 中风险 3 项全部收敛，统一上限在发送链路强制生效。
- 当前进度：✅ 全部落地——高风险 4 项（grep 行+context 截断、read_file limit 字节封顶、git_blame 行数上限、neuron 网络/候选上限）、中风险 clamp、统一发送链路兜底 cap_tool_result（round_executor 落库点）；`cargo test --lib` 384 全绿。
- 下一步 1：✅ 已完成（高风险 4 项）。
- 下一步 2：✅ 已完成（cap_tool_result 统一兜底，与主 spec L1/L2 合并）。
- 下一步 3：✅ 已完成（针对性单测 + `cargo test --lib` 回归 384 全绿）。
- 涉及文件 / 模块：`fileops/fs.rs`、`fileops/gitops/repo.rs`、`core/neuron/tools.rs`、`core/neuron/store.rs`、`core/round_executor.rs`、`core/context_safety.rs`（新）、`core/model_call_input.rs`、`core/conversation_runner.rs`、`core/config.rs`
- 风险：截断阈值需校准；neuron 上限需与前端分页语义对齐。
- 验证方式：`cargo test --lib`；对高风险工具写「超大输入 → 返回被截断」的针对性单测。
- Execution Approval: `Approved`

## Change Log

- 2026-08-22 21-22: 全量工具审计完成并落盘本 spec（高风险 4 项 / 中风险 3 项 / 结构性缺口 4 处）；Q2 已定「不落盘」，截断即烧，故统一截断点是唯一防线，必须双保险（落库点 + 发送链路）。
- 2026-08-22 22:xx: 实现落地——grep ignore 对齐 + 行/context 截断、read_file 累计字符封顶、git_blame 5000 行上限、neuron get_network 500 节点 + select_candidates 50 上限、cap_tool_result 统一兜底（round_executor 落库点）。`cargo test --lib` 384 全绿。

## Validation

- Self-check: 审计清单覆盖全部工具注册点（gateway.rs assemble_local_tools + assemble_mcp_progressive + neuron system tools）；每项高风险均有代码行号证据。
- Static checks: ✅ `cargo check --lib` 通过。
- Runtime / Test: ✅ `cargo test --lib` 384 通过（含 cap_tool_result 截断、sanitize 幂等等针对性单测）。
- Human confirmation: 阈值（12K 工具结果 / 2K 行宽 / 5000 blame 行 / 500 网络节点）需实际运行校准。
- 结果汇总：高风险与中风险全部收敛，统一兜底生效。
- 核心目标是否已由证据证明完成：是（代码落地 + 回归通过）。
- 若未完成，当前剩余差距：真实场景阈值校准。
- 剩余风险：阈值校准需实际运行。

## Resume / Handoff

- 当前状态：✅ 审计 + 实现 + 回归全部完成；与主 spec 合并为一个完整方案，已落地。
- 当前卡点：无。
- 下一步唯一动作：实际运行观察工具返回上限与阈值是否合适；如需要调整 `context` 节参数。
- 下一轮核心目标：所有工具返回有明确上限，任何单条工具结果不可能撑爆上下文（已达成，待运行校准）。

## 参考资料（社区经验）

- OpenClaw ToolResultCompactor RFC（工具结果执行时截断 + 落盘 + 提示，预防式）
- lite_agent PR：工具结果长度上限 12K 字符 + head/tail 截断 + 引导更精确重试
- Why Your AI Agent Keeps Losing Context（token 预算、loop breaker、裸截断破坏多步推理）
- 主 spec：`docs/specs/2026-08-22_19-45_context-overflow-hardening.md`
