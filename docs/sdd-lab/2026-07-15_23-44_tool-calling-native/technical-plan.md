# Technical Plan / 技术方案: tool-calling-native

## Requirement Baseline / 需求基线

- 对应需求文档：`docs/sdd-lab/2026-07-15_23-44_tool-calling-native/requirements.md`
- 需求确认状态：等待用户确认。
- 本方案覆盖范围：`types.ts`、`skills.ts`、`agent.ts` 三个文件的改造，不改 Gateway/CLI 接口。

## Current Project Facts / 当前项目事实

- 已读取文件/模块：
  - `packages/mini-claw/src/types.ts` — `Skill` 接口（name, description, execute）
  - `packages/mini-claw/src/skills.ts` — `SkillManager` 类 + 三个内置 Skill 工厂函数
  - `packages/mini-claw/src/agent.ts` — `Agent` 类，含 `parseSkillCall` 正则 + `buildSystemPrompt` 注入
- 当前实现事实：
  - 工具调用完全依赖 system prompt 中"请以 JSON 格式返回"的指令。
  - 解析用 `content.match(/\{[\s\S]*\}/)` + `JSON.parse`，仅支持单次调用。
  - 每个 Skill 的入参没有 JSON Schema 定义，LLM 只能靠 description 猜。
- 相关接口/数据结构：
  - `Skill.execute(params: any) => Promise<any>` — 入参无约束。
  - `Agent.process(messages: Message[]) => Promise<string>` — 当前调用方是 `gateway.ts`，不改。
- 约束与风险：
  - `openai` 包版本 `^4.80.1`，`ChatCompletionTool` 类型可用。
  - DeepSeek 和 Ollama 兼容同一 `tools` 格式。

## Open Questions / 开放问题

- [ ] Q1 `custom` 提供商不支持 `tools` 时怎么办？
  - 触发来源：需求文档
  - 无法确定的内容：用户是否愿意为 `custom` 保留当前正则回退路径
  - 影响范围：`agent.ts` 的 `process()` 方法
  - 候选处理：方案 A — 全部统一走 tools，custom 不支持则报错；方案 B — tools 失败时回退到现有 prompt + 正则方式
  - 状态：待用户确认

## Solution Options / 方案候选

### Option A / 方案 A（推荐）

- 推荐：是
- 方案摘要：一刀切改全部走原生 `tools`，`custom` 提供商不支持 `tools` 时直接报错。
- 涉及模块：`agent.ts`、`skills.ts`、`types.ts`
- 优点：
  - 代码最简洁，不留历史包袱。
  - 所有服务商统一的工具调用路径，测试覆盖简单。
- 缺点：
  - 如果用户使用的 `custom` 服务商不支持 `tools`，工具调用失效。
  - 删除正则回退后，`custom` 无法降级工作。
- 风险：低。

### Option B / 方案 B

- 推荐：否
- 方案摘要：优先走原生 `tools`，如果 `custom` 提供商抛错或返回不支持，自动回退到当前 prompt + 正则方式。
- 涉及模块：`agent.ts`、`skills.ts`、`types.ts`
- 优点：
  - 兼容性最强，`custom` 提供商遇到不支持时仍可用。
- 缺点：
  - 代码需维护两套路径，增加复杂度。
  - `custom` 遇到不兼容 API 时需要 try/catch 降级，降级后的行为不可靠。
  - 回退路径很难充分测试。
- 风险：中（维护负担 + 不可靠回退）。

## Decision / 方案决策

- Selected / 选定方案：（等待用户决策）
- Why / 选择原因：（等待用户决策）
- Decision Owner / 决策人：用户
- Decision Time / 决策时间：（等待用户确认）
- Open Questions 状态：Q1 待用户确认

## API Design / API 设计

- 变更类型：扩展（非破坏性）
- 消费方：`SkillManager` 内部使用，对外 `Skill` 接口后向兼容
- 真相源文件：
  - `types.ts` — `Skill.parameters` 新增
  - `skills.ts` — `SkillManager.toTools()` 新增

### Skill 接口

```
Skill {
  name: string;          // 不变
  description: string;   // 不变
  parameters?: Record<string, unknown>;  // 新增，选填
  execute: (params: any) => Promise<any>; // 不变
}
```

`parameters` 格式为标准 JSON Schema Object：

```typescript
{
  type: "object",
  properties: {
    expression: { type: "string", description: "数学表达式" }
  },
  required: ["expression"]
}
```

### SkillManager.toTools()

```typescript
toTools(): ChatCompletionTool[] {
  return this.getAllSkills().map(skill => ({
    type: 'function' as const,
    function: {
      name: skill.name,
      description: skill.description,
      parameters: skill.parameters || { type: 'object', properties: {} },
    },
  }));
}
```

### Compatibility Notes / 兼容说明

- 已有 `Skill` 实例不设 `parameters` 时，`toTools()` 会自动用 `{type:"object", properties:{}}` 填充，SDK 正常接受。
- `Skill` 的 `execute` 签名不变，现有 Skill 工厂函数只需加 `parameters` 字段即可。
- `Gateway` 和 `index.ts` 不需要任何改动。

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：
  - 用户确认需求边界（requirements.md 已评审）。
  - 用户确认 Q1 的 custom 回退决策。
  - 用户明确批准进入执行阶段。

### Step 1. types.ts — Skill 接口扩展

#### 文件：`packages/mini-claw/src/types.ts`

- 改动类型：修改
- 改动内容：`Skill` 接口新增可选字段 `parameters?: Record<string, unknown>`
- 设计约束：
  - API：后向兼容，已有 Skill 实例不报错。
- 验收点：`pnpm build` 编译通过，不产生新的类型错误。

### Step 2. skills.ts — 内置 Skill 补 Schema + toTools

#### 文件：`packages/mini-claw/src/skills.ts`

- 改动类型：修改
- 改动内容：
  1. `createTimeSkill` 无参，无需加 parameters（原有工厂函数不变，`toTools` 自动填充空 schema）
  2. `createCalculatorSkill` 加 `parameters: { type:"object", properties:{ expression: {type:"string"} }, required:["expression"] }`
  3. `createEchoSkill` 加 `parameters: { type:"object", properties:{ message: {type:"string"} }, required:["message"] }`
  4. `SkillManager` 新增 `toTools()` 方法（见 API Design 节）
- 设计约束：
  - API：`SkillManager` 对外新增方法，不破坏现有调用。
- 验收点：`toTools()` 输出符合 `ChatCompletionTool[]` 格式。

### Step 3. agent.ts — 重写 process，删除正则

#### 文件：`packages/mini-claw/src/agent.ts`

- 改动类型：修改
- 改动内容：
  1. 删除整个 `parseSkillCall()` 方法
  2. 修改 `buildSystemPrompt()` — 删除"请以 JSON 格式返回"相关指令，改为简短提示（如"你可以使用以下技能：..."）
  3. 改造 `generateResponse()` — 传入 `tools` 参数 + `tool_choice: "auto"`
  4. 重写 `process()` — 判断 `msg.tool_calls`，如果有则并行执行后二次调用
- 设计约束：
  - API：`process(messages)` 签名不变。
  - 数据流：
    1. `client.chat.completions.create({ ..., tools })`
    2. 判断 `response.choices[0].message.tool_calls`
    3. 有 → `Promise.all()` 并行执行 → 追加 `role: "tool"` 消息 → 二次调用 → 返回
    4. 无 → 直接返回 `msg.content`
- 验收点：
  - LLM 返回 `tool_calls` 时能正确解析并执行。
  - LLM 返回多个 `tool_calls` 时并行执行。
  - LLM 不调工具时正常返回文本。
  - 不再有正则匹配逻辑。

### Step 4. 验证

#### 命令

- `pnpm build` — 编译通过
- `pnpm dev` — 启动正常，工具调用仍可触发

## Risk And Mitigation / 风险与缓解

- 风险：`custom` 提供商不支持 `tools` 参数，工具调用静默失效。
  - 缓解方式：Q1 待用户决策（选方案 A 直接报错 / 选方案 B 回退）。
- 风险：Ollama 旧版（< v0.12）或旧模型（< llama3.1）不支持 tools。
  - 缓解方式：当前项目已默认使用 llama3.1:8b，且在 `.env.example` 中已列出该配置，风险低。
- 风险：`tool_calls` 返回的 `arguments` 是 JSON 字符串，忘记 `JSON.parse` 导致运行时错误。
  - 缓解方式：在执行步骤中明确写出 `JSON.parse(tc.function.arguments)`，走 review。

## Execute Checkpoint / 执行检查点

- 当前理解：改造只涉及三个文件，不改 Gateway/CLI，`custom` 回退策略待定。
- 核心目标：用原生 `tools` 替代正则，支持并行 `tool_calls`，所有 4 个服务商统一。
- 下一步动作：
  1. 用户审阅 `requirements.md` 和 `technical-plan.md`
  2. 用户确认 Q1（custom 回退策略）
  3. 用户明确批准后进入执行
- 风险：主要在 custom 回退策略和验证深度。
