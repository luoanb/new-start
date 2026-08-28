# 一键发布脚本（release.mjs）

- 日期：2026-08-28
- 范围：`packages/pulsar-app/scripts/release.mjs` + `package.json` 入口 `pnpm release`
- 状态：已完成

## 需求

发布新版本时，一键完成：累加版本号 → 打 tag → 推送 release（触发 CI 构建发布）。支持指定加大（major）/中（minor）/小（patch）版本。

## 实现

- 用法：`pnpm release [patch|minor|major]`（在 `packages/pulsar-app` 下；默认 patch；支持中文别名 小/中/大）。
- 版本号同步四处：`package.json` / `tauri.conf.json` / `Cargo.toml` / `Cargo.lock`（仅 `[[package]] name = "pulsar-app"` 块，避免误改同版本号依赖）。
- 以 `package.json` 版本为基准，启动前校验四处一致，不一致则中止并提示。
- 前置校验：当前分支必须是 `main`，工作区无未提交改动。
- 流程：
  1. 累加版本号并写回四处；
  2. `git add` 四处 → commit `chore: bump version to X.Y.Z` → push origin main；
  3. `checkout release` → `merge --ff-only main` → push origin release（触发 `.github/workflows/publish-pulsar-app.yml`）；
  4. 打 annotated tag `pulsar-vX.Y.Z` → push origin tag；
  5. 回到 main。
- 任何一步失败：切回 main，输出错误提示（版本改动可能已产生，需手工回滚）。

## 验证

- `node --check` 语法通过。
- Cargo.lock 正则替换单测：精确命中 pulsar-app 包块，替换后全局仅 1 处变化。
- 四处版本一致性校验：当前 `0.2.1` 一致。
