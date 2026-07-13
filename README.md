# 🦞 Mini-Claw

一个简化版的 OpenClaw 风格 AI 智能体框架，用于学习 Agent 开发原理。

## 仓库结构

本仓库采用 pnpm workspace 单包结构：

```
.
├── packages/
│   └── mini-claw/      # Mini-Claw 应用包
│       ├── src/        # 源码
│       │   ├── index.ts     # CLI 入口
│       │   ├── gateway.ts   # 网关模块
│       │   ├── agent.ts     # 智能体模块
│       │   ├── memory.ts    # 记忆系统
│       │   ├── skills.ts    # 技能系统
│       │   └── types.ts     # 类型定义
│       ├── .env.example     # 环境变量模板
│       ├── package.json
│       └── tsconfig.json
├── .env                # 环境变量（根目录执行时使用）
├── .mini-claw/         # 数据存储目录（自动创建，已配置 watch 排除）
├── package.json        # workspace 根入口
├── pnpm-workspace.yaml
└── README.md
```

> 根目录为 workspace 管理入口，应用源码位于 `packages/mini-claw`。后续拆分多包时，新增目录直接放在 `packages/` 下。

## 核心架构

Mini-Claw 参考 OpenClaw 设计，包含四个核心组件：

| 组件 | 功能 | 对应文件 |
|------|------|----------|
| **Gateway（网关）** | 消息路由、会话管理 | [packages/mini-claw/src/gateway.ts](packages/mini-claw/src/gateway.ts) |
| **Agent（智能体）** | LLM 集成、推理逻辑 | [packages/mini-claw/src/agent.ts](packages/mini-claw/src/agent.ts) |
| **Memory（记忆）** | 会话持久化、记忆管理 | [packages/mini-claw/src/memory.ts](packages/mini-claw/src/memory.ts) |
| **Skills（技能）** | 工具注册、执行 | [packages/mini-claw/src/skills.ts](packages/mini-claw/src/skills.ts) |

## 支持的 LLM 服务商

| 服务商 | 说明 | 是否免费 |
|--------|------|----------|
| **OpenAI** | GPT-4、GPT-3.5 等 | 否 |
| **Ollama** | 本地运行 Llama3、Qwen 等 | 是 |
| **DeepSeek** | DeepSeek 模型 | 否 |
| **Custom** | 任何兼容 OpenAI 格式的 API | - |

## 快速开始

### 1. 安装依赖

```bash
pnpm install
```

### 2. 配置环境变量

复制 `.env.example` 为 `.env` 并填入你的配置：

```bash
cp .env.example .env
# 编辑 .env 文件选择 LLM 服务商并配置
```

#### 示例配置：

**使用 OpenAI（默认）:**
```env
LLM_PROVIDER=openai
LLM_MODEL=gpt-4o-mini
LLM_API_KEY=your_openai_api_key_here
```

**使用 Ollama（本地免费）:**
```env
LLM_PROVIDER=ollama
LLM_MODEL=llama3.1:8b
LLM_BASE_URL=http://localhost:11434/v1
```

**使用 DeepSeek:**
```env
LLM_PROVIDER=deepseek
LLM_MODEL=deepseek-chat
LLM_API_KEY=your_deepseek_api_key_here
```

### 3. 运行程序

```bash
pnpm dev
```

## 使用方式

启动程序后，你可以：

- **直接对话** - 输入任何问题进行交流
- **`/help`** - 查看帮助信息
- **`/skills`** - 列出所有可用技能
- **`/history`** - 查看会话历史
- **`/clear`** - 清空当前会话
- **`/info`** - 查看当前会话信息
- **`/exit`** - 退出程序

## 内置技能

| 技能 | 功能 | 示例 |
|------|------|------|
| `get_current_time` | 获取当前时间 | "现在几点了？" |
| `calculate` | 数学计算 | "计算 25 * 4 + 10" |
| `echo` | 回显消息 | "echo 你好" |

## 开发命令

| 命令 | 说明 |
|------|------|
| `pnpm dev` | 开发运行（根目录触发） |
| `pnpm dev:watch` | watch 模式（已排除 `.mini-claw`） |
| `pnpm build` | TypeScript 编译 |
| `pnpm start` | 运行编译产物 |

也可进入 `packages/mini-claw` 目录直接使用同组命令。

## 扩展开发

### 添加新技能

在 [packages/mini-claw/src/skills.ts](packages/mini-claw/src/skills.ts) 中添加新技能对象，然后在 Gateway 构造函数中注册。

### 学习要点

这个项目展示了 Agent 开发的核心概念：
- 如何通过工具调用（Skills）扩展 LLM 能力
- 如何实现会话记忆（Memory）持久化
- 如何通过 Gateway 协调各组件
- 如何设计灵活的插件架构

## 参考资源

- [OpenClaw 官方文档](https://agentopenclaw.io/)
- [OpenAI API 文档](https://platform.openai.com/docs/)
