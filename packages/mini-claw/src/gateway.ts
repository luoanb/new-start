import { Agent } from './agent.js';
import { Memory } from './memory.js';
import { SkillManager, createTimeSkill, createCalculatorSkill, createEchoSkill } from './skills.js';
import { GatewayConfig, AgentConfig, Message, LLMProvider } from './types.js';

export class Gateway {
  private memory: Memory;
  private agent: Agent;
  private skillManager: SkillManager;
  private currentConversationId: string;

  constructor(
    gatewayConfig: GatewayConfig,
    agentConfig: AgentConfig
  ) {
    this.memory = new Memory(gatewayConfig.storageDir, gatewayConfig.sessionPersistence);
    this.skillManager = new SkillManager();
    this.agent = new Agent(agentConfig, this.skillManager);
    this.currentConversationId = this.memory.createConversation().id;

    this.registerDefaultSkills();
  }

  private registerDefaultSkills() {
    this.skillManager.registerSkill(createTimeSkill);
    this.skillManager.registerSkill(createCalculatorSkill);
    this.skillManager.registerSkill(createEchoSkill);
  }

  switchConversation(conversationId: string) {
    if (!this.memory.getConversation(conversationId)) {
      this.memory.createConversation(conversationId);
    }
    this.currentConversationId = conversationId;
  }

  getCurrentConversationId(): string {
    return this.currentConversationId;
  }

  async sendMessage(userInput: string): Promise<string> {
    const userMessage: Message = {
      role: 'user',
      content: userInput,
      timestamp: Date.now(),
    };

    this.memory.addMessage(this.currentConversationId, userMessage);
    const messages = this.memory.getMessages(this.currentConversationId);
    const response = await this.agent.process(messages);

    const assistantMessage: Message = {
      role: 'assistant',
      content: response,
      timestamp: Date.now(),
    };

    this.memory.addMessage(this.currentConversationId, assistantMessage);
    return response;
  }

  getConversationHistory(): Message[] {
    return this.memory.getMessages(this.currentConversationId);
  }

  clearCurrentConversation() {
    this.memory.clearConversation(this.currentConversationId);
    this.currentConversationId = this.memory.createConversation().id;
  }

  getAllConversations() {
    return this.memory.getAllConversations();
  }

  getSkillManager(): SkillManager {
    return this.skillManager;
  }
}
