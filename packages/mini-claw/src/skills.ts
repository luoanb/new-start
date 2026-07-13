import { Skill } from './types.js';

export class SkillManager {
  private skills: Map<string, Skill> = new Map();

  registerSkill(skill: Skill) {
    this.skills.set(skill.name, skill);
  }

  getSkill(name: string): Skill | undefined {
    return this.skills.get(name);
  }

  getAllSkills(): Skill[] {
    return Array.from(this.skills.values());
  }

  async executeSkill(name: string, params: any): Promise<any> {
    const skill = this.skills.get(name);
    if (!skill) {
      throw new Error(`Skill "${name}" not found`);
    }
    return await skill.execute(params);
  }

  getSkillsDescription(): string {
    const skills = this.getAllSkills();
    if (skills.length === 0) {
      return 'No skills available.';
    }
    return skills.map(skill => `- ${skill.name}: ${skill.description}`).join('\n');
  }
}

export const createTimeSkill: Skill = {
  name: 'get_current_time',
  description: '获取当前时间',
  execute: async () => {
    const now = new Date();
    return {
      timestamp: now.getTime(),
      iso: now.toISOString(),
      local: now.toLocaleString(),
      timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    };
  },
};

export const createCalculatorSkill: Skill = {
  name: 'calculate',
  description: '执行数学计算',
  execute: async (params: { expression: string }) => {
    try {
      const result = Function('"use strict"; return (' + params.expression + ')')();
      return { expression: params.expression, result };
    } catch (err) {
      throw new Error('Invalid expression');
    }
  },
};

export const createEchoSkill: Skill = {
  name: 'echo',
  description: '回显消息',
  execute: async (params: { message: string }) => {
    return { echo: params.message };
  },
};
