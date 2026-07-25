import OpenAI from 'openai';
import type { ChatCompletionMessageParam } from 'openai/resources/chat/completions.js';
import { AgentConfig, Message, LLMProvider } from './types.js';
import { SkillManager } from './skills.js';

// 预设的服务商配置
const PROVIDER_CONFIGS: Record<LLMProvider, { baseURL?: string; apiKeyRequired: boolean }> = {
  openai: {
    baseURL: undefined, // 使用 OpenAI 默认
    apiKeyRequired: true,
  },
  ollama: {
    baseURL: 'http://localhost:11434/v1',
    apiKeyRequired: false,
  },
  deepseek: {
    baseURL: 'https://api.deepseek.com/v1',
    apiKeyRequired: true,
  },
  custom: {
    baseURL: undefined,
    apiKeyRequired: false,
  },
};

export class Agent {
  private client: OpenAI;
  private config: AgentConfig;
  private skillManager: SkillManager;

  constructor(config: AgentConfig, skillManager: SkillManager) {
    this.config = config;
    this.skillManager = skillManager;

    // 根据服务商配置初始化 OpenAI 客户端
    const providerConfig = PROVIDER_CONFIGS[config.provider];

    this.client = new OpenAI({
      apiKey: config.apiKey || process.env.OPENAI_API_KEY || 'dummy-key',
      baseURL: config.baseURL || providerConfig.baseURL,
      dangerouslyAllowBrowser: true,
    });
  }

  private buildSystemPrompt(): string {
    const skillsDesc = this.skillManager.getSkillsDescription();
    return (
      this.config.systemPrompt ||
      `你是一个有用的 AI 助手。你可以使用以下技能来帮助用户：

${skillsDesc}

请根据用户的请求，决定是否需要使用技能。`
    );
  }

  private toApiMessages(messages: Message[]): ChatCompletionMessageParam[] {
    const systemMessage = { role: 'system' as const, content: this.buildSystemPrompt() };
    return [
      systemMessage,
      ...messages.map((m) => ({
        role: m.role as 'user' | 'assistant',
        content: m.content,
      })),
    ];
  }

  async process(messages: Message[]): Promise<string> {
    try {
      const apiMessages = this.toApiMessages(messages);
      const tools = this.skillManager.toTools();
      const toolEnabled = tools.length > 0;

      // runningMessages 不断累积，支持多轮 tool_calls 循环
      const runningMessages: ChatCompletionMessageParam[] = [...apiMessages];

      // eslint-disable-next-line no-constant-condition
      while (true) {
        const response = await this.client.chat.completions.create({
          model: this.config.model,
          messages: runningMessages,
          tools: toolEnabled ? tools : undefined,
          tool_choice: toolEnabled ? 'auto' : undefined,
          max_tokens: this.config.maxTokens || 1000,
          temperature: this.config.temperature || 0.7,
        });

        const msg = response.choices[0].message;

        // 模型没调工具 → 本轮结束
        if (!msg.tool_calls || msg.tool_calls.length === 0) {
          return msg.content || '';
        }

        // 模型调了工具 → 并行执行
        runningMessages.push(msg as ChatCompletionMessageParam);

        const toolResults = await Promise.all(
          msg.tool_calls.map(async (tc) => {
            const args = JSON.parse(tc.function.arguments);
            const result = await this.skillManager.executeSkill(tc.function.name, args);
            return {
              role: 'tool' as const,
              tool_call_id: tc.id,
              content: JSON.stringify(result),
            };
          })
        );

        runningMessages.push(...toolResults);
        // 继续循环，直到模型返回纯文本
      }
    } catch (error) {
      console.error('Agent error:', error);
      return `抱歉，处理请求时出错了：${error instanceof Error ? error.message : '未知错误'}`;
    }
  }
}
