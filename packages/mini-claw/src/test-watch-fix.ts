import * as fs from 'fs';
import * as path from 'path';

console.log('🧪 测试 tsx watch 排除功能...\n');

// 确保测试目录存在
const testDir = './.mini-claw/test-exclude';
if (!fs.existsSync(testDir)) {
  fs.mkdirSync(testDir, { recursive: true });
}

console.log('✅ 测试脚本运行中...\n');
console.log('📝 这个脚本会在 .mini-claw 文件夹中写入文件来测试排除功能\n');

// 模拟用户输入时写入文件
setInterval(() => {
  const testFile = path.join(testDir, `test-${Date.now()}.txt`);
  fs.writeFileSync(testFile, `测试内容 ${new Date().toISOString()}`);
  console.log(`📝 已写入: ${testFile}`);
}, 2000);

// 保持进程运行
console.log('🔄 进程将持续运行，按 Ctrl+C 退出...\n');
