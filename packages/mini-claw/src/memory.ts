import { Conversation, Message } from './types.js';
import * as fs from 'fs';
import * as path from 'path';

export class Memory {
  private conversations: Map<string, Conversation> = new Map();
  private storageDir: string;
  private persistenceEnabled: boolean;

  constructor(storageDir: string = './.mini-claw/memory', persistenceEnabled: boolean = true) {
    this.storageDir = storageDir;
    this.persistenceEnabled = persistenceEnabled;
    this.ensureStorageDir();
    this.loadFromDisk();
  }

  private ensureStorageDir() {
    if (!fs.existsSync(this.storageDir)) {
      fs.mkdirSync(this.storageDir, { recursive: true });
    }
  }

  private getConversationPath(conversationId: string): string {
    return path.join(this.storageDir, `${conversationId}.json`);
  }

  private loadFromDisk() {
    if (!this.persistenceEnabled) return;

    try {
      if (!fs.existsSync(this.storageDir)) return;

      const files = fs.readdirSync(this.storageDir);
      for (const file of files) {
        if (file.endsWith('.json')) {
          const filePath = path.join(this.storageDir, file);
          try {
            const data = fs.readFileSync(filePath, 'utf-8');
            const conversation = JSON.parse(data) as Conversation;
            this.conversations.set(conversation.id, conversation);
          } catch (err) {
            console.error(`Failed to load conversation ${file}:`, err);
          }
        }
      }
    } catch (err) {
      console.error('Failed to load memory from disk:', err);
    }
  }

  private saveToDisk(conversation: Conversation) {
    if (!this.persistenceEnabled) return;

    try {
      const filePath = this.getConversationPath(conversation.id);
      fs.writeFileSync(filePath, JSON.stringify(conversation, null, 2));
    } catch (err) {
      console.error('Failed to save memory to disk:', err);
    }
  }

  createConversation(conversationId?: string): Conversation {
    const id = conversationId || `conv_${Date.now()}`;
    const conversation: Conversation = {
      id,
      messages: [],
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };
    this.conversations.set(id, conversation);
    this.saveToDisk(conversation);
    return conversation;
  }

  getConversation(conversationId: string): Conversation | undefined {
    return this.conversations.get(conversationId);
  }

  addMessage(conversationId: string, message: Message) {
    let conversation = this.conversations.get(conversationId);
    if (!conversation) {
      conversation = this.createConversation(conversationId);
    }
    conversation.messages.push({
      ...message,
      timestamp: message.timestamp || Date.now(),
    });
    conversation.updatedAt = Date.now();
    this.conversations.set(conversationId, conversation);
    this.saveToDisk(conversation);
  }

  getMessages(conversationId: string): Message[] {
    const conversation = this.conversations.get(conversationId);
    return conversation?.messages || [];
  }

  clearConversation(conversationId: string) {
    this.conversations.delete(conversationId);
    if (this.persistenceEnabled) {
      const filePath = this.getConversationPath(conversationId);
      if (fs.existsSync(filePath)) {
        fs.unlinkSync(filePath);
      }
    }
  }

  getAllConversations(): Conversation[] {
    return Array.from(this.conversations.values());
  }
}
