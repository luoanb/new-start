---
name: code-delivery
description: Implements an approved plan against effective requirements and code contracts, then runs project checks, source metrics, contract review, and a delivery summary. Use when the user asks to implement, fix, verify, or deliver code after requirements and contracts are approved.
---

# Code Delivery

## Authority

Before working, read and follow:

- [代码交付规则](../../rules/代码交付规则.md)
- [指标治理规则](../../rules/指标治理规则.md)
- [代码契约规则](../../rules/代码契约规则.md)
- [需求契约追溯规则](../../rules/需求契约追溯规则.md)
- [文档治理规则](../../rules/文档治理规则.md)

The rules decide entry conditions, order, failure handling, and acceptance. This Skill only sequences those rules and the project tools.

## Inputs

- an effective requirement folder and its affected-contract list;
- the related effective code contracts;
- an approved implementation plan;
- the project test command and any applicable project check commands;
- the optional metric override file at `.new-paradigm/governance/metrics/overrides.json`.

Do not infer missing product decisions, contract semantics, test commands, or metric thresholds.

## Workflow

1. **Check the implementation gate**
   - Apply DEL-001 and DEL-002.
   - Confirm the requirement, every affected contract, and the implementation plan are effective or explicitly approved for this scope.
   - Stop when a pending decision would change behavior, an important path, or acceptance.
   - Requirement approval does not approve a contract. Contract approval does not approve implementation beyond the plan.

2. **Implement only the approved scope**
   - Change code, tests, and configuration required by the plan.
   - Record adjacent issues without expanding the approved scope.
   - Keep the smallest extra fix needed to compile or test the approved change, and mark it in the delivery summary.

3. **Run project-native verification**
   - Apply DEL-004.
   - Run the existing format, lint, type-check, test, and pre-build commands that apply to the change.
   - Preserve the command, exit code, and relevant output.
   - A missing or failed command is not a pass.

4. **Run source metrics**
   - Apply DEL-005 and the metric governance rules.
   - For JavaScript or TypeScript projects, install the locked adapter dependencies and run:

```bash
pnpm install --frozen-lockfile --ignore-scripts
node src/cli.js --project <项目路径> --test-command "<项目测试命令>"
```

   The adapter directory is `.new-paradigm/governance/metrics/tools/javascript/`. Run `pnpm install` in that directory. Optional arguments are `--tsconfig`, `--include`, `--exclude`, `--overrides`, and `--output`.
   - Read thresholds only from the active metric definitions and a valid override file.
   - Do not add a command flag or edit a definition to make a failing metric pass.
   - Treat exit code `2` or any `error` status as an execution failure. Treat exit code `1` as a metric failure.

5. **Review contract consistency**
   - Apply DEL-006 after metrics.
   - Check capability boundaries, operation signatures, shared types, errors, capability relationships, collaboration dependencies, failure handling, consistency boundaries, guarantees, and applicable extensions.
   - When the change adds or edits a code contract, regenerate the overview from `.new-paradigm/governance/tools/contract-overview/`:

```bash
node src/cli.js --project <项目根目录>
```

   - Before the delivery summary, run `node src/cli.js --project <项目根目录> --check` in that directory.
   - Exit code `1` means the overview is stale or a collaboration relation is invalid. Exit code `2` means the check could not run.
   - Regenerate a stale overview, then continue the contract-body review. A consistent overview does not replace that review.
   - A passing metric does not prove contract consistency.

6. **Route and repair deviations**
   - Apply DEL-007 and DEL-008.
   - When the requirement, contract, rule, or metric definition is wrong, stop implementation, correct and reconfirm that document, then synchronize downstream assets.
   - When the documents are right, repair the implementation and rerun every affected project check and metric.
   - Stop on missing permissions, environment or tool failures, unresolved conflicts, or repeated repairs that still fail. Preserve the original evidence.

7. **Summarize and stop for human acceptance**
   - Apply DEL-009 and DEL-010.
   - Include change scope, requirement and contract basis, project-native results, metric results, contract evidence, approved exceptions, residual risks, and unfinished work.
   - Do not describe `fail` or `error` as passed.
   - Stop after the summary. Code, tests, metrics, and contract review only make the result ready for product acceptance.

## Outputs

- implementation and tests within the approved plan;
- project-native command evidence;
- the metric JSON result, including effective thresholds, status, evidence, and tool versions;
- a delivery summary;
- an explicit handoff for human product acceptance.
