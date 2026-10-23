# Micro Spec：pulsar-plugin-host 更名为 pulsar-host

- 日期：2026-10-23
- 状态：已批准并执行
- 任务深度：fast

## 背景

包名 `@pulsar/plugin-host` 放在 `@pulsar/plugin-*` 命名空间下，会被读成“一个名为 host 的插件”；实际角色是宿主内核。按仓库既有的「命名存在的角色」原则，包名应只表达 Host。

## 变更

- 目录 `packages/pulsar-plugin-host` → `packages/pulsar-host`；包名 `@pulsar/plugin-host` → `@pulsar/host`。
- 运行时日志、页面标题、示例文案同步改名。
- 当前文档 `README.md`、`docs/pulsar-plugin-host/` → `docs/pulsar-host/` 同步。
- `@pulsar/plugin-client` 及其目录保持不变。
- 历史 spec 与 `docs/sdd-lab` 迭代记录不改写，保留当时的包名。

## 风险

- 局部 workspace 的 `node_modules` 元数据可能仍记录旧包名，必要时重新安装。
- 仓库外若有脚本按旧目录名或旧包名引用会失效；仓库内已检索，无其他引用。

## 验证

- `pnpm --filter @pulsar/host typecheck`
- `pnpm --filter @pulsar/host verify`

## 结果

- `pnpm typecheck`：通过（exit 0）。
- `pnpm verify`：两段全绿（装配 / Client 产物 / import map / RPC / 禁用回滚 / 重新启用命中；停止后重启命中）。
- 仓库内旧名引用仅剩本 spec 的记录性文字；历史 spec 与 `docs/sdd-lab` 保留旧名。
