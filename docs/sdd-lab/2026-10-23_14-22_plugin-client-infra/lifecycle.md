# Lifecycle / 生命周期: 插件前端基建现代化

```yaml
status: done
result: completed
created_at: 2026-10-23 14:22
updated_at: 2026-10-23 18:40
owner: user
```

## Current Summary / 当前摘要

- 批准状态：用户已批准执行（2026-10-23 16:28）。
- 当前状态：`executing -> done`，依据：P0 ~ P3 全部落地并通过 `typecheck` / `verify` / 浏览器实测。
- 交付内容：`packages/pulsar-plugin-client`（浏览器契约 + `PluginRuntime` / `BrowserRuntime` + `Compiler` + `esbuildCompiler` + 产物目录原语 + `node` / `dom` / `solid` 三个适配器）、宿主收缩为零构建内核、产物落盘与保留策略、开发期热替换。
- 已定方案：Solid 为官方适配器（npm 依赖，import map 共享）；同页多适配器共存；根 `packages/` 归属、局部 workspace 安装；后端统一走 `PluginRuntime`；产物模型 P0 一次做对；产物目录 `<cwd>/.pulsar/artifacts` 保留最近 3 个 hash。
- 遗留（未纳入本期）：Vue / React 适配器、P4（RPC 类型生成、Playwright 验收）、根 workspace 统一安装后移除 `pulsar-plugin-host/pnpm-workspace.yaml` 中的 client 条目。
- 下一步唯一动作：无；如需继续，先开新迭代再动代码。

## Execution Log / 执行记录

- 1. 2026-10-23 14:22: 创建迭代。读取 `packages/pulsar-plugin-host` 现有实现（`client/runtime.js`、`src/host.ts`、`src/profile.ts`、`src/types.ts`、`src/verify.ts`）。
- 2. 2026-10-23 14:22: 产出 `requirements.md`；无 Figma / 视觉稿，按规则不创建 `visual-design.md`。
- 3. 2026-10-23 14:22: 产出 `technical-plan.md`，含 Solid / Vue / React 三套方案对比；`Decision` 保持待用户决策，未进入执行。
- 4. 2026-10-23 15:10: 因用户指出「不同框架在构建打包节点就可能不同」，判定原 `RuntimeAdapter`（仅 `mount/slot`）边界错误——它把单个框架的接入面切成了宿主 `bundleClient` 与适配器两半。重构为 `PluginRuntime.activate(source, ctx) -> Activation`，适配器全权负责编译、存放、投递、执行。
- 5. 2026-10-23 15:10: 补充适配器之间的四层关系（继承特化 / 平行 / 共存耦合 / 降级兼容），并明确 import map 是唯一结构化耦合点（全页唯一、先于首个 module script、由宿主合并去重）。
- 6. 2026-10-23 15:10: 整篇重写 `technical-plan.md`（宿主内核收缩为装配 + fiber 生命周期 + RPC/Events + HTTP/路由/HTML；新增 `BrowserRuntime` 基类；Open Questions 扩到 Q1 ~ Q6）。同步回写 `requirements.md`：删除与「宿主零构建」冲突的旧约束，补充验收断言与 2026-10-23 15:10 的五条需求决策。
- 7. 2026-10-23 15:40: 按用户新增约束「编译产出需要存储、不随插件销毁、运行阶段直接从该位置加载」修订三份文档。`technical-plan.md` 新增「产物目录（Artifact Store）」章节：`<artifactsDir>/<runtime>/<pluginId>/<hash>/` 三级布局 + `manifest.json` 命中判定 + `<hash>.tmp/` 原子 rename，并明确「挂 fiber 回滚的是路由与监听，不是产物」。`requirements.md` 同步修正「路由与缓存回滚」等冲突表述，新增三条落盘验收与 Q7；`lifecycle.md` 更新摘要与下一步。
- 8. 2026-10-23 16:00: 因用户指出「基类承载编译」是「分离又没有完全分离」，且各框架编译过程并不同（`dom` 剥离 TS、`react` 用 esbuild JSX、`solid` 的 esbuild JSX 只是近似、`vue` 必须走 `@vue/compiler-sfc` 并产出 scoped CSS），把编译从 `BrowserRuntime` 抽出为独立策略接口 `Compiler`（`fingerprint` / `compile` / 可选 `watch`），基类只保留查盘、原子落盘、静态投递、路由回滚与 boot graph 交付。`CompileResult` 固定为 `files | external` 两态；`esbuildCompiler(options)` 改为组合件；包导出分 `.` / `./runtime` / `./runtime/esbuild` 三段；Step 1、Step 2、Step 5、Step 6、Risk 与 Execute Checkpoint 全部同步，新增两条机械验收（基类无打包器标识、新增框架不改 `browser.ts` / `artifacts.ts`）。
- 9. 2026-10-23 16:20: 用户关闭 Q1 ~ Q7（Q1 = A Solid，Q2 = 允许多适配器共存，Q4 = 根 workspace，Q5 = 后端统一，Q6 = 产物模型进 P0，Q7 = 固定路径 + 保留最近 3 个；Q3 因选 Solid 不适用）。`technical-plan.md` 填写 `Decision`、勾选全部 Open Questions、Step 0 前置条件解除、Step 2 `node` 与 Step 3 verify 按 Q5 定稿、Step 5 固化为 `solid.ts`、产物目录新增第 5 条保留约定（`prune()` 在新产物 rename 后触发并跳过当前 hash）、Risk 与 Checkpoint 同步；`requirements.md` 勾选 Open Questions、补 16:20 需求决策与三条验收、Scope In/Out 按 Q4 ~ Q6 调整。状态仍为 `planned`，等待批准后转 `executing`。
- 10. 2026-10-23 16:52: Step 1 完成。新建 `packages/pulsar-plugin-client`（`package.json` 三段 exports、`tsconfig.json`）：浏览器契约 `types/slots/rpc/events/locale/styles/define/index`、适配器契约 `runtime/types.ts`、编排基类 `runtime/browser.ts`、编译策略 `runtime/compiler/{types,esbuild}.ts`、产物原语 `runtime/artifacts.ts`。`pnpm --filter @pulsar/plugin-client typecheck` 通过；临时自测脚本验证「含 CSS 多产物 / 未变源码零编译 / 依赖 CSS 变更产生新 hash / 保留 3 个版本 / 静态投递与越界拒绝」全部通过，脚本已删除。
  - 执行期修正（已回写 `technical-plan.md` 与 `requirements.md`）：原设计「`<hash>.tmp/` + manifest 为唯一命中依据」不成立——hash 依赖完整依赖图，只能编译后确定。改为 `latest.json` 指针 + 版本内 `manifest.json`，hash 由「编译器名 + 构建指纹 + 全部 `inputs` hash」后置计算，临时目录用 `.tmp-<pid>-<time>/`；`CompileResult.files` 增加可选 `inputs`，`esbuildCompiler` 通过 `metafile.inputs` 提供，确保修改被 import 的 CSS 必失效。
  - 偏差记录：根 workspace 当前不可安装（`pulsar-plugin-host` 的 `@deepseek-ai/cordis@workspace:*` 只在局部 workspace 可解析），因此把 `../pulsar-plugin-client` 同时登记到 `pulsar-plugin-host/pnpm-workspace.yaml`，安装与类型检查走局部 workspace；包源码位置仍满足 Q4「根 `packages/`」。后续统一根 workspace 时移除该行。
- 11. 2026-10-23 17:40: Step 2 / Step 3 完成。新增 `runtime/node.ts`（退化实现，`import()` 后交给宿主）与 `runtime/dom.ts`（`mount` 用子节点差集追踪旧写法产物）；宿主改为 `runtime.activate()`——`load()`、`setEnabled()` 共用一条激活路径，适配器注册的路由与监听收进 `cleanups` 并随 fiber 回滚，激活失败立即回滚；删除 `bundleClient` / `handleClientBundle` / `clientBundles`；boot graph 输出 `client` 描述、`runtimes` 与合并去重的 `imports`；`renderPage()` 注入唯一 import map；`client/runtime.js` 增加 CSS 产物预挂载。
  - 结构调整（已回写 `technical-plan.md`）：内建适配器移到 `src/index.ts` 的 `createHost()` 组装根注册，`host.ts` 只 import `@pulsar/plugin-client/runtime` 契约、不 import 任何适配器实现——否则「宿主零构建」只是字面上成立。
  - 执行期修正：`node` 适配器返回的 `server` 只取 `apply` 会丢 `inject`，Cordis 报 `cannot get property "router" without inject`；改为 `adoptServer()` 同时补齐 `apply` 与 `inject`。
  - 验证：`pnpm --filter @pulsar/plugin-host typecheck` 通过；`verify` 两段全绿（装配 / 产物可取 / import map 唯一且先于 module / RPC / 禁用后路由 404 但产物留在磁盘 / 重新启用命中同一 URL / 重启后仍命中）。宿主 `dependencies` 已移除 `esbuild`，`@pulsar/plugin-client` 以 `workspace:*` 引入。
- 12. 2026-10-23 17:55: Step 4（P1）完成。`SlotRegistry` 的渲染记录记住所属注册项，`register` 返回的 dispose 会卸载该注册项在所有容器上的已渲染内容；`renderInto` 在挂载前清理同容器同插槽旧记录，使嵌套 `renderSlot` 也幂等；新增 `legacy()` 显式包装旧签名组件（渲染前清空容器），替代按参数个数推断。
  - 偏差：`apply` 返回值作为插件级 dispose 推迟到 Step 6——P1 阶段浏览器侧没有卸载入口，提前接线会留下死代码。
  - 验证：临时脚本验证「渲染幂等 / 注销清空 / onCleanup LIFO / legacy 清空容器」，通过后已删除；两包 typecheck 与宿主 verify 仍全绿。
- 13. 2026-10-23 18:20: Step 5（P2）完成。按用户决策直接 npm 引入 `solid-js` + `esbuild-plugin-solid`（完整 babel 链），新增 `runtime/solid.ts`：`Compiler` 用 `esbuildCompiler` + `solidPlugin()`，`external` 三个 solid 入口，`imports` 指向适配器自投递的 `/rt/vendor/solid/{solid,web,store}.js`（白名单，路径由 `createRequire` 解析）；新增 `runtime/mount.ts` 供 `dom` / `solid` 共用；新增 `examples/solid-plugin` 并在 profile 中以对象形态声明 `runtime: solid`；verify 增加 Solid 断言。
  - 验证：`typecheck` 两包通过；`verify` 全绿（dom + solid 共存、产物保留 `from "solid-js"`、vendor 可取、白名单外 404）。浏览器实测：页面同时渲染 hello 面板与 `solid count: 0`，连点 3 次计数递增，无模块解析或 404。
  - 执行期修正：bootstrap 原先忽略组件返回值，Solid 组件（`insert`/JSX 返回节点）永远不挂载；改为「Node 或 Node[] 一律 append」，并在 Solid 示例里用 `createRoot` + `insert` 显式建立 root 并把 dispose 交回宿主。
  - 约束收窄：`requirements.md` 的「框架源码唯一来源是 vendor」改为只约束 Cordis 框架层，允许框架运行时作为适配器 npm 依赖但必须走 import map 共享。
- 14. 2026-10-23 18:35: Step 6（P3）完成。`esbuildCompiler` 实现 `watch()`（`context()` + `onEnd`）；`RuntimeContext` 增加可选 `notify`；`BrowserRuntime` 在新 hash 出现时挂新路由并上报 `plugin/changed { id, rev, url, assets }`，旧路由与旧产物保留；宿主新增 `HostOptions.dev`（默认按 `NODE_ENV`），开发模式下先刷新 boot graph 再转发 SSE；浏览器 bootstrap 按插件记录 disposers 与 CSS link，收到事件后卸载 → 重新拉 boot graph → import 新 URL → 重新 apply → 重渲染。
  - 验证：临时脚本改动 `hello-plugin/client.ts` 后，2 秒内出现 `[dom] hello 重新编译为 <hash>`，`/api/boot` 的 `client.url` 更新，新产物含最新源码字符串，旧 URL 仍可访问；脚本已删除。
  - 文档：`docs/pulsar-plugin-host/plugin-guide.md` 更新 runtime 选择表、产物与热替换说明、调试入口与排错项。
