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
  mode: "chat" | "agent" | "assistant";
  messages: Message[];
  created_at: number;
  updated_at: number;
  /** 会话级扩展（后端 skip_serializing_if=None）：assistant 模式承载会话运行态。 */
  extra?: {
    session?: {
      state?: {
        last_selected_neuron_id?: string | null;
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
  | { kind: "text"; content: string }
  | { kind: "tool_call"; content: string; tool_calls: ToolCall[] }
  | { kind: "tool_result"; tool_call_id: string; tool_name: string; content: string }
  | { kind: "compaction"; summary_of: string[]; content: string }
  | { kind: "nudge"; content: string };

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

export type ToolInfo = {
  name: string;
  description: string;
  source: ToolSource;
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
};

export type HttpToolConfig = {
  name: string;
  desc: string;
  method?: string;
  url: string;
  timeout_ms?: number | null;
};

export type CommandToolConfig = {
  name: string;
  desc: string;
  template: string;
  timeout_ms?: number | null;
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

export type ModelInfo = {
  id: string;
  provider_id: string;
  display_name: string;
  capabilities: ModelCapabilities;
  context_window?: number;
  max_output_tokens?: number;
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

export type TopicStatus = "todo" | "in_progress" | "paused" | "done" | "cancelled";

export type ScopeInItem = {
  id: string;
  goal: string;
  done_contract: string;
  status: string; // "pending" | "completed"
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
  /** 会话规格（仅 `system_type` 非空的神经元可挂载；后端缺失回落 null）。 */
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

// ── Running Sessions ──

export type RunningSession = {
  session_id: string;
  started_at: number;
  current_step: string | null;
};

// ── 会话规格（系统神经元 behavior 管理，字段与后端 models.rs serde 一致）──

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
 * - "None" 不取提示词；"Fixed" 读规格自己 content；
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

/** 会话规格行为（承载于 session.% 系统神经元的 behavior 列）。 */
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
