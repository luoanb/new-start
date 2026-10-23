# Requirements / 需求文档: 插件前端基建现代化

## Restated Understanding / 需求复述

- 我理解当前需求是：`packages/pulsar-plugin-host` 的前端目前只有「装载」和「裸 DOM 拼接」，缺少组件模型、生命周期、类型契约、依赖共享与开发期反馈；需要把这些补齐到现代前端框架的体验水平。
- 当前核心目标是：引入**执行环境适配器（PluginRuntime）**作为前端能力的唯一承载者，它全权负责「编译 → 存放 → 投递 → 执行」；编译产物落盘到宿主约定的产物目录，不随插件销毁，运行阶段直接从该位置静态加载；适配器内部再按「编排 + 编译策略」分层——`BrowserRuntime` 只做产物流水线，编译由可替换的 `Compiler` 策略完成；框架（Solid / Vue / React）与原生 DOM 都作为该抽象下的适配器实现，插件作者只依赖统一类型包，不再手写上下文类型或直接拼 DOM。
- 当前边界是：宿主内核**不参与前端构建**，只保留装配、fiber 生命周期、RPC/Events 总线与 HTTP/路由/HTML 渲染；宿主对外只认两个产出（`server` 实例、`client` 执行描述）。P0 ~ P3（适配器包化与类型、生命周期、依赖共享与缓存、开发期单插件热替换）；三套框架适配器各出一套方案，最终选定一个作为官方一等公民。
- 暂不处理：P4 的 RPC 类型代码生成、Playwright 浏览器验收、pnpm 插件安装、Electron 桌面壳、SPA 路由、全局状态管理、SSR。

## Scope / 范围

- In:
  - 新建独立包 `packages/pulsar-plugin-client`，承载前端抽象、类型、Slot 注册表、RPC/事件/本地化/样式客户端。
  - 定义**执行环境适配器**契约 `PluginRuntime`：`activate(source, ctx) -> Activation`，适配器自己完成编译、存放、投递与执行，宿主只消费 `Activation`。
  - 适配器内部再分层：`BrowserRuntime` 是浏览器侧编排基类（查盘、原子落盘、静态投递、路由回滚、boot graph 交付），编译抽为可替换的 `Compiler` 策略（`fingerprint` / `compile` / 可选 `watch`），二者都不认识具体打包器；新增框架只需新增一个 `Compiler` 与一组 `mount/slot`。
  - 现有 `(container, view)` 手写 DOM 写法保留，收敛为内置的 `dom` 适配器，旧插件零改动可继续运行。
  - 宿主内核收缩：移除 `bundleClient` / esbuild 依赖与 `clientBundles` 缓存，改为向适配器索取 `client` 执行描述并写入 boot graph。
  - profile / boot graph 支持 runtime 选择与共享依赖；import map 由各适配器以 `imports` 结构化声明，宿主合并去重后输出全页唯一一张。
  - 编译产物落盘：宿主约定产物根目录（默认 `<cwd>/.pulsar/artifacts`，写入 `.gitignore`），适配器按 `runtime / plugin / hash` 写盘，运行阶段直接静态加载；产物不随插件卸载删除，每个插件保留最近 3 个 hash。
  - 产物模型（Q6 = P0）：P0 即采用「文件组 + hash」，`manifest.json`、`assets` 数组、`<hash>.tmp/` 原子 rename 一次做对。
  - 后端同步收编（Q5 = 是）：`node` 作为退化实现接入 `PluginRuntime`，`load()` 只走 `activate()`。
  - 包归属（Q4 = 根 workspace）：新包由根 `pnpm-workspace.yaml` 的 `packages/*` 覆盖，`plugin-host` 以 `workspace:*` 引用。
  - 生产环境内容哈希缓存（由适配器负责落盘与静态投递）；开发环境单插件热替换与 source map。
  - 同时给出 Solid / Vue / React 三套适配器方案，选定一套作为官方实现。
- Out:
  - 业务组件、设计系统组件库本身。
  - 跨插件的全局状态容器、路由、SSR。
  - 框架版本升级策略与插件市场分发。
  - RPC 类型自动生成（P4）。
  - 非官方框架适配器（Vue / React）：Q1 已选 Solid，这两套只保留方案说明，本期不实现、不引入 `@vue/compiler-sfc`。

## User Interaction / 用户交互

- 触发入口：
  - 插件作者：在 profile 中声明 `client.entry` 与 `client.runtime`，在入口文件调用 `defineClientPlugin()`。
  - 宿主维护者：启动宿主后浏览器访问根路径，由 boot graph 驱动装载。
  - 终端用户：看到由插件渲染的插槽内容，交互行为与现在一致。
- 用户操作路径：
  1. 插件作者写 `client.tsx` / `client.ts`，import `@pulsar/plugin-client` 类型与 API。
  2. profile 声明 runtime；宿主按名称找到已注册的执行环境适配器，把插件源码入口交给它。
  3. 适配器编译 → 存放产物 → 返回 `Activation.client`（URL + 资源列表）；宿主把该描述写进 boot graph。
  4. 浏览器拉取页面 → `<head>` 中唯一一张 import map 把共享依赖指向 vendor → 逐个 `import()` 各插件的 `client.url` → 注册槽 → 渲染。
- 系统反馈：
  - 插件声明的 runtime 未注册时，启动图标记该插件失败并在页面显示可读错误，不影响其他插件。
  - 旧写法（字符串 client、`(container, view)` 组件）继续工作，不出现弃用报错。
  - 框架共享依赖只加载一次；页面中可从 network 面板确认无重复实例。
  - 宿主重启后，同一插件命中已落盘的产物，`entry.js` 仍是同一 hash 路径，不产生新的编译。
- 状态变化：
  - 插件禁用 → boot graph 移除、适配器卸载组件、DOM 与监听器清理干净；适配器为该插件注册的路由与监听同步回滚，但已落盘的编译产物保留。
  - 插件重新启用 → 命中已落盘产物、不重新编译，重新挂载一次，页面不出现重复内容。
  - 开发期修改插件前端 → 只替换该插件模块，其他插件状态保留。
- 异常/边界交互：
  - 单个插件 apply 抛错 → 只影响该插件，错误面板展示可定位的堆栈（含 source map）。
  - 适配器激活失败 → 依赖该适配器的插件全部跳过，其余适配器不受影响。
  - 同一插件重复注册槽 id → 启动期立即报错并提示冲突 id。
  - 两个适配器把同一模块声明到不同 URL → 启动期立即报错，指出冲突模块名与两个来源。
- 不应发生的交互：
  - 同一框架 runtime 在页面中存在多份实例。
  - 页面出现多张 import map，或 import map 出现在 module script 之后。
  - 插件卸载后仍保留定时器、事件监听或 DOM 节点。
  - 插件作者需要手写 `ClientContext`、`SlotSpec` 等类型。

## Acceptance Criteria / 验收标准

- [ ] 插件入口只需 `import { defineClientPlugin } from '@pulsar/plugin-client'`，无需手写任何客户端上下文类型即可通过 `tsc`。
- [ ] `pnpm typecheck` 对宿主、客户端包与三套适配器均通过。
- [ ] 旧 profile 写法（`client: ../xxx/client.ts`）不改动即可运行，且走 `dom` 运行时。
- [ ] 新写法（`client: { entry, runtime: solid|vue|react }`）可正确装载对应适配器并渲染插槽。
- [ ] 同一 runtime 的框架依赖在页面中只加载一次；两个插件共用时不重复下载。
- [ ] 禁用插件后，其 DOM 节点、事件监听、定时器全部释放；重新启用后页面内容不重复。
- [ ] 生产模式下插件 bundle 使用内容哈希并命中强缓存；开发模式下修改插件前端后无需重启宿主即可生效。
- [ ] `pnpm verify` 覆盖：装配、boot graph、bundle 可取、RPC、禁用卸载、重新启用，并扩展到至少一个框架 runtime。
- [ ] `packages/pulsar-plugin-host/package.json` 的 `dependencies` 中不再出现 `esbuild`；宿主源码中不再有 `bundleClient` 与 `clientBundles` 缓存。
- [ ] 页面 HTML 中 `type="importmap"` 的 `<script>` 恰好一个，且位于首个 `type="module"` 之前；两个适配器声明同一模块映射到不同 URL 时启动期报错。
- [ ] 含 CSS 的插件入口（如 Vue SFC scoped style）产物不被静默丢弃，`Activation.client.assets` 能列出全部非 JS 产物。
- [ ] 适配器注册的路由与监听在插件卸载后随 fiber 回滚，重复卸载不报错；本次编译产物目录仍保留在磁盘。
- [ ] 产物按 `<artifactsDir>/<runtime>/<pluginId>/<hash>/` 落盘并带 `manifest.json`，插件目录下有 `latest.json` 指针；宿主重启后同一源码命中该目录，不重新编译。
- [ ] 修改被入口 import 的依赖（如 CSS）会触发重新编译并产生新 hash；只改入口之外的依赖不会复用旧产物。
- [ ] 编译写盘先落 `.tmp-<pid>-<time>/` 再原子 rename，中途失败不留下可被误加载的半成品目录。
- [ ] 每个 `<runtime>/<pluginId>/` 下最多保留 3 个 hash 目录；回收只在新增产物后触发，且不删除当前正在服务的 hash。
- [ ] 后端插件经 `node` 适配器 `activate()` 装载，返回 `server`，行为与改造前 `load()` 一致且不创建产物目录。
- [ ] `runtime/browser.ts` 与 `runtime/artifacts.ts` 中搜索不到任何打包器标识（`esbuild` / `outputFiles` / `loader` / `jsx`），`esbuild` 仅出现在 `runtime/compiler/esbuild.ts`。
- [ ] 新增一个框架适配器（含 `Compiler` 与 `mount/slot`）时，`browser.ts` 与 `artifacts.ts` 的 diff 为空。
- [ ] `docs/pulsar-plugin-host/plugin-guide.md` 同步更新新契约与三套 runtime 的接入方式。
- [ ] 官方 Solid 适配器：同一页面同时装载 `dom` 与 `solid` 插件；Solid 产物保留 `from "solid-js"`；`/rt/vendor/solid/*` 可共享取用；浏览器中 Solid 计数器可交互。
- [ ] 开发模式下修改 Client 源码，适配器自动重新编译为新 hash、boot graph 与页面插件同步更新，旧产物仍可访问。

## Constraints / 约束

- 业务约束：宿主仍然「只有机制，没有业务」；不得把 UI 组件库或业务能力塞进宿主。
- 技术约束：
  - 框架源码唯一来源仍是 `packages/pulsar-kernel/vendor`（只约束 Cordis 框架层），不复制、不修改。
  - 框架运行时依赖（如 `solid-js`）允许作为适配器包的 npm `dependencies`，但必须通过 import map 共享，插件产物不得内联框架运行时。
  - 插件贡献必须走 Cordis `ctx.effect`，卸载即整体回滚。
  - 运行时必须以适配器（`PluginRuntime`）方式提供，而不是把某个框架硬编码进宿主；构建配置（loader / jsx / external / define）属于适配器的私有实现，不得回流到宿主。
  - 宿主内核零构建：宿主不 import 任何打包器，只接收 `Activation`；编译默认发生在宿主进程内由适配器完成，不引入必须常驻的独立构建服务。
  - 适配器内部也零构建耦合：`BrowserRuntime` 与 `artifacts.ts` 不得 import 任何打包器，编译只能通过注入的 `Compiler` 策略完成。
  - import map 必须是全页唯一一张、由宿主合并各适配器 `imports` 后输出，且出现在首个 module script 之前。
  - 适配器占用的 HTTP 路由与监听必须挂在插件 fiber 上，随卸载回滚。
  - 编译产物必须写入宿主约定的产物目录（`artifactsDir`），不得随插件卸载删除，也不得写到该目录之外。
- 兼容性约束：现有 `hello-plugin`、`profile.yml`、`verify.ts` 的行为不能回退；旧组件契约至少保留一个大版本周期。

## Open Questions / 开放问题

- [x] Q1 官方一等公民 runtime 选哪一个（Solid / Vue / React）？—— A Solid。
- [x] Q2 是否允许同一页面同时存在多种 runtime（多框架共存），还是每份 profile 只允许一种？—— 允许共存，宿主合并 `imports` 并去重，冲突抛错。
- [x] Q3 若选 Vue，是否接受为编译器引入 Vite 或等价 SFC 编译链？—— 不适用（Q1 选 Solid），本期不引入 SFC 编译链。
- [x] Q4 `pulsar-plugin-client` 作为独立包后，包管理归属按根 workspace 还是 plugin-host 局部 workspace 处理？—— 根 workspace。
- [x] Q5 后端（node）是否也统一收编为 `PluginRuntime` 适配器，即 `activate` 返回 `server` 实例成为唯一后端装载路径？—— 是，`node` 作为退化实现。
- [x] Q6 产物模型（多 `outputFiles`、`assets` 列表）是在 P0 一次性做对，还是先沿用单文件模型、留到 P2 随框架适配器一起改？—— P0 一次性做对。
- [x] Q7 产物目录的默认位置、是否纳入版本控制、以及旧 hash 的清理方式？—— 固定 `<cwd>/.pulsar/artifacts`（`.gitignore` 忽略），每个插件保留最近 3 个 hash，自动回收。

## Requirement Decisions / 需求决策

- 2026-10-23 14:22:
  - 决策：三套框架适配器（Solid / Vue / React）各出一套方案，最终选一套作为官方实现。
  - 原因：用户要求「三个出三套方案」，需要在同一基座下做可比选型。
- 2026-10-23 14:22:
  - 决策：本期范围锁定 P0 ~ P3。
  - 原因：用户确认；P4（类型生成、Playwright 验收）留待后续迭代。
- 2026-10-23 14:22:
  - 决策：前端运行时本身以插件方式提供，旧的 `(container, view)` 写法抽象为一个运行时插件（`dom`）。
  - 原因：用户明确「前端运行时本质提供构建+运行的抽象，旧写法可以抽象为一个运行时插件」。
- 2026-10-23 14:22:
  - 决策：运行时与类型放在新建独立包中。
  - 原因：插件作者需要能正常 `import` 类型，宿主只负责下发单一 ESM。
- 2026-10-23 15:10:
  - 决策：该抽象正式命名为「执行环境适配器（`PluginRuntime`）」，不再使用 `RuntimeAdapter`。
  - 原因：用户明确认可该命名；「执行环境」比「运行时」更准确地覆盖编译与执行两端。
- 2026-10-23 15:10:
  - 决策：适配器全权负责「编译 → 存放 → 投递 → 执行」，宿主内核零构建。
  - 原因：用户原话「Host 不用知道前端代码怎么构建，怎么运行，它只需要拿到一个插件的代码，然后把代码交付给运行时插件即可」。构建配置与框架强相关，放在宿主会把一个框架的接入面切成两半。
- 2026-10-23 15:10:
  - 决策：适配器之间按四层关系组织——编排复用 + 策略替换（browser 家族共用 `BrowserRuntime` 流水线，编译由各自 `Compiler` 注入）、平行（node 与 browser 互不依赖）、共存耦合（共享 import map 与 `@pulsar/plugin-client` 单例）、降级兼容（`dom` 适配器承载旧 `(container, view)` 签名）。
  - 原因：用户指出原方案只回答了 host ↔ adapter，没有回答 adapter ↔ adapter。
  - 修订：第二层关系在 16:00 由「继承与特化」改为「编排复用 + 策略替换」，见下方 16:00 决策。
- 2026-10-23 15:10:
  - 决策：import map 是唯一的结构化耦合点，必须由适配器以 `imports` 字段声明、宿主合并去重、全页唯一输出。
  - 原因：import map 是浏览器原生 JSON，必须在首个 module script 之前出现，多张在 Safari 不可靠；让宿主解析并合并原始 HTML 片段比直接收结构化数据更脆弱。
- 2026-10-23 15:10:
  - 决策：承认现有 esbuild 产物模型「只取 `outputFiles[0]`」是缺陷，含 CSS 的入口会静默丢产物；修复时机待 Q6 决定。
  - 原因：Vue SFC scoped style 必产出独立 CSS 文件，沿用单文件模型会直接破坏样式。
- 2026-10-23 15:40:
  - 决策：编译产出必须落盘存储，且**不随插件销毁**；宿主约定一个产物目录，运行阶段直接从该位置加载编译结果。
  - 原因：用户明确「编译产出需要存储（不用随插件销毁），可以考虑约定一个存储目录，运行阶段直接从该位置加载编译结果」。这样卸载/重新启用、宿主重启都不必重编译，产物也可被静态托管与排查。
  - 边界澄清：挂 fiber 回滚的是**路由与监听**，不是产物；产物目录的清理策略单列为 Q7。
- 2026-10-23 16:00:
  - 决策：编译从 `BrowserRuntime` 基类中抽出，定义为独立策略接口 `Compiler`（`fingerprint` / `compile` / 可选 `watch`），基类只保留查盘、原子落盘、静态投递、路由回滚与 boot graph 交付。
  - 原因：用户指出「你这样分离又没有完全分离」——把编译放在基类里，等于假设所有框架编译过程一致。事实上 `dom` 只需剥离 TS、`react` 用 esbuild JSX、`solid` 的 esbuild JSX 只是近似（真正优化在 `babel-preset-solid`）、`vue` 必须用 `@vue/compiler-sfc` 拆块并产出 scoped CSS，无法用一份配置描述。
  - 编译结果形态固定为两种：`{ kind: 'files', entry, assets }`（本地编译，已写入 `outDir`）与 `{ kind: 'external', url, assets? }`（外部构建，如 Vite dev server）。
  - 复用方式由继承改为组合：`esbuildCompiler(options)` 是组合件，`dom` / `react` / `solid` 各传不同 options，`vue` 写独立 `Compiler` 或走 `external`。
  - 包导出同步分三段：`.`（纯浏览器）、`./runtime`（零打包器依赖）、`./runtime/esbuild`（唯一可引入 esbuild 的入口）。
- 2026-10-23 16:20（Q1 ~ Q7 关闭）:
  - 决策：官方一等公民适配器为 **A Solid**；Vue / React 只保留方案说明，本期不实现（Q1）。
  - 决策：同一页面允许多适配器共存，宿主合并 `imports` 并去重，同键不同 URL 冲突时抛错（Q2）。
  - 决策：Q3 不适用——选 Solid 后本期不引入 `@vue/compiler-sfc`；未来接 Vue 走 `Compiler` 策略或 `external`，不改基类。
  - 决策：新包归属根 workspace，`@pulsar/plugin-host` 以 `workspace:*` 引用（Q4）。
  - 偏差（执行期，16:52）：根 workspace 当前无法安装——`pulsar-plugin-host` 的 `@deepseek-ai/cordis@workspace:*` 只能在其局部 workspace 解析。因此新包同时登记到 `pulsar-plugin-host/pnpm-workspace.yaml`，安装与类型检查走局部 workspace；源码位置仍在根 `packages/`，符合 Q4 的归属意图，待根 workspace 可安装后移除该行。
  - 决策：后端统一收编为 `PluginRuntime`，`node` 是退化实现（编译恒等、不落盘、不投递、执行即 `import()`），`load()` 只走 `activate()`（Q5）。
  - 决策：产物模型在 P0 一次做对——`latest.json` 指针、版本内 `manifest.json`、`assets` 数组、`.tmp-<pid>-<time>/` 原子 rename 全部进 P0（Q6）。
  - 决策：产物根目录固定 `<cwd>/.pulsar/artifacts` 并写入 `.gitignore`；每个 `<runtime>/<pluginId>/` 保留最近 3 个 hash，`prune()` 在新产物 rename 后触发并跳过当前服务的 hash（Q7）。
- 2026-10-23 18:10（执行期补充）:
  - 决策：`solid-js` 与 `esbuild-plugin-solid` 作为 `@pulsar/plugin-client` 的 `dependencies` 直接 npm 安装，不收编进 `pulsar-kernel/vendor`；接受 `babel-preset-solid` 完整编译链。
  - 原因：用户选择「直接 npm 依赖」+「接受完整链」。vendor 目前只承载 Cordis 框架层，收编 Solid 会引入整条 babel 链并与上游同步流程耦合。
  - 影响：`packages-conventions` / `project.mdc` 中「框架源码唯一来源是 vendor」的表述需收窄为「Cordis 框架层」；已在 `technical-plan.md` Step 5 记录。
  - 决策：`solid-js` 通过 import map 共享，由适配器从自身依赖的 `dist` 投递 `/rt/vendor/solid/*`（白名单三个文件），插件产物保留 `from "solid-js"`，保证页面只有一份实例。

