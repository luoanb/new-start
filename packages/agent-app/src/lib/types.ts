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

export type Message = {
  role: "user" | "assistant" | "system";
  content: string;
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
