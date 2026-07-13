import { config as dotenvConfig } from 'dotenv';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';
import * as readline from 'readline/promises';
import { stdin as input, stdout as output } from 'process';
import chalk from 'chalk';
import { Gateway } from './gateway.js';
import { LLMProvider } from './types.js';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Load .env from workspace root (pnpm workspace exec from package dir)
dotenvConfig({ path: resolve(__dirname, '../..', '.env') });
// Fallback: .env relative to CWD (direct package execution)
dotenvConfig();

const rl = readline.createInterface({ input, output });

console.log(chalk.cyan.bold('\n🦞 Mini-Claw - 简化版 OpenClaw 风格智能体\n'));

// 从环境变量读取配置
const provider = (process.env.LLM_PROVIDER as LLMProvider) || 'openai';
const model = process.env.LLM_MODEL || (provider === 'openai' ? 'gpt-4o-mini' : provider === 'ollama' ? 'llama3.1:8b' : 'deepseek-chat');
const apiKey = process.env.LLM_API_KEY;
const baseURL = process.env.LLM_BASE_URL;

console.log(chalk.gray(`使用服务商: ${chalk.yellow(provider)} | 模型: ${chalk.yellow(model)}\n`));
console.log(chalk.gray('输入你的问题，或输入 /help 查看帮助，输入 /exit 退出\n'));

const gateway = new Gateway(
  {
    storageDir: './.mini-claw/memory',
    sessionPersistence: true,
  },
  {
    provider,
    model,
    apiKey,
    baseURL,
    systemPrompt: `你是一个有用的 AI 助手。你可以使用以下技能来帮助用户：

- get_current_time: 获取当前时间
- calculate: 执行数学计算
- echo: 回显消息

请根据用户的请求，决定是否需要使用技能。如果需要使用技能，请以 JSON 格式返回：
{
  "action": "use_skill",
  "skill": "技能名称",
  "params": { ... }
}

如果不需要使用技能，请直接回答用户的问题。`,
    temperature: 0.7,
    maxTokens: 1000,
  }
);

async function main() {
  // 显示当前会话信息
  const showSessionInfo = () => {
    const convId = gateway.getCurrentConversationId();
    const history = gateway.getConversationHistory();
    console.log(chalk.gray(`💾 会话 ID: ${convId.substring(0, 12)}... | 历史消息: ${history.length} 条\n`));
  };
  
  showSessionInfo();
  
  while (true) {
    const userInput = await rl.question(chalk.green('👤 你: '));

    if (userInput.trim() === '/exit') {
      console.log(chalk.yellow('👋 再见！'));
      break;
    }

    if (userInput.trim() === '/help') {
      console.log(chalk.blue('📖 帮助:'));
      console.log(chalk.white('  /help    - 显示帮助信息'));
      console.log(chalk.white('  /clear   - 清空当前会话'));
      console.log(chalk.white('  /skills  - 列出所有可用技能'));
      console.log(chalk.white('  /history - 查看会话历史'));
      console.log(chalk.white('  /info    - 显示当前会话信息'));
      console.log(chalk.white('  /exit    - 退出程序\n'));
      continue;
    }

    if (userInput.trim() === '/info') {
      showSessionInfo();
      continue;
    }

    if (userInput.trim() === '/clear') {
      gateway.clearCurrentConversation();
      console.log(chalk.yellow('✨ 会话已清空\n'));
      showSessionInfo();
      continue;
    }

    if (userInput.trim() === '/skills') {
      const skills = gateway.getSkillManager().getAllSkills();
      console.log(chalk.blue('🛠️  可用技能:'));
      skills.forEach((skill) => {
        console.log(chalk.white(`  - ${skill.name}: ${skill.description}`));
      });
      console.log('');
      continue;
    }

    if (userInput.trim() === '/history') {
      const history = gateway.getConversationHistory();
      console.log(chalk.blue('📜 会话历史:'));
      if (history.length === 0) {
        console.log(chalk.gray('  (暂无历史消息)\n'));
      } else {
        history.forEach((msg, i) => {
          const role = msg.role === 'user' ? '👤' : '🤖';
          const time = new Date(msg.timestamp).toLocaleTimeString();
          console.log(chalk.gray(`${i + 1}. [${time}] [${role}] ${msg.content}`));
        });
        console.log('');
      }
      continue;
    }

    console.log(chalk.gray('🤖 正在思考...'));
    try {
      const response = await gateway.sendMessage(userInput);
      console.log(chalk.magenta.bold('🤖 Mini-Claw: ') + chalk.white(response + '\n'));
    } catch (error) {
      console.error(chalk.red('❌ 错误:'), error, '\n');
    }
  }

  rl.close();
}

main().catch((error) => {
  console.error(chalk.red('❌ 程序异常退出:'), error);
  process.exit(1);
});
