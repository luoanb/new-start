import { randomUUID } from "node:crypto";
import { readFileSync } from "node:fs";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { loadDefinitions } from "./definitions.js";
import { inspectProject, resolveTsconfigs } from "./project.js";
import * as defaultCollectors from "./collectors.js";

const ADAPTER = { name: "@new-paradigm/javascript-metrics-adapter", version: "1.0.0" };
const METRIC_MAPPING = JSON.parse(readFileSync(new URL("../metric-mapping.json", import.meta.url), "utf8"));

export function compareValue(value, policy) {
  if (typeof value !== "number" || !Number.isFinite(value)) throw new Error("指标值必须是有限数字");
  if (policy.operator === "lte") return value <= policy.threshold;
  if (policy.operator === "gte") return value >= policy.threshold;
  throw new Error(`不支持的比较运算符：${policy.operator}`);
}

export function exitCodeFor(results) {
  if (results.some(({ status }) => status === "error")) return 2;
  if (results.some(({ status }) => status === "fail")) return 1;
  return 0;
}

function safeError(error) {
  return { name: error.name ?? "Error", message: error.message ?? String(error) };
}

function resultFromValue(definition, collected) {
  if (collected?.applicable === false) {
    return {
      id: definition.id,
      name: definition.name,
      value: null,
      unit: definition.measurement.unit,
      effectivePolicy: definition.effectivePolicy,
      severity: definition.effectivePolicy.severity,
      status: "not_applicable",
      evidence: { reason: collected.reason, override: definition.override },
      toolVersion: collected.toolVersion ?? null,
      error: null,
    };
  }
  const status = compareValue(collected.value, definition.effectivePolicy) ? "pass" : "fail";
  return {
    id: definition.id,
    name: definition.name,
    value: collected.value,
    unit: definition.measurement.unit,
    effectivePolicy: definition.effectivePolicy,
    severity: definition.effectivePolicy.severity,
    status,
    evidence: { ...collected.evidence, override: definition.override },
    toolVersion: collected.toolVersion ?? null,
    error: null,
  };
}

function errorResult(definition, error) {
  return {
    id: definition.id,
    name: definition.name,
    value: null,
    unit: definition.measurement.unit,
    effectivePolicy: definition.effectivePolicy,
    severity: definition.effectivePolicy.severity,
    status: "error",
    evidence: { override: definition.override },
    toolVersion: null,
    error: safeError(error),
  };
}

async function capture(definitions, ids, operation, select = (value) => value) {
  try {
    const value = await operation();
    return ids.map((id) => {
      const selected = value?.applicable === false ? value : select(value, id);
      return resultFromValue(definitions.get(id), {
        ...selected,
        toolVersion: selected?.toolVersion ?? value?.toolVersion,
      });
    });
  } catch (error) {
    return ids.map((id) => errorResult(definitions.get(id), error));
  }
}

export async function runAdapter(options, dependencies = {}) {
  const collectors = { ...defaultCollectors, ...dependencies.collectors };
  const startedAt = new Date().toISOString();
  const definitionsList = await loadDefinitions({
    definitionsDirectory: options.definitionsDirectory,
    overridesFile: options.overrides,
  });
  const definitions = new Map(definitionsList.map((definition) => [definition.id, definition]));
  const mappedIds = Object.keys(METRIC_MAPPING.mappings).sort();
  const definitionIds = definitionsList.map(({ id }) => id).sort();
  if (JSON.stringify(mappedIds) !== JSON.stringify(definitionIds)) {
    throw new Error(`指标映射与已加载定义不一致：${mappedIds.join(", ")}`);
  }
  const project = await inspectProject(options.project, {
    include: options.include,
    exclude: options.exclude,
  });
  const tsconfigs = await resolveTsconfigs(project, options.tsconfig);

  const [eslintResults, typeResults, testResults, coverageResults, duplicationResults, cycleResults, auditResults] = await Promise.all([
    capture(definitions, ["QLT-001", "QLT-007", "QLT-011"],
      () => collectors.collectEslint(project, definitions.get("QLT-007")),
      (value, id) => id === "QLT-001" ? value.static : id === "QLT-007" ? value.complexity : value.accessibility),
    capture(definitions, ["QLT-002"], () => collectors.collectTypescript(project, tsconfigs)),
    capture(definitions, ["QLT-003"], () => collectors.collectTests(project, options.testCommand)),
    capture(definitions, ["QLT-004", "QLT-005", "QLT-006"],
      () => collectors.collectCoverage(project, options.testCommand),
      (value, id) => id === "QLT-004" ? value.lines : id === "QLT-005" ? value.branches : value.functions),
    capture(definitions, ["QLT-008"], () => collectors.collectDuplication(project, definitions.get("QLT-008"))),
    capture(definitions, ["QLT-009"], () => collectors.collectCycles(project, tsconfigs)),
    capture(definitions, ["QLT-010"], () => collectors.collectAudit(project)),
  ]);

  for (const definition of definitionsList) {
    if (definition.override?.applicable === false) {
      const group = [eslintResults, typeResults, testResults, coverageResults, duplicationResults, cycleResults, auditResults];
      const current = group.flat().find((item) => item.id === definition.id);
      Object.assign(current, resultFromValue(definition, {
        applicable: false,
        reason: `项目覆盖将此指标标记为不适用：${definition.override.reason}`,
      }));
    }
  }

  const results = [
    ...eslintResults, ...typeResults, ...testResults, ...coverageResults,
    ...duplicationResults, ...cycleResults, ...auditResults,
  ].sort((left, right) => left.id.localeCompare(right.id));
  const finishedAt = new Date().toISOString();
  const report = {
    schemaVersion: "1.0",
    run: { id: randomUUID(), startedAt, finishedAt },
    project: {
      path: project.root,
      packageManager: project.packageManager,
      include: project.include,
      exclude: project.exclude,
      tsconfigs,
      workspaces: project.workspaces.map((workspace) => ({
        path: workspace.relativePath,
        frameworks: workspace.frameworks,
      })),
    },
    adapter: { ...ADAPTER, mapping: METRIC_MAPPING.mappings },
    runtime: { node: process.version, platform: process.platform },
    results,
    exitCode: exitCodeFor(results),
  };
  if (options.output) {
    const outputPath = path.resolve(options.output);
    await mkdir(path.dirname(outputPath), { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");
  }
  return report;
}

export { resultFromValue, errorResult };
