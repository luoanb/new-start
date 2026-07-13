import OpenAI from 'openai';
import { AgentConfig, Message, Skill, LLMProvider } from './types.js';
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

请根据用户的请求，决定是否需要使用技能。如果需要使用技能，请以 JSON 格式返回：
{
  "action": "use_skill",
  "skill": "技能名称",
  "params": { ... }
}

如果不需要使用技能，请直接回答用户的问题。`
    );
  }

  private async generateResponse(messages: Message[]): Promise<string> {
    const systemMessage: Message = {
      role: 'system',
      content: this.buildSystemPrompt(),
      timestamp: Date.now(),
    };

    const allMessages = [
      systemMessage,
      ...messages.map((m) => ({
        role: m.role,
        content: m.content,
      })),
    ];

    const response = await this.client.chat.completions.create({
      model: this.config.model,
      messages: allMessages as any,
      max_tokens: this.config.maxTokens || 1000,
      temperature: this.config.temperature || 0.7,
    });

    return response.choices[0].message.content || '';
  }

  private parseSkillCall(content: string): { skill: string; params: any } | null {
    try {
      const match = content.match(/\{[\s\S]*\}/);
      if (match) {
        const parsed = JSON.parse(match[0]);
        if (parsed.action === 'use_skill' && parsed.skill) {
          return { skill: parsed.skill, params: parsed.params || {} };
        }
      }
    } catch {
      // Not a valid JSON skill call
    }
    return null;
  }

  async process(messages: Message[]): Promise<string> {
    try {
      let response = await this.generateResponse(messages);
      const skillCall = this.parseSkillCall(response);

      if (skillCall) {
        const skillResult = await this.skillManager.executeSkill(
          skillCall.skill,
          skillCall.params
        );

        const followUpMessages = [
          ...messages,
          { role: 'assistant' as const, content: response, timestamp: Date.now() },
          {
            role: 'user' as const,
            content: `技能执行结果：\n${JSON.stringify(skillResult, null, 2)}\n\n请根据结果给用户一个友好的回答。`,
            timestamp: Date.now(),
          },
        ];

        response = await this.generateResponse(followUpMessages);
      }

      return response;
    } catch (error) {
      console.error('Agent error:', error);
      return `抱歉，处理请求时出错了：${error instanceof Error ? error.message : '未知错误'}`;
    }
  }
}
