---
name: contract-author
description: 需求立项与语义契约撰写：建骨架、填实体/状态/不变量/边界/验收条目、自检后交人审。Use when 用户提需求、要写或改语义契约、准备评审，或提到 contract:new、contract:review、docs/contracts。交付实现不要用本 skill。
---

# Contract Author

## 定位

只负责「意图 → 可评审的语义契约」，写完即停。代码交付由 `contract-deliver` 负责。

状态机、闸门与两区边界见 `.cursor/rules/project.mdc` §三，本 skill 只读引用，不修改立法。

## 前置判断

- 需求触碰契约、边界、依赖方向或跨实体重构（L2/L3）→ **停下**，提示 `pnpm law:new <slug>`，本 skill 不处理。
- 需求本身没定清楚 → 先复述并问清，再立项；条目留空比编造好。

## 步骤

1. **复述**：本次要解决什么、明确不解决什么、授权层级（默认 L1）。
2. **立项**：

   ```bash
   pnpm contract:new <slug> --auth L1 --scope <glob> [--scope ...]
   ```

   `scope` 只写本次允许改动的路径；拿不准就收窄，扩大范围要升版本重审。

3. **写契约**：按 `docs/contracts/<id>/contract.md` 的五类条目填。

   | 类 | 写什么 |
   |---|---|
   | E | 实体职责 + 依赖方向（允许引用谁、禁止引用谁） |
   | S | 状态迁移：触发条件 + 非法迁移必须被拒绝 |
   | I | 永远成立的条件 |
   | B | 输入/输出契约、依赖边界、禁止跨层 |
   | A | 可观察的验收事实 |

   每条 = 陈述 + 判定方式。判定方式必须是可执行的东西（测试、指标、类型、lint、命令），不接受「人工确认」。

4. **记录歧义**：收敛不了的点写进「6. 歧义与未决」，不要替用户做语义决定。
5. **自检**：`pnpm contract:lint`（单个：`node scripts/contract-lint.mjs --dir docs/contracts/<id>`），必须通过。
6. **交人审**：输出 `pnpm contract:review <id>` 与歧义清单，然后停止。

## 禁止

- 写实现代码、改 `scope` 外的文件。
- 执行 `contract:approve` / `contract:accept`（只有人能执行）。
- 为了让 lint 通过而删条目、降级判定方式或写「人工确认」。

## 失败处理

| 现象 | 处理 |
|---|---|
| lint 报占位符 / 缺类别 | 补契约内容，不改 lint |
| 判定方式写不出来 | 回需求澄清；写不出说明还没想清楚 |
| 发现要改边界或依赖方向 | 停，转 `pnpm law:new <slug>` |

## 参考

- 模板：`docs/contracts/_template/contract.md`
- 命令：`scripts/contract-new.mjs`、`scripts/contract-lint.mjs`
