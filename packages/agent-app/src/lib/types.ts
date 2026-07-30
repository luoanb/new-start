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

export type Message = {
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: number;
  msg_type?: string;
  summary_of?: string[];
  tool_calls?: ToolCall[];
  tool_call_id?: string;
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
  status: string; // "pending" | "done"
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

export type PollerRunState = "running" | "paused";

export type PollerStatus = {
  state: PollerRunState;
  tick_count: number;
  base_interval_ms: number;
  task_count: number;
  pending_trigger: boolean;
};
