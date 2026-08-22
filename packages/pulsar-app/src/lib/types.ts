export type AppError = {
  code: string;
  message: string;
};

export type ChatResponse = {
  conversation_id: string;
  response: string;
};

export type Conversation = {
  id: string;
  mode: "chat" | "agent" | "assistant" | "system";
  messages: Message[];
  created_at: number;
  updated_at: number;
  /** 会话级扩展（后端 skip_serializing_if=None）：assistant 模式承载会话运行态。 */
  extra?: {
    session?: {
      state?: {
        last_selected_neuron_id?: string | null;
        /** 会话级模型选择（后端持有，前端切换会话回显；None = 未指定回退全局默认）。 */
        model?: ChatModelSelection | null;
      };
    };
  } | null;
};

export type ToolCall = {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
};

export type MessageRole = "user" | "assistant" | "system" | "tool" | "compaction";

export type MessageBody =
  | {
      kind: "text";
      content: string;
      /** 推理模型的思考链（wire `reasoning_content` 同源投影；无思考时缺失）。 */
      reasoning?: string;
      /** 模型声明的一次性工具调用（wire 平级字段；存量 `kind:"tool_call"` 数据反序列化并入）。 */
      tool_calls?: ToolCall[];
    }
  | { kind: "tool_result"; tool_call_id: string; tool_name: string; content: string }
  | { kind: "compaction"; summary_of: string[]; content: string }
  | { kind: "nudge"; content: string }
  | { kind: "role_context"; content: string };

export type Message = {
  role: MessageRole;
  body: MessageBody;
  timestamp: number;
  /** 所属神经元（assistant 模式每轮选中，落库盖章；旧消息 / 非 assistant 模式缺失）。 */
  neuron_id?: string | null;
};

export type RuntimeStatus = {
  app_name: string;
  storage_path: string;
  current_conversation_id: string;
  skill_count: number;
  conversation_count: number;
};

export type SkillInfo = {
  name: string;
  description: string;
};

// ── Tools（运行阶段三通道：native / config / mcp）──

export type ToolSource = "native" | "config" | "mcp";

/** 工具标签（用途/行为维度）：normal 由神经元管理（默认）／system 系统模式会话自动带／core 任何对话都带。 */
export type ToolTag = "normal" | "system" | "core";

export type ToolInfo = {
  name: string;
  description: string;
  source: ToolSource;
  tag: ToolTag;
  parameters: Record<string, unknown>;
};

export type McpServerStatusKind = "connecting" | "connected" | "failed" | "disabled";

export type McpServerStatus = {
  name: string;
  transport: string;
  status: McpServerStatusKind;
  tool_count: number;
  error: string | null;
};

// ── 工具配置（弹窗编辑 / 写回 JSON，字段与后端 serde 保持一致）──

export type McpServerConfig = {
  name: string;
  transport: "stdio" | "http";
  command?: string | null;
  args?: string[];
  env?: Record<string, string>;
  url?: string | null;
  headers?: Record<string, string>;
  disabled?: boolean;
  /** 该 server 下全部工具打此标，缺省 normal。 */
  tag?: ToolTag;
};

export type HttpToolConfig = {
  name: string;
  desc: string;
  method?: string;
  url: string;
  timeout_ms?: number | null;
  tag?: ToolTag;
};

export type CommandToolConfig = {
  name: string;
  desc: string;
  template: string;
  timeout_ms?: number | null;
  tag?: ToolTag;
};

export type ToolConfigView = {
  mcp_servers: McpServerConfig[];
  http_tools: HttpToolConfig[];
  command_tools: CommandToolConfig[];
};

export type ProviderInfo = {
  id: string;
  display_name: string;
  api_base: string | null;
  auth_env: string;
  kind: "open_ai" | "open_ai_compatible";
};

export type ModelCapabilities = {
  chat: boolean;
  tools: boolean;
  streaming: boolean;
  structured_output?: boolean;
  vision?: boolean;
  extras?: Record<string, string>;
};

/** 采样参数（统一规范，对齐后端 SamplingParams）：全字段可选，逐级合并。 */
export type SamplingParams = {
  temperature?: number;
  top_p?: number;
  /** 单次请求输出上限；覆盖模型定义 max_output_tokens。 */
  max_tokens?: number;
  presence_penalty?: number;
  frequency_penalty?: number;
  stop?: string[];
  seed?: number;
};

export type ThinkingEffort = "low" | "high" | "max";

/** 思考模式（深度思考）配置（对齐后端 ThinkingConfig）。 */
export type ThinkingConfig = {
  enabled?: boolean;
  effort?: ThinkingEffort;
};

/** 模型思考能力声明（对齐后端 ThinkingCapability）。 */
export type ThinkingCapability = {
  supported: boolean;
  default_enabled?: boolean;
  default_effort?: ThinkingEffort;
};

/** 会话级 / 调用级统一模型选择（对齐后端 ChatModelSelection）。 */
export type ChatModelSelection = {
  provider_id: string;
  model_id: string;
  params?: SamplingParams;
  thinking?: ThinkingConfig;
};

export type ModelInfo = {
  id: string;
  provider_id: string;
  display_name: string;
  capabilities: ModelCapabilities;
  context_window?: number;
  max_output_tokens?: number;
  /** 模型定义级默认采样参数（作为会话覆盖的底层默认）。 */
  sampling?: SamplingParams;
  /** 模型思考模式能力 + 默认。 */
  thinking?: ThinkingCapability;
  pricing_input?: number;
  pricing_output?: number;
};

export type ModelCallResponse = {
  provider_id: string;
  model_id: string;
  output: string;
};

// ── Provider / Model 管理视图（get_provider_config / save_provider_config 载荷）──

export type ProviderKind = "open_ai" | "open_ai_compatible";

export type ProviderDefaults = {
  provider: string;
  model: string;
};

export type ModelEditInfo = {
  id: string;
  display_name?: string | null;
  capabilities: ModelCapabilities;
  context_window?: number | null;
  max_output_tokens?: number | null;
  sampling?: SamplingParams | null;
  thinking?: ThinkingCapability | null;
  pricing_input?: number | null;
  pricing_output?: number | null;
  pricing_cache_input?: number | null;
  knowledge_cutoff?: string | null;
};

export type ProviderEditInfo = {
  id: string;
  display_name?: string | null;
  kind: ProviderKind;
  api_base?: string | null;
  /** 掩码回显；提交时与掩码相同视为未修改（保留原值）。 */
  api_key?: string | null;
  /** 是否已配置 API Key（env 或 config），仅回显用。 */
  api_key_set: boolean;
  auth_env?: string | null;
  enabled: boolean;
  builtin: boolean;
  models: ModelEditInfo[];
};

export type ProviderConfigView = {
  defaults?: ProviderDefaults | null;
  providers: ProviderEditInfo[];
};

// ── Topic / Poller ──

export type TopicStatus =
  | "todo"
  | "in_progress"
  | "paused"
  | "done"
  | "cancelled"
  | "waiting_user"
  | "wrapping_up";

export type ScopeInItem = {
  id: string;
  goal: string;
  done_contract: string;
  status: string; // "pending" | "completed" | "blocked"
};

export type Topic = {
  id: string;
  name: string;
  status: TopicStatus;
  description: string;
  scope_in: ScopeInItem[];
  progress: number;
  session_id?: string | null;
  extra?: Record<string, unknown> | null;
  created_at: number;
  updated_at: number;
};

export type Neuron = {
  id: string;
  desc: string;
  content: string;
  weight: number;
  system_type?: string | null;
  tool_ids: string[];
  created_at: number;
  updated_at: number;
  use_count?: number;
  last_used_at?: number | null;
  deleted_at?: number | null;
  /** 系统神经元行为（仅 `system_type` 非空的神经元可挂载；后端缺失回落 null）。 */
  behavior?: SessionBehavior | null;
};

/** 管理面分页结果（list_neurons_page 返回）。 */
export type NeuronPage = {
  items: Neuron[];
  total: number;
  has_more: boolean;
};

export type Connection = {
  source: string;
  target: string;
  weight: number;
};

export type NeuronSubgraph = {
  seed_id: string;
  neurons: Neuron[];
  connections: Connection[];
};

export type CreateNeuronPlainInput = {
  desc: string;
  content?: string;
  link_to?: string | null;
  tool_ids?: string[];
};

export type PollerRunState = "running" | "paused";

export type PollerStatus = {
  state: PollerRunState;
  tick_count: number;
  base_interval_ms: number;
  task_count: number;
  pending_trigger: boolean;
  assistant_poll_parallelism: number;
};

export type LogLevel = "error" | "warn" | "info" | "debug" | "trace";

export type LogEntry = {
  ts_ms: number;
  level: LogLevel;
  target: string;
  message: string;
  fields?: Record<string, string>;
};

// ── Hook Judgements（裁决记录，与后端 hook_judgement_store.rs serde 一致）──

/** 裁决终态（后端 JudgementStatus 序列化同构；pending 为过程态）。 */
export type HookJudgementStatus = "pending" | "ok" | "retried_ok" | "downgraded";

export type HookJudgementRecord = {
  id: string;
  session_id?: string | null;
  conversation_id: string;
  /** 锚点消息索引（消息列表裁决卡挂载位置）；未绑定消息为 null。 */
  anchor_message_index?: number | null;
  /** system_type（如 `assistant_complete_scope`）。 */
  hook_type: string;
  status: HookJudgementStatus;
  /** 尝试次数（1 或 2）。 */
  attempts: number;
  /** 每轮尝试明细（JSON 数组字符串：`[{attempt, raw, error}]`）。 */
  attempts_detail: string;
  /** 用户侧裁决输入（JSON 字符串）。 */
  payload: string;
  /** 最终轮模型原始输出（全文）。 */
  raw_response: string;
  /** 解析出的 JSON 决策（成功时；降级时为空串）。 */
  decision?: string | null;
  /** 失败/降级原因摘要。 */
  error?: string | null;
  /** 总耗时（含重试），毫秒。 */
  duration_ms: number;
  model_provider?: string | null;
  model_id?: string | null;
  created_at: number;
  updated_at: number;
};

/** `hook_judgements_list` 过滤入参（后端 HookJudgementFilter camelCase 同构）。 */
export type HookJudgementFilter = {
  hookType?: string;
  status?: string;
  conversationId?: string;
  limit?: number;
  offset?: number;
};

/** `hook_defs_list` 出参：hook 元信息（面板过滤下拉数据源）。 */
export type HookDefMeta = {
  system_type: string;
  label: string;
};

// ── Running Sessions ──

export type RunningSession = {
  session_id: string;
  started_at: number;
  current_step: string | null;
};

// ── 系统神经元行为（system neuron behavior 管理，字段与后端 models.rs serde 一致）──

/** 邻域池策略（对齐 NeighborhoodPoolPolicy）。 */
export type NeighborhoodPoolPolicy = {
  existing_downstream: number;
  new_downstream: number;
  fill_downstream_shortage: boolean;
  siblings: number;
  upstream_depth: number;
  global_top_weight: number;
};

/** 选型策略：Rust externally-tagged enum 的 JSON 形态（宽容解析兼容旧字段）。
 * - "None" 不取提示词；"Fixed" 读系统神经元自己 content；
 * - Neighborhood 邻域选 1；Global 无历史全域选 1、有历史退化为邻域。 */
export type SelectionPolicy =
  | "None"
  | "Fixed"
  | { Neighborhood: { policy: NeighborhoodPoolPolicy } }
  | { Global: { limit: number } };

/** 工具授权策略：None 不授权 / FromNeuron 取角色神经元 tool_ids / Allowlist 显式白名单。 */
export type ToolPolicy =
  | "None"
  | "FromNeuron"
  | { Allowlist: string[] };

/** 系统神经元行为（承载于 session.% 系统神经元的 behavior 列）。 */
export type SessionBehavior = {
  selection: SelectionPolicy;
  tools: ToolPolicy;
  insert_id?: string | null;
};

/** 系统神经元状态摘要（list_session_specs / create_session_spec 返回）。 */
export type SystemPromptStatus = {
  system_type: string;
  neuron_id?: string | null;
  behavior?: SessionBehavior | null;
};

// ── 工作区 / 文件管理（对齐后端 core/workspace.rs 与 core/fs.rs DTO）──

export type WorkspaceEntry = {
  id: string;
  name: string;
  /** 规范化后的绝对路径。 */
  root: string;
  /** 该工作区文件树过滤规则（glob/前缀语义）。 */
  ignore: string[];
  created_at: number;
};

export type WorkspaceView = {
  workspaces: WorkspaceEntry[];
  active_id: string | null;
};

export type FsEntry = {
  name: string;
  /** 相对 workspace 根（`/` 分隔，无前导斜杠）。 */
  path: string;
  is_dir: boolean;
  size: number | null;
  modified_ms: number | null;
};

export type FsReadResult = {
  /** 分段读取的行内容（offset/limit 截断）。 */
  content: string;
  total_lines: number;
  total_chars: number;
  /** 读取时刻的文件 mtime（保存冲突检测的 base_mtime）。 */
  mtime_ms: number;
  truncated: boolean;
};

export type FsWriteResult = {
  mtime_ms: number;
};

export type FsMatch = {
  path: string;
  modified_ms: number;
};

export type GrepMatch = {
  path: string;
  /** 1-based 行号。 */
  line: number;
  /** 行内列偏移（0-based）。 */
  column: number;
  text: string;
  context_before?: string[];
  context_after?: string[];
};

export type FsInfo = {
  exists: boolean;
  is_dir: boolean;
  size: number;
  modified_ms: number | null;
  is_binary: boolean;
};

// ── Git（对齐后端 fileops/gitops/mod.rs DTO）──

export type GitRepo = {
  /** 稳定 id：由 canonicalized repo 根派生。 */
  id: string;
  name: string;
  root: string;
  /** 是否为嵌套 repo。 */
  is_nested: boolean;
};

export type GitStatusEntry = {
  /** 相对 repo 根（`/` 分隔）。 */
  path: string;
  /** M/A/D/R/?/U 等单字母或组合（如 "MM"）。 */
  status: string;
  is_dir: boolean;
};

export type GitStatusView = {
  branch: string | null;
  ahead: number;
  behind: number;
  staged: GitStatusEntry[];
  unstaged: GitStatusEntry[];
  untracked: GitStatusEntry[];
  conflicted: GitStatusEntry[];
};

export type GitDiffLineKind = "context" | "add" | "del";

export type GitDiffLine = {
  kind: GitDiffLineKind;
  old_no: number | null;
  new_no: number | null;
  text: string;
};

export type GitHunk = {
  old_start: number;
  old_lines: number;
  new_start: number;
  new_lines: number;
  /** 原始头 `@@ -a,b +c,d @@ ctx`。 */
  header: string;
  lines: GitDiffLine[];
};

export type GitFileDiff = {
  path: string;
  status: string;
  is_binary: boolean;
  hunks: GitHunk[];
};

export type GitDiff = {
  files: GitFileDiff[];
  /** 输出超限被截断。 */
  truncated: boolean;
};

export type GitCommitInfo = {
  hash: string;
  short: string;
  author: string;
  date: string;
  subject: string;
};

/** 某提交中单个变更文件的统计（`git show --numstat`）。 */
export type GitShowFile = {
  path: string;
  additions: number;
  deletions: number;
  /** 二进制 / LFS 指针文件 → 不渲染 diff 正文。 */
  is_binary: boolean;
};

export type GitBlameLine = {
  line_no: number;
  short: string;
  author: string;
  date: string;
  text: string;
};

export type GitStashEntry = {
  /** stash@{n} 的 n。 */
  index: number;
  message: string;
};

export type GitBranchItem = {
  name: string;
  current: boolean;
  upstream: string | null;
};

export type GitResetMode = "mixed" | "soft" | "hard" | "keep";

export type GitResetPreview = {
  /** hard 场景将丢失改动文件清单。 */
  lost: string[];
};

export type GitStashAction = "push" | "pop" | "drop" | "apply";

export type ConflictTake = "ours" | "theirs" | "both";

/** git 确认弹窗载荷（git_confirm 事件消费后 resolve）。 */
export type GitConfirmRequest = {
  op_id: string;
  kind: string;
  title: string;
  detail: unknown;
};

/** git 面板聚合视图（dataStore 单一数据源）。 */
export type GitView = {
  repos: GitRepo[];
  activeRepoId: string | null;
  /** 每个 repo 的状态（文件树徽标按文件归属 repo 取数；面板用 active repo 的 status）。 */
  statusByRepo: Record<string, GitStatusView | null>;
  status: GitStatusView | null;
  branches: GitBranchItem[];
  log: GitCommitInfo[];
  stash: GitStashEntry[];
  confirmConfig: { dangerous_writes: boolean };
};

/** git 写操作确认结果。 */
export type GitConfirmResult =
  | { approved: true }
  | { approved: false; reason: "rejected" | "timed_out" | "error"; message?: string };
