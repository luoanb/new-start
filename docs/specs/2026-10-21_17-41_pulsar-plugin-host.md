# Spec: pulsar-plugin-host（完整插件体系，不含业务代码）

## Goal

- 要解决什么问题：`pulsar-kernel` 只包含 Cordis 框架层（vendor 9 包），缺少装配、两端装载、UI 扩展与前后端通信四层，无法承载一个真正的插件化应用。
- 验收结果：新增独立工程，能启动一个 Web 形态的 Host，装载一个同时具备 Host 面与 Client 面的 hello 插件并渲染到页面；禁用该插件后服务、槽位与界面贡献全部消失。

## Done Contract

- 什么算完成：空壳可启动；hello 插件可安装并在页面出现；禁用后无残留；`pnpm typecheck` 通过。
- 由什么证明：`pnpm start` 起 Host 并输出监听地址；浏览器打开后可见 hello 组件；`pnpm demo:disable` 后组件与服务消失；三条命令的实际输出。
- 哪些情况仍算未完成：只有目录与依赖、没有可运行 Host；插件只能注册 Host 侧；slot 仍带 `session` 语义；运行时依赖任一 `core/*`、`llm/*`、`session/*`、`tools/*` 业务包。

## Scope

- In：
  - 复用 `packages/pulsar-kernel/vendor` 的 9 个 Cordis 包。
  - 从 `packages/deepseek-harness` 复制并裁剪以下包：`boot/app-boot`、`boot/hmr`、`boot/plugin-manager`、`host/webserver`、`host/frontend-static`、`client/web`、`client/modules`、`client/connection`、`client/hmr`、`client/store`、`client/locale`、`client/ui-slots`、`client/ui-renderer`、`client/ui-theme`、`client/ui-primitives`、`api/gateway`、`api/remotes`、`typert/{protocol,registry,loader,generator}`。
  - Slot scope 泛化：`root | scoped-maybe | scoped`；`SessionProvider` 改为 `ScopeProvider`；移除 `useSession` / `useSessions` / `useWorkspace` 等业务标准 seat。
  - 传输层：`host/webserver` + `/api` + `remote.mux`（dsh 方案），桌面端后置。
  - 一个 skeleton bundle、一个最小 launcher、一个 `examples/hello-plugin`（Host 注册服务 + Client 注册 slot 组件）。
- Out：
  - `core/*`（session / system-prompt / tools / agent / agent-loop / scope / agent-default-model / agent-tool-presentation）、`llm/*`、`bundle/base|web-app` 的业务行、全部 `ui-*` 业务插件包。
  - Electron 桌面壳、Tauri、Python SDK、ACP、官方插件市场目录。
  - 发布流水线、CI 矩阵、双语文档、100% 覆盖率门禁。

## Facts / Constraints

- 已确认事实：
  - `vendor/` 9 包 6,417 行 src；`packages/core` 8 包 14,571 行 src；8/8 core 包依赖 `@deepseek-ai/cordis`，vendor 对 `dsh-*` 零引用。
  - `client/modules` 为双面包，node 半扫描 `dsh.client` 并组合读取图，浏览器半是 lazy CJS 模块表；其依赖只有 webserver / package-manifest / client-store，无业务包。
  - `client/web` 定义 `PLATFORM_MODULES`，执行两阶段 boot（模块阶段 + 插件阶段），读取 Host 注入的 `__DSH_BOOT__`。
  - `ui-renderer` 激活依赖 `slots`、`sessions`、`layout` 三个服务，内部持有 `SessionProvider`；裁剪时必须替换，否则编译不过。
  - Slot scope 目前仅有 `root / session-maybe / session`（`docs/subsystems/slots.md`）。
  - `typert/generator` 内含 `tool-schema.ts`、`cordis-catalog.ts` 等业务与文档专用 emitter；RPC 相关在 `remote-*.ts`。
  - `app-boot` 依赖 launch-environment / home-paths / package-manifest；对 `dsh-system-prompt` 仅 devDependency（测试 fixture）。
  - dsh 各包均 MIT，`publishConfig.access = public`；可以直接复制源码，但必须保留 LICENSE 与版权声明。
- 技术/业务约束：
  - 不能整包复制：业务耦合处必须 copy-and-prune，并重新命名包作用域。
  - 三套依赖语义必须分别保留，不能合并：Cordis service `inject`（运行时等待）、module graph `external`（同步 require）、`dsh.client.inject`（仅信息性）。
  - 仓库约定：ESM、跨包用包名、包内相对导入用 `.ts` 后缀、tsconfig paths 指向 src。
  - 先做 Web profile；Electron 只替换 carrier，不改变体系。
- 已知风险：
  - scope 改名会波及 slots 类型代数与所有注册者，属于跨包类型改造。
  - typert generator 依赖 Host 的 `ts.Program` 与工作区拓扑，裁剪后需重新验证 codegen 产物。
  - 无业务 profile 下 HMR 与 profile 写锁尚未验证，可能存在对 `dsh-base` 行 id 的隐式假设。
  - 复制包的升级策略未定，后续与上游同步成本高。

## Open Questions

- [ ] 包作用域用 `@pulsar/*`，还是保留 `@deepseek-ai/*` 减少改名成本？建议 `@pulsar/*`，避免与上游混淆。
- [ ] 保留 profile 概念（单 profile 固定），还是去掉 profile 直接读一份 patch？建议保留，plugin-manager 与 HMR 都依赖它。
- [ ] locale 机制保留但词典为空，还是直接移除？建议保留机制。

## Restated Understanding

- 我理解当前任务是：把 DeepSeek Harness 的插件体系整体抽成一个可独立运行的新工程，保留机制、剔除业务。
- 当前核心目标是：得到一个"空壳可启动、插件可装可卸"的宿主平台，而不是 agent 框架。
- 当前边界是：Web 形态的 Host + Browser 两棵树；不含会话、模型、工具等业务语义。
- 暂不处理：桌面打包、插件市场、发布与版本兼容豁免。

## 接口契约设计

```ts
// 宿主生命周期
export interface PluginHostOptions {
  /** 数据根，默认 ~/.pulsar-plugin-host */
  home?: string
  /** 监听端口，0 表示由系统分配 */
  port?: number
}

export class PluginHost {
  constructor(options?: PluginHostOptions)
  /** 装配 profile → 起 HTTP → 注入 boot graph；重复调用幂等 */
  start(): Promise<{ url: string }>
  /** 卸载全部插件与 HTTP 服务 */
  stop(): Promise<void>
  /** 运行时启停一个插件条目，等价于 profile patch 的 disabled 翻转 */
  setEnabled(entryId: string, enabled: boolean): Promise<void>
}
```

```ts
// Slot scope 泛化（替换 session/session-maybe）
export type SlotScope = 'root' | 'scoped-maybe' | 'scoped'

export interface ScopeProviderProps {
  /** 当前作用域标识；scoped 必须提供，scoped-maybe 可选 */
  scopeId?: string
}
```

```ts
// examples/hello-plugin：一个包两个面
// Host 面 src/index.ts
export const name = 'hello-plugin'
export const inject = []
export function apply(ctx: Context) {
  ctx.provide('hello')            // 供其他插件注入
  ctx.on('hello/ping', () => 'pong')
}

// Client 面 src/client/index.ts
export const inject = ['slots']
export function apply(ctx: Context) {
  ctx.slots.register({ name: 'root.main', id: 'hello' }, HelloPanel)
  // HelloPanel 通过 inject face 拿到 hello 服务的只读投影
}
```

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是。范围只覆盖"机制"，业务包全部列入 Out。
- 若否，偏差在哪里：无。
- 是否需要调整本轮目标或范围：否。

## Checkpoint Summary

- 当前任务理解：新建 `packages/pulsar-plugin-host`，承载完整插件机制，零业务代码。
- 当前核心目标：空壳可启动、hello 插件可装可卸、两端各一棵 Cordis 树。
- 当前进度：范围与两项关键决策已定，规格待批准。
- 下一步 1：建立工程骨架与依赖闭包，复制 vendor 与装配层，跑通"空壳启动"。
- 下一步 2：泛化 slot scope，改造 ui-renderer，跑通"hello 插件上屏"。
- 涉及文件 / 模块：`packages/pulsar-plugin-host/**`、本 spec。
- 风险：scope 类型链改造、typert 裁剪后的 codegen、无业务 profile 的 HMR 行为。
- 验证方式：`pnpm typecheck`、`pnpm start`、`pnpm demo:hello`、`pnpm demo:disable`。
- Execution Approval: `Pending`

## Change Log

- 2026-10-21: 建立规格。用户决策：① 前端 slot 语义泛化，保持零业务语义；② 采用 dsh 那套 Web 形态（webserver + /api + remote.mux），不使用 Tauri IPC。

## Validation

- Self-check: 待执行。
- Static checks: 待执行。
- Runtime / Test: 待执行。
- Human confirmation: 待用户批准执行。
- 结果汇总：未开始。
- 核心目标是否已由证据证明完成：否。
- 若未完成，当前剩余差距：全部实现工作。
- 剩余风险：见 Facts / Constraints。

## Resume / Handoff

- 当前状态：规格已落盘，等待执行许可。
- 当前卡点：三个 Open Questions 待确认（默认取建议值）。
- 下一步唯一动作：批准后先完成工程骨架与"空壳启动"。
- 下一轮核心目标：hello 插件在页面上可装可卸。
