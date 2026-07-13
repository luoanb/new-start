export interface Message {
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: number;
}

export interface Conversation {
  id: string;
  messages: Message[];
  createdAt: number;
  updatedAt: number;
}

export interface Skill {
  name: string;
  description: string;
  execute: (params: any) => Promise<any>;
}

export type LLMProvider = 'openai' | 'ollama' | 'deepseek' | 'custom';

export interface AgentConfig {
  provider: LLMProvider;
  model: string;
  apiKey?: string;
  baseURL?: string;
  systemPrompt?: string;
  maxTokens?: number;
  temperature?: number;
}

export interface GatewayConfig {
  storageDir: string;
  sessionPersistence: boolean;
}
