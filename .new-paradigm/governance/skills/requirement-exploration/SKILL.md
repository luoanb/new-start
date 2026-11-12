---
name: requirement-exploration
description: Explores a vague or project-level product need before any requirement exists, producing a vision document when missing and a tightly bounded exploration folder with a human-owned decision log. Use when the user brings a goal, direction, or problem that cannot yet be stated as one identifiable functional change, or asks to brainstorm, scope, or converge such a need.
---

# Requirement Exploration

## Authority

Before working, read and follow:

- [需求探索规则](../../rules/需求探索规则.md)
- [文档治理规则](../../rules/文档治理规则.md)
- [愿景文档格式](references/愿景文档格式.md)
- [探索文档格式](references/探索文档格式.md)
- [决策日志格式](references/决策日志格式.md)

The rules are authoritative. The reference files only define this Skill's output format.

## Stance

The human audits; the AI drafts, challenges, and compresses. Every line costs the human attention, so:

- never state a conclusion the human has not reached — mark it `[推断]` or `[假设]`;
- never fill the 人的决策 column;
- never restate background the human already knows;
- when over any count or line limit, delete rather than compress wording.

## Inputs

- the effective vision at `functional/product/愿景.md`, or the material to write one;
- the product glossary at `functional/product/术语表.md`, when it exists;
- the user's statement of the need, in their own words;
- any evidence provided: data, user quotes, competitor notes, existing code constraints;
- existing explorations that may overlap.

## Workflow

1. **Check the vision**
   - If `functional/product/愿景.md` is missing or not 生效, stop and draft it with the vision format. Request approval before continuing. Do not start an exploration without it.

2. **Locate or create the exploration**
   - Check existing `functional/explorations/` for overlap; reuse rather than duplicate.
   - Otherwise propose `EXP-YYYYMMDD-NNN-<名称>` and create `探索.md`, `决策日志.md`, and `材料/` if raw input exists.
   - Store raw user input verbatim under `材料/`; it is not audited.

3. **Diverge: problems before solutions**
   - Write 第一章 only: who hurts, when, how, with evidence. Tag every statement.
   - Use canonical names from the product glossary. When a business term is undefined, ambiguous, or used inconsistently by the user and the materials, add it to 开放问题 as `术语：<名词>` and ask the human for its meaning. Do not define it yourself.
   - Produce questions and evidence gaps before any candidate solution.
   - Stop and present. Do not fill 第二章 in the same round.

4. **Diverge: candidates with objections**
   - Propose at most 3 mutually exclusive candidates. Each must carry its strongest objection; a candidate with no objection has not been analyzed.
   - Run a premortem: assume the direction failed six months later and name the most likely cause.
   - Check the decision log; do not reintroduce a rejected candidate unless its 重审条件 has triggered.
   - Record each candidate in `决策日志.md` with AI 建议 filled and 人的决策 empty.

5. **Converge**
   - After the human fills 人的决策, update 第三章: the smallest slice that can start now, or state 尚不具备 with the evidence that is missing.
   - Write 收敛条件 as observable evidence, not intent.

6. **Each update round**
   - Rewrite `探索.md` as a snapshot; move replaced content into the decision log.
   - Fill 本轮变更 (≤ 3 lines) first. The human reads that and the open questions, not the whole file.
   - Refresh 下次检视.
   - Verify: 3 chapters, ≤ 60 body lines, ≤ 5 assertions, ≤ 3 candidates, ≤ 3 open questions, every assertion tagged, no 验收 / 非目标 / contract paths.

7. **Exit**
   - When the human confirms the minimal slice, set 已收敛, list the planned requirement path, and stop. Writing the requirement belongs to the requirement-authoring Skill and needs separate authorization.
   - Hand over the terms the human clarified during exploration: list them under 最小可验证切片 as `待入术语表：<名词>`. The requirement-authoring Skill writes them into the product glossary; the exploration never becomes the glossary's source.
   - When 下次检视 passes without progress, ask the human to continue or 搁置; a 搁置 needs 放弃理由 and 重开条件.

## Outputs

- `functional/product/愿景.md` when it did not exist;
- `functional/explorations/EXP-YYYYMMDD-NNN-<名称>/探索.md`;
- `functional/explorations/EXP-YYYYMMDD-NNN-<名称>/决策日志.md`;
- optional `材料/` with verbatim raw input;
- on convergence, the planned requirement path for the next Skill.
