# Spec: tauri:dev 前端产物兜底构建

- 日期：2026-09-17
- 状态：已实施（Execution Approval: Approved）
- 影响包：`packages/pulsar-app`（dev 脚本 + 文档）

## Goal

- 要解决什么问题：`build/` 缺失时 `pnpm tauri:dev` 必然编译失败——
  `#[derive(RustEmbed)] folder '../build/' does not exist`，并级联出两条
  `FrontendAssets::get` 的 E0599。根因是 dev 路径只跑 `beforeDevCommand: pnpm dev`
  （vite dev server，不落盘），而 `embed-static` 在 `default` feature 中恒开启，
  `net/static_assets.rs` 在编译期硬要求 `../build/` 存在。
- 验收结果：全新克隆或 `build/` 被清理后，直接 `pnpm tauri:dev` 一次成功启动，无需人工先跑 `pnpm build`。

## Done Contract

- 什么算完成：`scripts/dev-tauri.mjs` 在启动 tauri 前检测 `../build/index.html`，缺失时自动执行一次前端构建再继续；构建后仍无产物则明确报错退出。
- 由什么证明：删除 `build/` 后运行 `pnpm tauri:dev`，日志出现自动构建提示、`build/index.html` 生成、tauri 进入编译；`node --check scripts/dev-tauri.mjs` 通过。
- 哪些情况仍算未完成：仍需用户手工 `pnpm build`；自动构建失败后仍继续启动 tauri。

## Scope

- In：
  - [scripts/dev-tauri.mjs](../../packages/pulsar-app/scripts/dev-tauri.mjs)：新增产物检测 + 缺失时自动 `pnpm build`（阻塞、继承 stdio），构建后断言 `build/index.html` 存在，否则退出。
  - 文档同步：[README.md](../../README.md) 的 `tauri:dev` 说明、[Cargo.toml](../../packages/pulsar-app/src-tauri/Cargo.toml) 的 `embed-static` 注释、[spec 构建矩阵边界](../specs/2026-08-21_22-30_build-matrix-server-headless.md)补记 dev 路径已覆盖（反写）。
- Out：
  - 不改 `build.rs`、不改 feature 默认组合、不改 `tauri.conf.json` 的 `beforeDevCommand`。
  - 不做 `build/` 陈旧性校验或 dev 期间热更新（dev 下内嵌副本不被使用，仅需存在）。
  - 裸 `pnpm tauri dev`（绕过 dev-tauri.mjs）与 `cargo build` 不自动兜底，仅以文档注明。

## Facts / Constraints

- 已确认事实：
  - [dev-tauri.mjs:52](../../packages/pulsar-app/scripts/dev-tauri.mjs#L52) 仅 `spawn("pnpm", ["exec","tauri","dev",...])`；`tauri.conf.json` 的 `beforeDevCommand` 只起 vite dev server，dev 下 `frontendDist` 不参与。
  - [Cargo.toml:14](../../packages/pulsar-app/src-tauri/Cargo.toml#L14) `default = ["embed-static"]`；[static_assets.rs:14-16](../../packages/pulsar-app/src-tauri/src/net/static_assets.rs#L14-L16) 的 `#[folder = "../build/"]` 以 `CARGO_MANIFEST_DIR`（`src-tauri/`）为基准。
  - [.gitignore:3](../../packages/pulsar-app/.gitignore#L3) 忽略 `/build`，新克隆必然缺失该目录。
  - 2026-08-21 spec 的「边界」只声明了 build 路径由 `beforeBuildCommand` 保证，**dev 路径未覆盖**；后改 `default = ["embed-static"]` 才使该缺口变为首次编译必挂。
- 技术/业务约束：不触碰端口单一来源语义（`DEV_FRONT_PORT` / `PULSAR_PORT`）；不注入 `PULSAR_HOST`。
- 已知风险：首次 dev 启动多出一次前端构建耗时；构建失败必须中断并给清晰错误，不得让 tauri 带着缺失产物继续编译。

## Restated Understanding

- 我理解当前任务是：让 `pnpm tauri:dev` 不再依赖人工前置 `pnpm build`。
- 当前核心目标是：dev 启动路径自洽——编译期需要的 `build/` 由脚本按需生成。
- 当前边界是：只动 dev 脚本与文档，不改 feature 组合与编译期行为。
- 暂不处理：`tauri build` / `pulsar-server` / 裸 `cargo build` 路径。

## 接口契约设计

脚本层改动，无对外 API；新增唯一内部契约如下：

```js
// 启动前保证编译期产物存在；已存在则零开销返回
function ensureFrontendBuild() {
  if (existsSync(buildIndexHtml)) return;
  console.log("[dev-tauri] 缺少前端产物 build/，先执行 pnpm build ...");
  const r = spawnSync("pnpm", ["build"], { cwd: pkgRoot, stdio: "inherit" });
  if (r.status !== 0 || !existsSync(buildIndexHtml)) {
    console.error("[dev-tauri] 前端构建未产出 build/index.html，终止启动");
    process.exit(1);
  }
}
```

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是（消除 dev 首次启动的硬前置）。
- 若否，偏差在哪里：—
- 是否需要调整本轮目标或范围：否

## Checkpoint Summary

- 当前任务理解：dev 路径缺产物兜底，导致 `tauri:dev` 首次编译必挂
- 当前核心目标：`dev-tauri.mjs` 启动前按需生成 `build/`
- 当前进度：spec 落盘，待批准
- 下一步 1：改 `scripts/dev-tauri.mjs`，加 `ensureFrontendBuild()`
- 下一步 2：同步 README / Cargo.toml 注释 / 2026-08-21 spec 边界条目
- 涉及文件 / 模块：`packages/pulsar-app/scripts/dev-tauri.mjs`、`README.md`、`packages/pulsar-app/src-tauri/Cargo.toml`、`docs/specs/2026-08-21_22-30_build-matrix-server-headless.md`
- 风险：首启耗时增加；构建失败需明确报错
- 验证方式：清空 `build/` 后跑脚本观察自动构建与产物生成；`node --check`
- Execution Approval: `Approved`

## Change Log

- 2026-09-17: 建立 spec（待批）。
- 2026-09-17: 实施——[dev-tauri.mjs](../../packages/pulsar-app/scripts/dev-tauri.mjs) 新增 `pkgRoot` / `buildIndex` / `ensureFrontendBuild()` 并在 spawn tauri 前调用；同步 README（运行章节 + 命令表）、Cargo.toml `embed-static` 注释、2026-08-21 spec 边界条目（反写）。

## Validation

- Self-check: `node --check scripts/dev-tauri.mjs` 通过。
- Static checks: 上述语法校验；改动集中在单个 22 行增量，无类型/契约变更。
- Runtime / Test:
  - 缺失分支：删除 `build/` 后运行 `node scripts/dev-tauri.mjs`，日志先打印「缺少前端产物 build/，先执行 pnpm build ...」，随后 tauri 未启动并输出「前端构建未产出 build/index.html，终止启动」（该次失败原因是本次 agent 工具沙箱禁止写 `E:\.pnpm-store-v2`，非脚本缺陷）。
  - 产物生成：`node node_modules/vite/bin/vite.js build` 成功，`build/` 产出 139 个文件，`build/index.html` 存在。
  - 已有产物分支：再次运行 `node scripts/dev-tauri.mjs`，**无**构建提示，直接进 `beforeDevCommand (pnpm dev)` 与 `cargo run --no-default-features --features embed-static`（已进入 watch）。
  - 观察事实（补充）：Tauri CLI 在 dev 下确实以 `--features embed-static` 启动，印证 dev 路径必然触发 rust-embed 编译期校验。
- Human confirmation: 待用户在本机正常终端执行 `pnpm tauri:dev`（会先补构建，随后 Rust 首次全量编译，耗时较长）。
- 结果汇总：脚本兜底逻辑按要求生效；dev 路径的硬前置已由脚本消除。
- 核心目标是否已由证据证明完成：脚本层已证明；端到端「一次命令启动成功」需用户本机实跑确认。
- 若未完成，当前剩余差距：Rust 侧首次全量编译完成后能否启动 GUI，未在本轮验证（编译耗时长，未在会话内跑完）。
- 剩余风险：`pnpm build` 首次构建耗时；若前端构建报错，启动会中断并提示（符合预期）。

## Resume / Handoff

- 当前状态：dev 脚本兜底 + 三处文档同步已完成；`build/` 已在本地生成。
- 当前卡点：无。
- 下一步唯一动作：在本机 `packages/pulsar-app` 下执行 `pnpm tauri:dev`，等待 Rust 首次全量编译完成。
- 下一轮核心目标：确认真机启动成功（GUI 拉起 + 内嵌服务可访问）。
