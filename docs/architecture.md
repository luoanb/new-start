# Mini-Claw 架构设计

## 目录结构

```
.
├── packages/mini-claw/src/
│   ├── index.ts          # CLI 入口
│   ├── gateway.ts        # Gateway 编排器
│   ├── agent.ts          # Agent LLM 交互
│   ├── memory.ts         # Memory 持久化
│   ├── skills.ts         # SkillManager + 内置技能
│   ├── types.ts          # 类型定义
│   ├── test-setup.ts     # 多服务商初始化测试
│   └── test-watch-fix.ts # watch 排除测试
├── docs/
│   └── architecture.md   # 本文档
└── .mini-claw/memory/    # 对话持久化存储
```

## 架构分层

```
┌─────────────────────────────────────────────────────┐
│                    CLI 层 (index.ts)                  │
│  dotenv 加载 │ readline 循环 │ 命令解析 │ chalk 输出  │
├─────────────────────────────────────────────────────┤
│                    核心编排层                         │
│  Gateway: 组合 Agent / Memory / SkillManager          │
├───────────────────┬─────────────────────┬───────────┤
│   Agent           │   Memory            │ Skills     │
│   LLM 交互        │   文件持久化         │ 技能系统    │
│   tools 参数调用   │   会话 CRUD          │ 注册/执行   │
│   tool_calls      │   JSON 存储          │ toTools()  │
│   并行执行         │                      │            │
│   follow-up 二次   │                      │            │
│   调用             │                      │            │
├───────────────────┴─────────────────────┴───────────┤
│                    类型定义层                         │
│  Message / Conversation / Skill / AgentConfig / ...  │
├─────────────────────────────────────────────────────┤
│                    外部依赖                           │
│  openai SDK │ dotenv │ chalk                        │
├─────────────────────────────────────────────────────┤
│                    LLM 服务商                         │
│  OpenAI │ DeepSeek │ Ollama │ Custom (兼容 OpenAI)   │
└─────────────────────────────────────────────────────┘
```

## 模块职责

### Gateway（编排器）

- **文件**: `gateway.ts`
- **角色**: 组合所有核心模块，对外暴露统一接口
- **关键方法**:
  - `sendMessage(userInput)`: 接收用户输入 → 存储到 Memory → 调用 Agent → 存储回复 → 返回文本
  - `getSkillManager()`: 返回 SkillManager 实例，供 CLI 展示技能列表
  - `switchConversation()` / `clearCurrentConversation()` / `getConversationHistory()`: 会话管理
- **初始化流程**:
  1. 创建 Memory 实例（指定存储目录）
  2. 创建 SkillManager 实例
  3. 注册三个内置技能（get_current_time / calculate / echo）
  4. 创建 Agent 实例（传入 AgentConfig + SkillManager）
  5. 创建默认会话

### Agent（LLM 交互）

- **文件**: `agent.ts`
- **角色**: 与 LLM 服务商通信，处理工具调用
- **关键方法**:
  - `process(messages)`: 主处理流程
- **数据流（一次请求）**:
```
messages
  → buildSystemPrompt()（拼接技能列表）
  → toApiMessages()（转为 SDK 格式）
  → SkillManager.toTools()（Skill → ChatCompletionTool[]）
  → openai.chat.completions.create({ tools, tool_choice: "auto" })
  → 判断 msg.tool_calls
      ├─ 无 → 直接返回 msg.content
      └─ 有 → Promise.all 并行执行所有 tool_calls
               → 追加 role: "tool" 消息到 messages
               → 二次 API 调用
               → 返回最终回复
```
- **服务商配置**:
  - 通过 `PROVIDER_CONFIGS` 记录预设 baseURL
  - 优先使用 `AgentConfig.baseURL`，其次使用预设值
  - 支持 OpenAI / DeepSeek / Ollama / Custom 四种提供商
  - `custom` 提供商不支持 tools 时会直接报错

### Memory（持久化）

- **文件**: `memory.ts`
- **角色**: 会话消息的持久化存储
- **存储方式**: JSON 文件，每个会话一个文件，存储在 `.mini-claw/memory/` 下
- **关键方法**:
  - `createConversation()`: 创建新会话
  - `addMessage()`: 追加消息（同时触发磁盘写入）
  - `getMessages()`: 获取会话全部消息
  - `clearConversation()`: 删除会话（同步删除文件）
- **特性**: 支持可选的持久化开关（`persistenceEnabled`）

### SkillManager（技能系统）

- **文件**: `skills.ts`
- **角色**: 技能注册、执行、转换为 OpenAI Tool 格式
- **关键方法**:
  - `registerSkill()`: 注册技能
  - `executeSkill(name, params)`: 按名称执行技能
  - `toTools()`: 将全部 Skill 转为 `ChatCompletionTool[]`，用于 Agent 调 API
  - `getSkillsDescription()`: 获取技能文本描述，用于 system prompt
- **设计要点**: `toTools()` 会为无 `parameters` 的 Skill 自动填充空 schema

### 内置技能

| 技能名称 | 参数 | 说明 |
|----------|------|------|
| `get_current_time` | 无 | 返回当前时间（ISO / 本地 / timezone） |
| `calculate` | `expression: string` | 使用 `Function()` 安全求值 |
| `echo` | `message: string` | 原样回显消息 |

### CLI 层（index.ts）

- **角色**: 命令行交互入口
- **流程**:
  1. 加载 `.env`（从 workspace 根目录和 CWD 两级回退）
  2. 读取环境变量 `LLM_PROVIDER` / `LLM_MODEL` / `LLM_API_KEY` / `LLM_BASE_URL`
  3. 创建 Gateway 实例
  4. readline 循环处理用户输入
- **指令**: `/help` / `/clear` / `/skills` / `/history` / `/info` / `/exit`

## 数据流

### 用户发送消息

```
┌─────────┐     sendMessage()     ┌───────────┐     process()     ┌───────┐
│  index  │ ──────────────────→  │  Gateway  │ ────────────────→ │ Agent │
│  .ts    │                       │  .ts      │                   │ .ts   │
└─────────┘                       └───────────┘                   └───────┘
                                     │    ↑                          │
                              add    │    │ get                      │ toTools()
                              Message│    │ Messages                 │
                                     ↓    │                          ↓
                                  ┌──────────┐                 ┌──────────┐
                                  │  Memory  │                 │  Skills  │
                                  │  .ts     │                 │  .ts     │
                                  └──────────┘                 └──────────┘
                                                                   │
                                                            executeSkill()
                                                                   │
                                                                   ↓
                                                             ┌──────────┐
                                                             │  Result   │
                                                             └──────────┘
```

### Agent 与 LLM 的交互

```
Agent.process()
  │
  ├─ 1. API 调用（带 tools）
  │     └─ openai.chat.completions.create({ tools, tool_choice: "auto" })
  │
  ├─ 2. 判断响应
  │     ├─ 无 tool_calls → 直接返回文本 (finish_reason: "stop")
  │     └─ 有 tool_calls → 进入工具执行流程 (finish_reason: "tool_calls")
  │
  ├─ 3. 并行执行（Promise.all）
  │     └─ 遍历 tool_calls[]
  │         ├─ JSON.parse(tc.function.arguments)
  │         └─ SkillManager.executeSkill(name, args)
  │
  ├─ 4. 构造 follow-up messages
  │     └─ [...original, msg (assistant), ...toolResults (tool)]
  │
  └─ 5. 二次 API 调用（不带 tools）
        └─ openai.chat.completions.create({ messages: followUpMessages })
        └─ 返回最终回复
```

## 扩展点

### 添加新技能

在 `skills.ts` 中创建新的 Skill 对象，然后在 `Gateway.registerDefaultSkills()` 中注册：

```typescript
export const createWeatherSkill: Skill = {
  name: 'get_weather',
  description: '获取指定城市的天气',
  parameters: {
    type: 'object',
    properties: {
      city: { type: 'string', description: '城市名' },
    },
    required: ['city'],
  },
  execute: async (params: { city: string }) => {
    // 调用天气 API
    return { city: params.city, temperature: 25, unit: 'C' };
  },
};

// 在 Gateway 中注册
this.skillManager.registerSkill(createWeatherSkill);
```

### 添加新 LLM 服务商

在 `agent.ts` 的 `PROVIDER_CONFIGS` 中新增条目：

```typescript
claude: {
  baseURL: 'https://api.anthropic.com/v1',
  apiKeyRequired: true,
},
```

然后在 `types.ts` 的 `LLMProvider` 类型中追加 `| 'claude'`。

## 外部依赖

| 依赖 | 版本 | 用途 |
|------|------|------|
| `openai` | ^4.80.1 | LLM 调用（支持 tools 参数） |
| `dotenv` | ^16.4.7 | 环境变量加载 |
| `chalk` | ^5.4.1 | CLI 颜色输出 |
| `tsx` | ^4.19.0 | TypeScript 直接执行 |
| `typescript` | ^5.7.0 | 编译 |
