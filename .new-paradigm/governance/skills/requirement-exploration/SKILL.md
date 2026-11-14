---
name: requirement-exploration
description: Explores a vague or project-level product need before any requirement exists, clarifying the user's fuzzy input and appending it to upstream documents (vision, glossary, product definition) to provide clear input for the main chain. Use when the user brings a goal, direction, or problem that cannot yet be stated as one identifiable functional change, or asks to brainstorm, scope, or converge such a need.
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

The human audits and decides; the AI clarifies fuzzy input and proposes how to append it to upstream documents. Every line costs the human attention, so:

- never state a conclusion the human has not reached — mark it `[推断]` or `[假设]`;
- never fill the 人的决策 column;
- never write into 第五章 确认的补充 without executing the append/replace and receiving confirmation;
- write in plain spoken language: 探索.md must read like someone explaining it to the user, not like internal notes; coined terms (落位、收口、整批、粒度、可裁决正文) stay out unless a one-line plain explanation follows;
- never restate background the human already knows;
- when over any count or line limit, delete rather than compress wording.

## Inputs

- the effective vision at `functional/product/愿景.md`, or the material to write one;
- the effective product glossary at `functional/product/术语表.md`, when it exists;
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

3. **Restate the need, then frame the proposition**
   - Write 第一章 我对你需求的理解 first: in plain spoken language, restate what the user actually wants as you understand it after your own thinking. Each line must be a judgment the user can nod at or reject; ≤5 lines; no tags — this chapter asks for confirmation, not evidence. Do not quote the materials.
   - Then write 第二章 探索命题: state the user's fuzzy need and how the current upstream documents (vision, glossary, product definition) express it — whether insufficient or unclear. Tag every assertion.
   - Use canonical names from the effective product glossary. When a business term is undefined, ambiguous, or used inconsistently by the user and the materials, add it to 开放问题 as `术语：<名词>` and ask the human for its meaning. Do not define it yourself.
   - Identify evidence gaps before proposing any append plan.
   - Stop and present. Do not fill 第三章 in the same round.

4. **Propose append plans**
   - Propose at most 3 mutually exclusive plans to clarify and append the user's need to upstream documents. Each plan must specify 方案名（A/B/C）、承诺会变成什么（结果，不是动作）、改哪里（document § chapter）and its strongest objection in the form “如果选它，<谁/何时>会遇到<后果>”; a plan with no objection has not been analyzed.
   - Plans are usually append (adding new content); only when existing content is demonstrably wrong, propose replace.
   - Check the decision log; do not reintroduce a rejected plan unless its 重审条件 has triggered.
   - Keep current plans in 第三章 and append each to `决策日志.md` with AI 建议 filled and 人的决策 empty.

5. **Form the append recommendation**
   - Update 第四章 with your recommended append plan: which one, why, based on which evidence, and within what risk boundary.
   - A selected plan is not itself a recommendation. State why this plan best clarifies the user's fuzzy need.
   - When evidence is insufficient, write `尚未形成：缺 <证据>` instead of inventing a recommendation.
   - Ask the human to confirm the plan. Only a confirmed plan may be executed.

6. **Execute the append**
   - After the human confirms an append plan, execute the append on the target upstream document (vision, glossary, or product definition).
   - The append itself needs independent confirmation as a change to that upstream document.
   - After the upstream document append is confirmed, record in 第五章: `[补充] <文档名>已补充<内容要点>，见[路径]（YYYY-MM-DD 确认）` or `[修正] <文档名>已修正<内容要点>，见[路径]（YYYY-MM-DD 确认）`. Write a one-line summary of what was appended/corrected; reference the path.
   - Remove the resolved gap from 现有认知 or 开放问题. For a business term, the glossary append is recorded separately.
   - Keep ≤ 5 append/correction records in 第五章. Reaching the cap signals the exploration should converge.

7. **Each update round**
   - Rewrite `探索.md` as the current snapshot, except 第五章, which is append-only. Append plan decisions to the decision log; never rewrite existing log entries.
   - Fill 本轮变更 (≤ 3 lines) first. The human reads that and the open questions, not the whole file.
   - Refresh 下次检视.
   - Verify: 5 chapters in 理解—命题—方案—建议—补充 order, ≤ 60 body lines, ≤ 5 understanding statements, ≤ 5 current-understanding assertions, ≤ 3 append plans, ≤ 3 open questions, ≤ 5 confirmed appends/corrections each with a date and one-line summary, every assertion outside 第一章 tagged, one direct recommendation, and no 验收 / 非目标 / requirement or contract paths / future-action instructions.

8. **Exit**
   - When the human confirms an append plan and the append is executed and confirmed, record it in 第五章 and stop. Do not decide whether or how to turn the appended upstream documents into a requirement.
   - As soon as the human clarifies a cross-requirement term, propose an independent product glossary append. The glossary append needs separate confirmation and does not wait for exploration convergence.
   - After the glossary entry becomes effective, use it as the upstream definition in subsequent exploration and requirement work. The exploration only records where the term was discovered; it never becomes the glossary's authority.
   - When 下次检视 passes without progress, ask the human to continue or 搁置; a 搁置 needs 放弃理由 and 重开条件.
   - When enough appends are confirmed (typically 1-2 core appends), set 已收敛 and freeze the body. The appended upstream documents now provide clear input for the main chain.

## Outputs

- `functional/product/愿景.md` when it did not exist;
- appended upstream documents (vision, glossary, product definition) with independent confirmations;
- `functional/explorations/EXP-YYYYMMDD-NNN-<名称>/探索.md`;
- `functional/explorations/EXP-YYYYMMDD-NNN-<名称>/决策日志.md`;
- optional `材料/` with verbatim raw input.
