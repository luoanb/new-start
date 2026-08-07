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
  | { kind: "compaction"; summary_of: string[]; content: string };

export type Message = {
  role: MessageRole;
  body: MessageBody;
  timestamp: number;
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

export type McpServerStatusKind = "connected" | "failed" | "disabled";

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
