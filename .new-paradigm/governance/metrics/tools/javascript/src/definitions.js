import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";

const EXPECTED_IDS = Array.from({ length: 11 }, (_, index) => `QLT-${String(index + 1).padStart(3, "0")}`);
const DEFAULT_DEFINITIONS_DIRECTORY = fileURLToPath(
  new URL("../../../definitions/", import.meta.url),
);
const METRIC_SCHEMA_FILE = path.join(DEFAULT_DEFINITIONS_DIRECTORY, "metric-definition.schema.json");
const OVERRIDES_SCHEMA_FILE = path.join(DEFAULT_DEFINITIONS_DIRECTORY, "overrides.schema.json");

async function readJson(file) {
  try {
    return JSON.parse(await readFile(file, "utf8"));
  } catch (error) {
    throw new Error(`无法读取 JSON：${file}：${error.message}`, { cause: error });
  }
}

function schemaError(kind, file, errors) {
  const details = errors.map(({ instancePath, message }) => `${instancePath || "/"} ${message}`).join("; ");
  return new Error(`${kind} ${file} 未通过 JSON Schema：${details}`);
}

function validateDefinitionSemantics(definition, file) {
  if (definition.documentStatus !== "active") {
    throw new Error(`指标定义 ${definition.id ?? file} 未生效`);
  }
  const expectedOperator = definition.measurement.direction === "max" ? "lte" : "gte";
  if (definition.defaultPolicy.operator !== expectedOperator) {
    throw new Error(`指标 ${definition.id} 的方向与比较运算符不一致`);
  }
}

function normalizeOverride(entry) {
  const metricId = entry.metricId ?? entry.id;
  return { ...entry, metricId };
}

export async function loadDefinitions({
  definitionsDirectory = DEFAULT_DEFINITIONS_DIRECTORY,
  overridesFile,
} = {}) {
  const ajv = new Ajv2020({ allErrors: true, strict: true });
  const metricSchema = await readJson(METRIC_SCHEMA_FILE);
  const overridesSchema = await readJson(OVERRIDES_SCHEMA_FILE);
  const validateMetric = ajv.compile(metricSchema);
  const validateOverrides = ajv.compile(overridesSchema);
  const files = (await readdir(definitionsDirectory))
    .filter((file) => /^QLT-\d{3}-.+\.json$/.test(file))
    .sort();
  const definitions = await Promise.all(files.map(async (file) => {
    const definition = await readJson(path.join(definitionsDirectory, file));
    if (!validateMetric(definition)) throw schemaError("指标定义", file, validateMetric.errors);
    validateDefinitionSemantics(definition, file);
    return definition;
  }));
  const ids = definitions.map(({ id }) => id).sort();
  if (JSON.stringify(ids) !== JSON.stringify(EXPECTED_IDS)) {
    throw new Error(`必须加载 11 项已生效指标，实际为：${ids.join(", ")}`);
  }

  let overrides = [];
  if (overridesFile) {
    const document = await readJson(overridesFile);
    if (!validateOverrides(document)) throw schemaError("覆盖文件", overridesFile, validateOverrides.errors);
    const entries = Array.isArray(document) ? document : document.overrides;
    overrides = entries.map(normalizeOverride);
    const known = new Set(ids);
    for (const override of overrides) {
      if (!known.has(override.metricId)) throw new Error(`覆盖项引用未知指标 ${override.metricId}`);
      const definition = definitions.find(({ id }) => id === override.metricId);
      if (override.threshold !== undefined && definition.overridePolicy.threshold !== true) {
        throw new Error(`指标 ${override.metricId} 不允许覆盖阈值`);
      }
      if (override.applicable !== undefined && definition.overridePolicy.scope !== true) {
        throw new Error(`指标 ${override.metricId} 不允许覆盖适用范围`);
      }
    }
  }

  const overrideById = new Map(overrides.map((override) => [override.metricId, override]));
  return definitions.map((definition) => {
    const override = overrideById.get(definition.id);
    return {
      ...definition,
      effectivePolicy: {
        ...definition.defaultPolicy,
        threshold: override?.threshold ?? definition.defaultPolicy.threshold,
      },
      override: override ?? null,
    };
  });
}

export { DEFAULT_DEFINITIONS_DIRECTORY };
