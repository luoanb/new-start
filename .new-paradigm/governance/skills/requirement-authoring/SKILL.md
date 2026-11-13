---
name: requirement-authoring
description: Establishes stable product terminology or clarifies a functional request and drafts or updates its requirement document, with an optional visual design document and affected-contract list. Use when the user asks to define shared business concepts, proposes a feature, changes product behavior, asks to define requirements, or needs an existing requirement clarified before contract or code work.
---

# Requirement Authoring

## Authority

Before working, read and follow:

- [需求契约追溯规则](../../rules/需求契约追溯规则.md)
- [文档治理规则](../../rules/文档治理规则.md)
- [需求文档格式](references/需求文档格式.md)
- [视觉设计文档格式](references/视觉设计文档格式.md)
- [产品术语表格式](references/产品术语表格式.md)

The rules are authoritative. The reference files only define this Skill's output format.

## Inputs

Collect only what the current request needs:

- user goal and business context;
- current behavior and desired change;
- scope and explicit non-goals;
- users or usage scenarios;
- constraints and acceptance expectations;
- business terms the request relies on, and whether `functional/product/术语表.md` already defines them;
- existing contracts likely to be affected;
- visual design input, when present.

Do not silently turn assumptions into decisions.

## Workflow

1. **Read context**
   - Read applicable effective governance rules and existing functional documents.
   - Identify whether this is a glossary-only task, a new requirement, or an update to an existing requirement folder.

2. **Set the task boundary**
   - For a glossary-only task, define only the confirmed, cross-requirement concepts in scope and do not create a requirement.
   - For a requirement task, define one identifiable functional change, propose `REQ-YYYYMMDD-NNN` and a short Chinese name, and use `functional/requirements/<迭代编号>-<名称>/`.

3. **Clarify product meaning**
   - Resolve goal, scope, behavior and acceptance ambiguities.
   - Ask the user only about choices that change product meaning, contract direction or important constraints.
   - Record unresolved choices as pending decisions.

4. **Align business terms**
   - Use canonical names from the effective `functional/product/术语表.md` for every business concept; replace deprecated aliases.
   - Product glossary entries may be created or changed independently when the product meaning has been confirmed; they do not require a requirement instance.
   - When requirement discussion discovers a concept that other requirements will reuse, draft and confirm its glossary entry before confirming the requirement. The two documents may be reviewed in one batch, but the requirement must not define or constrain the term.
   - A concept used only by this requirement may be explained in place.
   - Create and confirm the glossary before a requirement first relies on a shared concept.

5. **Decide whether visual design is needed**
   - Skip this step for a glossary-only task.
   - Create `视觉设计.md` only when page structure, interaction state or visual constraints need separate expression.
   - Move every behavior or acceptance statement back into `需求.md`.

6. **Identify affected contracts**
   - Skip this step for a glossary-only task.
   - List every affected capability contract and collaboration contract using a relative path.
   - For a contract not yet created, identify whether it is a planned capability contract or collaboration contract.
   - Do not add requirement references to contract documents.

7. **Draft the documents**
   - For a glossary-only task, add or update `functional/product/术语表.md` using the product glossary format reference.
   - For a requirement task, write `需求.md` using the requirement format reference and create `视觉设计.md` only when needed.
   - When requirement discussion discovers a shared concept, draft the prerequisite glossary change before the requirement.
   - Keep each requirement body at no more than seven top-level sections.

8. **Validate before confirmation**
   - Confirm glossary entries express stable business meaning without depending on a requirement or copying code structure.
   - For a requirement task, confirm it answers why, what, non-goals and completion conditions; visual design contains no behavior absent from the requirement; affected contracts are complete and locatable.
   - Confirm every cross-requirement business term resolves to an effective product glossary entry; otherwise stop before requirement confirmation.
   - If a key pending decision remains, stop before declaring the corresponding document complete.

9. **Request approval**
   - Present glossary changes for explicit confirmation independently of any requirement.
   - When glossary changes and a requirement are reviewed together, confirm the glossary first and the requirement second.
   - For a requirement task, present the requirement and optional visual design for explicit user confirmation.
   - Requirement approval does not approve any code contract or implementation.
   - Stop after the approved scope; do not continue into contract or code work without separate authorization.

## Outputs

- optional new or updated entries in `functional/product/术语表.md`
- `functional/requirements/<迭代编号>-<名称>/需求.md` for requirement tasks
- optional `functional/requirements/<迭代编号>-<名称>/视觉设计.md`
- a concise list of unresolved decisions, if any;
- a concise list of affected or planned contracts.
