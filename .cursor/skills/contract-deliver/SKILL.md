---
name: contract-deliver
description: 语义契约的代码交付：按 scope 实现、写守约证据、跑交付闸门、验收后归档。Use when 契约已批准、要交付或补证据、交付失败排错、验收后归档，或提到 contract:deliver、contract:drift、evidence.md。还在写契约时不要用本 skill。
---

# Contract Deliver

## 前置检查（任一不满足就停）

- 契约 `status` 是 `contracted`，且 `approved_version == contract_version`；否则请人先 `contract:approve`。
- 只改 `scope` 内路径；需要扩大范围 → 停，请人升版本并重新 approve。
- 改动触碰契约、边界或依赖方向 → 停，走 `pnpm law:new <slug>`。

状态机与闸门见 `.cursor/rules/project.mdc` §三。

## 步骤

1. **对齐**：读 `contract.md` 与 `scope`，列出本次要动的文件，并映射到对应 E/B 条目。
2. **实现**：只改 `scope` 内文件；遵守 B 声明的依赖方向；不新增无归属代码。
3. **写证据**：`evidence.md` 一行对应一条契约条目，四列完整；失败的写 `fail` 并修，**禁止**预先写 `pass`。
4. **交付**：`pnpm contract:deliver <id>`
   - 报越界 → 缩小改动或回退；确实要扩 `scope` → 升 `contract_version` + 请人重新 approve
   - 报证据或 lint → 修复后重跑；状态会自动回退到 `contracted`
5. **交人验收**：输出 `pnpm contract:accept <id>`，然后**等用户确认**，不得自行推进。
6. **归档**：用户验收后执行 `pnpm contract:archive <id>`（仅 `accepted` 可归档）。

## 禁止

- 修改 `contract.md`（除非用户明确要求升版本并重新批准）。
- 执行 `contract:approve` / `contract:accept`。
- 修改 `law/*.mdc`。
- 手工编辑或删除 `contract.lock.json`、绕过 `contract:drift`。

## drift 报错处理

| 现象 | 处理 |
|---|---|
| 合法改动导致漂移 | 升 `contract_version` → 请人 approve → 重新 deliver |
| 非法改动导致漂移 | 回滚代码 |

## 参考

- 模板：`docs/contracts/_template/evidence.md`
- 命令：`scripts/contract-deliver.mjs`、`scripts/contract-drift.mjs`、`scripts/contract-accept.mjs`、`scripts/contract-archive.mjs`
