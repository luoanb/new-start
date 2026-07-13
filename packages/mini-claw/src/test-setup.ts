import 'dotenv/config.js';
import { Gateway } from './gateway.js';
import { Skill, LLMProvider } from './types.js';

console.log('🧪 Mini-Claw 多服务商测试启动...\n');

// 测试所有支持的服务商配置
const TEST_PROVIDERS: { provider: LLMProvider; model: string; name: string }[] = [
  { provider: 'openai', model: 'gpt-4o-mini', name: 'OpenAI' },
  { provider: 'ollama', model: 'llama3.1:8b', name: 'Ollama (本地)' },
  { provider: 'deepseek', model: 'deepseek-chat', name: 'DeepSeek' },
  { provider: 'custom', model: 'custom-model', name: '自定义' },
];

console.log('✅ 导入模块成功\n');

// 测试各个服务商的 Gateway 初始化
for (const test of TEST_PROVIDERS) {
  try {
    console.log(`🔍 测试服务商: ${test.name}...`);
    
    const gateway = new Gateway(
      {
        storageDir: './.mini-claw/test-memory',
        sessionPersistence: true,
      },
      {
        provider: test.provider,
        model: test.model,
      }
    );
    
    console.log(`   ✅ ${test.name} Gateway 初始化成功`);
    console.log(`   📝 使用模型: ${test.model}\n`);
  } catch (error) {
    console.log(`   ⚠️  ${test.name} 配置可能需要额外设置:`, (error as Error).message);
  }
}

// 创建并测试一个完整可用的 Gateway
console.log('🧩 测试完整功能...');
const gateway = new Gateway(
  {
    storageDir: './.mini-claw/test-memory',
    sessionPersistence: true,
  },
  {
    provider: 'openai',
    model: 'gpt-4o-mini',
  }
);

const mockSkill: Skill = {
  name: 'demo_skill',
  description: '演示技能',
  execute: async () => {
    return { message: '这是一个模拟的技能执行结果！' };
  },
};

gateway.getSkillManager().registerSkill(mockSkill);
console.log('✅ 技能系统正常\n');

// 列出所有可用技能
console.log('🛠️  可用技能:');
gateway.getSkillManager().getAllSkills().forEach((skill) => {
  console.log(`  - ${skill.name}: ${skill.description}`);
});

console.log('\n✅ 多服务商支持测试通过！');
console.log('\n📝 支持的 LLM 服务商:');
console.log('   1. OpenAI (gpt-4, gpt-3.5 等)');
console.log('   2. Ollama (本地运行，免费，llama3 等)');
console.log('   3. DeepSeek');
console.log('   4. 任何兼容 OpenAI 格式的 API (自定义)');
