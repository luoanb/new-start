# Spec: Pulsar 后端架构设计 —— 主流程驱动（SOLID 内生版）

> 状态：**方案待批，未开始实现**。
> 本版为第二版（主流程驱动版），取代 2026-09-06 的十域地图初稿；变更记录见文末 Change Log。

## 一、设计公理

1. **一个内核、四个门面** —— 同一套业务逻辑经桌面 IPC（Tauri）、HTTP（RPC+SSE+WS）、CLI、TUI 暴露，且能脱离 GUI 独立运行。内核必须对传输无知：凡 `use tauri` / `use axum` 的代码都是壳，不是业务（DIP 的根）。
2. **主流程是核心** —— 产品的本质是"不停地推轮子"（推进对话轮）。一切构件按**与主流程的关系**定位：脊柱、推进者、服务者、缝、地面。域不是名词分类，是流程角色。
3. **变化方向已知且有限** —— 会频繁变化的只有四类：工具、hook、服务商/协议、传输门面。架构只在这四个方向预置扩展点，其余地方追求直白（OCP 的精确打击面）。
4. **域是有界上下文** —— 域间协作只允许三条路：端口调用（单向）、事件广播（只出不进）、组合根注入。禁止跨域共享可变状态。

## 二、主流程（脊柱）—— 组织中心

一轮对话的生命周期。**脊柱内无循环**：一轮 = 载入 → 一次模型调用 → 执行本轮声明的全部工具调用 → 落库 → 出口。要不要续轮，由外侧驱动层读 `Outcome` 决定（见第三节）。

```text
════════════ 脊柱 RoundService（一轮，无循环）════════════
  0  串行闸      同会话同一时刻仅一轮（RAII 守卫）；User 轮可抢占取消
                 旧轮并等收敛；驱动者遇忙静默跳过                🔒盖§4
  1  载入上下文   读真相源 sessions/<id>.json（seed / state / messages）
  2  [IP-1]      用户轮：user_round_judgement（打分+课题路由）
                 + assistant.round.before（简报推进）
                 └─ 裁决 switch → 切会话 reload（回到站点 1）
  3  输入落库     User / Nudge / Continue 先落库（先落库再用）    🔒盖真相源
  4  [IP-2]      core.compaction：超阈值压缩本轮 wire（不动真相源）
  5  选型        round_resolver：种子分派 / neuron.select_one 定角色 ──▶ neuron
  6  授权组装     Core 组工具并入 wire（内置 + 文件 11 + git 只读 6）
  7  调模型      ModelPort → provider 整合层 → 协议层 → HTTP/SSE   ──▶ provider
                 └─ 流式 delta → message_delta 事件（前端实时渲染）🔒盖provider铁律
  8  工具执行     本轮模型响应声明的全部 tool_calls 各执行一次      ──▶ tool→world
                 （单次，不回站点 7）；结果随产物落库
                 └─ 工具结果上下文截断（工具自带上限 + 统一兜底）  🔒盖§5上下文安全
  9  产物落库     assistant 产物盖 neuron_id 落库（产品术语的"盖章"）
  10 [IP-5]      round_review 复盘（修订/验收）+ 轮次计数
  11 出口        RoundOutcome{tool_calls_declared, …} 返回驱动层
                 + Conversations{affected} / Topics 事件广播
══════════════════════════════════════════════════════════
```

32 个 🔒 盖章章节中约束行为的绝大部分全站在这条线上（会话单轮、真相源、上下文安全、provider 铁律调用侧、fileops 唯一通道调用侧、terminal 零区分）。钉死主流程 = 钉死大多数冻结约束。

## 三、循环统一在外侧：驱动层

三种驱动者完全对称，都只做一件事：**读上一轮 Outcome，决定是否再推一轮**。它们调用同一个 `RoundPort`，脊柱对驱动者一无所知。

| 驱动者 | 住所 | 推轮策略 |
|---|---|---|
| 用户驱动 | 门面（命令表一条命令 = 推一轮 User 输入） | 推一轮即返回（Chat 语义）；也是 Agent 模式第一轮的推动者 |
| AgentDriver（收敛循环） | conversation 域内、脊柱之外（`loop` 模块） | `Outcome.tool_calls_declared` 为真 → 推 Continue 轮；直至收敛或上限 20 轮 |
| PollerDriver（课题驱动） | autonomy 域 | 课题节拍/简报 → 推 Nudge 轮；并行度共享原子值 + 熔断退避 |

**行为等价说明**：现状 `agent_session` 已在 `run_round` 之外做收敛循环（"循环至收敛，上限 20 轮"是会话级逻辑，不是管线内逻辑）。本设计不改变任何模型调用序列、工具执行序列、落库与事件——只是把"循环"明确升格为与用户/轮询对称的驱动者角色，使其在架构图上有一等公民的位置。

## 四、完整流程地图：脊柱流不是全部，但一切有位置

```text
① 脊柱流（第二节图）── 产品的心跳
② 直通流（门面 ──▶ 域服务，不过脊柱）：
     文件/Git 面板命令 ──▶ world（护栏/确认闸/危险开关，行为盖章不变）
     终端面板        ──▶ world.terminal（PTY 字节流，AI 与人双路同源）
     神经元管理      ──▶ neuron（CRUD/连线/权重/分页）
     课题面板        ──▶ autonomy.topic（状态机读改/pause/resume）
     评分            ──▶ neuron（score_feedback 落网）
     服务商/工具配置  ──▶ provider / tool（保存即热重载 → 事件）
     日志面板        ──▶ platform.log（快照/级别/脱敏）
③ 观测流（域 ──▶ 门面，只出不进）：
     事件总线 12 kind + app://logs + terminal-output/exit
```

推论：**域服务只有一份，两个消费方**——脊柱在站点 8 经 ToolPort 调 tool，文件面板直通流调同一个 world 服务。域不是"为主流程而生"或"为 UI 而生"，一套 API 两个入口。

## 五、七域角色地图

```text
                    ┌─────────────── 驱动层（谁推轮子，循环在外）─────────────┐
                    │  用户（四门面→命令表）  AgentDriver（收敛循环）           │
                    │  autonomy（课题节拍+编排+状态机）                        │
                    └───────────────────────┬───────────────────────────────┘
                                            ▼ RoundPort（同一端口）
  ╔════════════════════ 脊柱域 conversation ═══════════════════════════╗
  ║  session（真相源）→ 串行闸 → [IP-1] → 落库 → [IP-2] → 选型 →        ║
  ║  组装 → 调模型 → 工具执行 → 落库 → [IP-5] → 出口                     ║
  ║  ★ inject：IP-1…IP-5 是脊柱自带的槽位（缝），对外即扩展 API          ║
  ╚═══╤═══════════════╤═══════════════╤═══════════════════════════════╝
      │站点5           │站点7           │站点6/8
      ▼               ▼               ▼
  ┌────────┐    ┌───────────┐   ┌───────────┐
  │ neuron │    │ provider  │   │ tool       │
  │ 人格    │    │ 整合│协议  │   │ 注册/授权   │
  └────────┘    └───────────┘   └─────▲─────┘
                                      │实现 Tool trait
                          ┌───────────┴──────────┐
                          │ world：fs│git│search│pty │
                          └──────────────────────┘
  platform = 地面（config/log/storage/事件总线，所有流站着的地面）

  裁决闭环（走事件，不走调用）：
    站点2 裁决钩子 ──JudgementOutcome 事件──▶ autonomy 改绑 topic
                                     ◀── autonomy 下一轮推新会话 ──
```

### 域界定（一个域 = 主流程上的一个角色）

| 域 | 角色 | 界定一句话 | 状态 |
|---|---|---|---|
| **conversation** | 脊柱（+缝+收敛驱动） | 一轮怎么转、缝上挂什么、要不要续轮 | sessions/*.json、hook_judgements |
| **autonomy** | 推进者 | 谁推轮子、推给谁（topic 状态机 + 编排 + Poller） | topics 表 |
| **neuron** | 服务者 | 人格与记忆（选型/评分/演化） | neurons/connections 表 |
| **provider** | 服务者 | 调一个模型（整合层→协议层，盖章铁律=显式两层） | providers 配置 |
| **tool** | 服务者 | 动作接入（注册/授权/MCP/动态工具） | 工具配置 json |
| **world** | 服务者的手 | 真实世界唯一通道（fs/git/search/PTY 四子域） | workspaces.json+索引+PTY |
| **platform** | 地面 | config/log/storage/事件总线/error | config.json、日志 |

### 关系白名单（全部合法边）

| # | 边 | 载体 | 语义 |
|---|---|---|---|
| 1 | 门面/AgentDriver/PollerDriver → conversation | RoundPort | 推轮（唯一入口） |
| 2 | conversation → neuron | SelectionPort / ScorePort | 选型（站点5）/ 评分落网 |
| 3 | conversation → provider | ModelPort | 模型调用（站点7，含钩子上下文注入复用） |
| 4 | conversation → tool | ToolRegistryPort | 授权 + 组装（站点6）+ 执行（站点8） |
| 5 | tool → world | Tool trait 实现 | 文件/git/搜索/PTY 工具的手住 world |
| 6 | autonomy → conversation | RoundPort | 推 Nudge 轮 |
| 7 | autonomy → conversation | HookAPI（inject 注册） | 注册 assistant.round.* 业务钩子 |
| 8 | conversation →(事件)→ autonomy | JudgementOutcome 事件 | 裁决 switch/create 由 autonomy 消费改绑 topic |
| 9 | 全体 → 事件总线 | EventSink | 12 kind 状态变更（只出不进） |
| 10 | 门面 → 各域（直通流） | 命令表 | 面板命令直达域服务 |

业务钩子需要的模型/历史能力由管线经**钩子上下文注入**（与主对话同源，`ctx.model` / `ctx.messages`），autonomy 不静态依赖 provider/neuron——依赖注入替代跨域静态边。

### 禁止边

- ✘ 脊柱 → autonomy（驱动只能来自驱动层；脊柱对 autonomy 的唯一输出是裁决事件）
- ✘ 脊柱 → world 直连（必须经 tool 端口——"唯一通道"的结构化表达）
- ✘ 任何域回头调用它的调用方；✘ 直通流绕过域服务直触他域状态
- ✘ 跨域共享可变状态；✘ 除 world 外任何域触碰文件系统

## 六、域内五件套（每个域的同构结构）

```text
domain/<域>/
├── mod.rs      # 对外稳定 API：Service + 事件（域之外只许 import 这层）
├── types.rs    # 域类型 + wire DTO（serde 字段名冻结；DTO 归域，不再有集中式 models.rs）
├── ports.rs    # 本域需要的设备接口（Repo/ModelPort/CommandBridge…消费方定义）
├── service.rs  # 域服务（脊柱站点与直通流的共同宿主；业务规则的唯一住所）
└── …           # 域内实现（管线/子模块），外界不可见
```

## 七、内核三件套

### 7.1 命令表 —— 对外能力唯一注册源（OCP 中心）

```rust
// kernel/dispatch.rs：一行 = 一条命令，四个门面自动获得
commands! {
    "list_topics"               => Query  (TopicApp::list),
    "create_topic"              => Write  (TopicApp::create, publishes = topics),
    "send_chat_message"         => Round  (SessionApp::send),          // 推一轮 User
    "send_model_message_stream" => Round  (SessionApp::send_stream),   // 流式：delta 走事件
    "git_commit"                => Write  (GitApp::commit, publishes = git),
    // …108 条逐一入表
}
```

宏同时生成：① Tauri `#[command]` 薄壳与 `generate_handler` 清单；② axum `/api/rpc` 分支；③ 表项（名称→处理器）。旧"lib.rs 103 个 command + rpc.rs 1,710 行字符串分发"两条平行通道收敛为**一张声明表 + 两个机械薄壳**。新增命令 = 加一行；wire 名、参数/返回 JSON 字段冻结不变。

### 7.2 事件总线

```rust
pub trait EventSink: Send + Sync {
    fn emit(&self, change: StateChange);
}
```

`StateChange` 12 种 kind 原样保留（前端按 kind 重拉的契约）。同一总线三路出口：Tauri emit（桌面）、SSE（浏览器）、测试收集器。域只发布事件，从不跨域拉取对方内部状态。

### 7.3 组合根 + 任务监督

`boot.rs` 是**唯一**知道所有具体类型的地方：构造 store → 域服务 → 注册命令表 → 以 `CancellationToken` 启动受监督后台任务（Poller 节拍、神经元容量回收、MCP 渐进装配）。没有结构体再聚合 20 个字段，"380 行构造函数"从结构上消失。

## 八、关键契约（伪代码）

```rust
// domain/conversation/ports.rs —— 驱动层看到的唯一入口
#[async_trait]
pub trait RoundPort: Send + Sync {
    async fn run(&self, input: InputRecord) -> Result<RoundOutcome>;   // 推一轮
}

// 一轮的结果：驱动层据此决定是否续轮（循环在外侧的契约基础）
pub struct RoundOutcome {
    pub response: ChatResponse,
    pub tool_calls_declared: bool,      // true → AgentDriver 推 Continue 轮
    pub session_id: ConversationId,
}

// conversation/loop.rs —— 收敛驱动（脊柱之外、域之内）
impl AgentDriver {
    pub async fn run_to_convergence(&self, first: InputRecord) -> Result<ChatResponse> {
        let mut outcome = self.rounds.run(first).await?;
        for _ in 0..MAX_AGENT_ROUNDS {                     // 上限 20 是驱动者策略
            if !outcome.tool_calls_declared { return Ok(outcome.response); }
            outcome = self.rounds.run(InputRecord::Continue { session: outcome.session_id }).await?;
        }
        Ok(outcome.response)
    }
}

// domain/conversation/ports.rs —— 模型能力是端口，不是对 SDK 的依赖
#[async_trait]
pub trait ModelPort: Send + Sync {
    async fn call(&self, req: ModelRequest) -> Result<ModelResponse>;
    fn stream(&self, req: ModelRequest) -> BoxStream<'static, Result<ModelDelta>>;
}

// domain/workspace/ports.rs —— AI 执行命令与手动终端零区分（盖章 §4）
pub trait CommandBridge: Send + Sync {
    async fn execute(&self, cmd: &str, workdir: &Path) -> Result<ExecOutcome>;
}

// domain/conversation/service.rs —— 脊柱：单轮，无循环
impl RoundService {
    pub async fn run(&self, input: InputRecord) -> Result<RoundOutcome> {
        let _g = self.coordinator.begin_session(&input)?;          // 0 串行闸
        let mut ctx = self.load_context(&input).await?;            // 1
        if let Some(judged) = self.inject.run(Ip1, &ctx).await? {  // 2
            if judged.switched() { ctx = self.reload(judged.session).await?; }
        }
        self.repo.persist_input(&input).await?;                    // 3
        let wire = self.inject.run(Ip2, wire).await?;              // 4 压缩
        let selection = self.resolver.resolve(&ctx).await?;        // 5
        let wire = self.assemble(wire, selection).await?;          // 6
        let resp = self.models.call(wire).await?;                  // 7（流式经事件）
        let tools = self.tools.execute_all(&resp.tool_calls).await?; // 8 单次
        self.repo.persist_outcome(&resp, &tools).await?;           // 9 盖章
        self.inject.run(Ip5, &outcome).await?;                     // 10
        self.sink.emit(StateChange::Conversations { .. });         // 11
        Ok(RoundOutcome { tool_calls_declared: !resp.tool_calls.is_empty(), .. })
    }
}
```

## 九、SOLID 逐条落点

| 原则 | 体现 |
|---|---|
| **S** 单一职责 | 脊柱只管一轮；循环归驱动者；类型/端口/编排/实现四分离（五件套）；后台任务归 boot 监督者 |
| **O** 开闭 | 命令表（加命令不改门面）；续轮策略变化只改驱动者不动脊柱；四个变化方向各设扩展点（Tool trait、IP 缝、ModelPort/协议透传、门面适配器）；其余刻意不抽象 |
| **L** 里氏替换 | 端口是行为契约（前置/后置写 rustdoc `# Errors`）；Tool/GitBackend 既有实现原样满足 |
| **I** 接口隔离 | 门面只见本域窄服务；端口全部单方法级窄接口；驱动者只见 RoundPort + RoundOutcome，看不见脊柱内部 |
| **D** 依赖倒置 | 箭头向内；Repo/Port 由域定义、platform/store 实现；域零框架 import，内核可脱离 Tauri 单独 `cargo test` |

## 十、功能等价证明（"保证功能正常"内建于设计）

1. **命令面**：108 个 wire 命令名逐一入表，参数/返回 JSON 字段不变（snake_case；hook 过滤器 camelCase 例外保留）。
2. **事件面**：`app://state-changed`（12 kind）+ `app://logs` + `terminal-output/exit` 原样。
3. **存储面**：`.pulsar/` 布局与 schema 完全不变（app.db 五张表、sessions/*.json、workspaces.json、dynamic_tools/mcp_servers.json、search/<hash>）。
4. **行为面**：32 个 🔒 章节逐条映射——fileops 唯一通道=FileService 唯一写路径；provider 两层铁律=显式子层边界；terminal 零区分=PtyBridge 双路广播；会话单轮+上下文安全=串行闸+截断原样；git 确认闸/危险开关=GitService 内不变。**循环外移不改变任何外部可见行为**：模型调用序列、工具执行序列、落库与事件与现状一致（现状 agent_session 本就在 run_round 之外循环）。
5. **并发面**：锁纪律四条保留（不持锁跨网络 I/O、clone-out then await、禁 blocking_lock 死等、meta→topic→neuron 加锁顺序）；域内状态私有化后跨域拿不到别人的锁；原语收敛为两类（域内 parking_lot、跨任务 tokio 通道）。

## 十一、刻意不做的设计

不引入 CQRS/事件溯源/通用插件加载器/无第二实现的仓储 trait；hook 注册表保持编译期静态（刻意设计）；不为三种驱动者造统一基类（它们只是调用同一端口的三个调用方）；不把驱动循环放回脊柱；不写"未来可能的"抽象——扩展点只在公理 3 列出的四个方向上。

## 十二、里程碑（主线优先）

- **M1 地面+内核+脊柱最小闭环**：platform（store/config/log/events）+ kernel（dispatch/events/boot）+ conversation（session/round/safety，Chat 模式种子分派、无缝无 neuron）+ provider → **四门面打通"用户驱动 Chat 一轮"**。
- **M2 手装上**：tool + world（fs/git/search/pty）+ 直通流（文件/git/终端面板命令）→ 站点 8 真实执行。
- **M3 认知与驱动**：neuron（选型/评分/管理）+ inject 缝（IP 槽位+裁决账本）+ AgentDriver（收敛循环）+ autonomy（topic 状态机+PollerDriver+裁决事件闭环）。
- **M4 清理与同步**：删除旧树（topic_manager 死域、script_engine 处置按 Open Questions）、models.rs 拆分收尾、Reverse Sync（`docs/pulsar/architecture.md` 重写为本蓝图、各域文档"实现参照"路径更新）。

每个里程碑出口：`cargo fmt/check/test` 全绿（441 个既有测试随域平移、零回退）、wire diff 为空、四门面冒烟通过、git 小步提交。域内已验证的健康实现（conversation 管线、neuron、hook、gitops）按新结构平移；接缝、门面与内核按蓝图重写。

## Open Questions

- [ ] `runtime/script_engine`（mlua 孤立子系统）去留：保留为预留域，还是 M4 清理？
- [ ] 并发原语收敛（4 类 → 2 类）的边界：M1-M3 期间新旧树并存时各自维持现状锁风格，是否可接受。
- [ ] `send_message` / `send_model_message` / `send_model_message_stream` 三入口收敛为 RoundPort 单入口 + 流式变体的时机（建议随 M1）。

## Checkpoint Summary

- 当前任务理解：按 SOLID 从零重新设计 pulsar 后端架构，主流程为组织中心，功能等价。
- 当前核心目标：设计书（主流程驱动版）获批并成为实现唯一依据。
- 当前进度：第二版已落库本文档（脊柱无循环、循环统一外侧、七域角色制）。
- 下一步 1: 用户审阅批复；下一步 2: 获批后从 M1 启动。
- 涉及文件 / 模块：本文档；实现阶段涉及 `packages/pulsar-app/src-tauri/**`。
- 风险：盖章约束已在 §十.4 逐条映射；世界域（world）约 9.5k 行需内部子模块纪律。
- 验证方式：每里程碑 cargo 全绿 + wire diff 空 + 四门面冒烟。
- Execution Approval: `Pending`

## Change Log

- 2026-09-06: 初稿落库（十域地图版）。
- 2026-09-09: 第二版（主流程驱动版）——主流程升为组织中心；**工具循环移出脊柱**（站点 8 改为单次工具执行，循环统一在外侧驱动层：用户/AgentDriver/PollerDriver 三驱动对称，经 RoundPort + RoundOutcome 契约续轮）；域按流程角色收敛为 7 域（脊柱/推进者/服务者/缝/地面）；里程碑改为主线优先（M1 先通脊柱）。

## Validation

- Self-check: §二/§三 主流程与驱动层契约完整；§十.4 含循环外移的行为等价论证。
- Static checks: 不适用（纯文档）。
- Runtime / Test: 不适用（未开始实现）。
- Human confirmation: 待用户批复本设计书。
- 结果汇总：第二版落库完成，等待批准。
- 核心目标是否已由证据证明完成：否（架构实现未开始）。
- 剩余差距 / 风险：三项 Open Questions 待定；实现未启动。

## Resume / Handoff

- 当前状态：主流程驱动版设计书已落库，Execution Approval 为 Pending。
- 当前卡点：等待用户批复（或继续迭代设计）。
- 下一步唯一动作：按批复修订本文档，或启动 M1（platform + kernel + 脊柱最小闭环）。
- 下一轮核心目标：M1 —— 四门面打通"用户驱动 Chat 一轮"。
