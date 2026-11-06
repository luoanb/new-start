---
name: requirement-authoring
description: Clarifies a functional request and drafts or updates its requirement document, with an optional visual design document and affected-contract list. Use when the user proposes a feature, changes product behavior, asks to define requirements, or needs an existing requirement clarified before contract or code work.
---

# Requirement Authoring

## Authority

Before working, read and follow:

- [需求契约追溯规则](../../rules/需求契约追溯规则.md)
- [文档治理规则](../../rules/文档治理规则.md)
- [需求文档格式](references/需求文档格式.md)
- [视觉设计文档格式](references/视觉设计文档格式.md)

The rules are authoritative. The reference files only define this Skill's output format.

## Inputs

Collect only what the current request needs:

- user goal and business context;
- current behavior and desired change;
- scope and explicit non-goals;
- users or usage scenarios;
- constraints and acceptance expectations;
- existing contracts likely to be affected;
- visual design input, when present.

Do not silently turn assumptions into decisions.

## Workflow

1. **Read context**
   - Read applicable effective governance rules and existing functional documents.
   - Identify whether this is a new requirement or an update to an existing requirement folder.

2. **Set the requirement boundary**
   - Define one identifiable functional change.
   - Propose `REQ-YYYYMMDD-NNN` and a short Chinese name.
   - Use `functional/requirements/<迭代编号>-<名称>/`.

3. **Clarify product meaning**
   - Resolve goal, scope, behavior and acceptance ambiguities.
   - Ask the user only about choices that change product meaning, contract direction or important constraints.
   - Record unresolved choices as pending decisions.

4. **Decide whether visual design is needed**
   - Create `视觉设计.md` only when page structure, interaction state or visual constraints need separate expression.
   - Move every behavior or acceptance statement back into `需求.md`.

5. **Identify affected contracts**
   - List every affected capability contract and collaboration contract using a relative path.
   - For a contract not yet created, identify whether it is a planned capability contract or collaboration contract.
   - Do not add requirement references to contract documents.

6. **Draft the documents**
   - Write `需求.md` using the requirement format reference.
   - When needed, write `视觉设计.md` using the visual design format reference.
   - Keep the requirement body at no more than seven top-level sections.

7. **Validate before confirmation**
   - Confirm the requirement answers why, what, non-goals and completion conditions.
   - Confirm visual design contains no behavior absent from the requirement.
   - Confirm affected contracts are complete and locatable.
   - If a key pending decision remains, stop before declaring contracting complete.

8. **Request approval**
   - Present the requirement and optional visual design for explicit user confirmation.
   - Requirement approval does not approve any code contract or implementation.
   - Stop after the approved scope; do not continue into contract or code work without separate authorization.

## Outputs

- `functional/requirements/<迭代编号>-<名称>/需求.md`
- optional `functional/requirements/<迭代编号>-<名称>/视觉设计.md`
- a concise list of unresolved decisions, if any;
- a concise list of affected or planned contracts.
