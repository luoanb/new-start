import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import {
  lstat,
  readFile,
  readdir,
  realpath,
  rename,
  rm,
  stat,
  writeFile
} from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";
import { parseDocument } from "yaml";

const OVERVIEW_PATH = "functional/contracts/overview.md";
const ID_PATTERN = /^CTR-[A-Z][A-Z0-9]*-[0-9]{3}$/;
const EXIT = {
  INVALID_ARGUMENTS: 2,
  CONTRACT_PARSE_ERROR: 2,
  PATH_ESCAPE_BLOCKED: 2,
  IO_FAILURE: 2,
  RELATION_INVALID: 1,
  OVERVIEW_STALE: 1
};
const schema = JSON.parse(readFileSync(new URL("./relation.schema.json", import.meta.url), "utf8"));
export const defaultIo = { lstat, readFile, readdir, realpath, rename, rm, stat, writeFile };

const compare = (left, right) => (left < right ? -1 : left > right ? 1 : 0);
const slash = (value) => value.split(path.sep).join("/");
export const EMPTY_SOURCE_DIGEST = createHash("sha256").update("").digest("hex");
const ajv = new Ajv2020({ allErrors: true, strict: true });
const validateRelations = ajv.compile(schema);

export class OverviewError extends Error {
  constructor(code, message, pathValue = "") {
    super(message);
    this.name = "OverviewError";
    this.code = code;
    this.path = pathValue;
  }
}

function issue(code, pathValue, message) {
  return { code, path: pathValue, message };
}

function inside(root, candidate) {
  const relative = path.relative(root, candidate);
  return relative === "" || (!relative.startsWith("..") && !path.isAbsolute(relative));
}

function mechanismRelative(project, file) {
  return slash(path.relative(path.join(project, ".new-paradigm"), file));
}

function sourceDigest(contracts) {
  const hash = createHash("sha256");
  for (const contract of contracts) {
    hash.update(`${contract.path}\n`, "utf8");
    hash.update(contract.bytes);
    hash.update("\n");
  }
  return contracts.length ? hash.digest("hex") : EMPTY_SOURCE_DIGEST;
}

function buildReport({ operation, project, contracts, status, issues, outputPath }) {
  const sortedIssues = [...issues].sort((left, right) =>
    compare(left.code, right.code) || compare(left.path, right.path) || compare(left.message, right.message));
  const exitCode = sortedIssues.reduce((code, item) => Math.max(code, EXIT[item.code] ?? 2), 0);
  return {
    schemaVersion: "1.0",
    operation,
    project,
    inputs: contracts.map((contract) => ({
      id: contract.id,
      type: contract.type,
      domain: contract.domain,
      path: contract.path
    })),
    sourceDigest: sourceDigest(contracts),
    status,
    issues: sortedIssues.map(({ code, path: issuePath, message }) => ({ code, path: issuePath, message })),
    outputPath,
    exitCode
  };
}

function argumentReport(operation, project, message) {
  return buildReport({
    operation: operation === "check" ? "check" : "generate",
    project,
    contracts: [],
    status: "rejected",
    issues: [issue("INVALID_ARGUMENTS", "", message)],
    outputPath: null
  });
}

export async function resolveProject(io, project) {
  if (!project) throw new OverviewError("INVALID_ARGUMENTS", "项目路径缺失");
  let resolved;
  try {
    resolved = await io.realpath(path.resolve(project));
    if (!(await io.stat(resolved)).isDirectory()) throw new Error("不是目录");
  } catch (error) {
    if (error instanceof OverviewError) throw error;
    throw new OverviewError("INVALID_ARGUMENTS", `项目路径无法访问: ${project}`);
  }
  return resolved;
}

function parseHeader(text) {
  let title = "";
  const fields = {};
  for (const line of text.split(/\r?\n/)) {
    if (line.startsWith("## ")) break;
    if (!title) {
      const heading = line.match(/^#\s+(.+?)\s*$/);
      if (heading) {
        title = heading[1];
        continue;
      }
    }
    const field = line.match(/^>\s*([^：:]+)\s*[：:]\s*(.*)$/);
    if (field) fields[field[1].trim()] = field[2].trim();
  }
  return { title, fields };
}

function relationBlocks(text) {
  return [...text.matchAll(/^```yaml contract-relations[ \t]*\r?\n([\s\S]*?)^```[ \t]*$/gm)].map((match) => match[1]);
}

function parseContract(file, project, contractsRoot) {
  const relativeToContracts = slash(path.relative(contractsRoot, file));
  const contractPath = mechanismRelative(project, file);
  const domain = path.posix.dirname(relativeToContracts);
  if (domain === ".") {
    return { error: issue("CONTRACT_PARSE_ERROR", contractPath, "生效契约必须位于领域目录") };
  }
  return { relativeToContracts, contractPath, domain, stem: path.posix.basename(relativeToContracts, ".md") };
}

async function collectFiles(io, directory, project, contractsRoot, issues) {
  const files = [];
  let entries;
  try {
    entries = await io.readdir(directory, { withFileTypes: true });
  } catch (error) {
    throw new OverviewError("IO_FAILURE", `无法读取契约目录: ${error.message}`, "functional/contracts");
  }
  for (const entry of [...entries].sort((left, right) => compare(left.name, right.name))) {
    if (entry.name === "overview.md") continue;
    const full = path.join(directory, entry.name);
    if (entry.isSymbolicLink()) {
      let resolved;
      try {
        resolved = await io.realpath(full);
      } catch (error) {
        if (error.code === "ENOENT") continue;
        throw new OverviewError("IO_FAILURE", `无法解析符号链接: ${error.message}`, mechanismRelative(project, full));
      }
      if (!inside(contractsRoot, resolved)) {
        issues.push(issue("PATH_ESCAPE_BLOCKED", mechanismRelative(project, full), `符号链接越界: ${entry.name}`));
        continue;
      }
      const info = await io.stat(resolved);
      if (info.isDirectory()) files.push(...await collectFiles(io, resolved, project, contractsRoot, issues));
      else if (info.isFile() && entry.name.endsWith(".md")) files.push(resolved);
      continue;
    }
    if (entry.isDirectory()) files.push(...await collectFiles(io, full, project, contractsRoot, issues));
    else if (entry.isFile() && entry.name.endsWith(".md")) files.push(full);
  }
  return files;
}

function unsafeReference(value) {
  return typeof value !== "string" ||
    !value ||
    value.includes("\0") ||
    value.includes("\\") ||
    path.posix.isAbsolute(value) ||
    path.win32.isAbsolute(value);
}

async function resolveParticipant(io, contractsRoot, sourceFile, reference) {
  if (unsafeReference(reference)) return { error: "unsafe" };
  const candidate = path.resolve(path.dirname(sourceFile), ...reference.split("/"));
  if (!inside(contractsRoot, candidate)) return { error: "unsafe" };
  const relative = path.relative(contractsRoot, candidate);
  const parts = relative.split(path.sep).filter(Boolean);
  let current = contractsRoot;
  for (const part of parts) {
    current = path.join(current, part);
    let info;
    try {
      info = await io.lstat(current);
    } catch (error) {
      if (error.code === "ENOENT") return { missing: true };
      throw new OverviewError("IO_FAILURE", `无法检查参与契约: ${error.message}`);
    }
    if (info.isSymbolicLink()) {
      const resolved = await io.realpath(current);
      if (!inside(contractsRoot, resolved)) return { error: "unsafe" };
      current = resolved;
    }
  }
  try {
    return { file: await io.realpath(current) };
  } catch (error) {
    if (error.code === "ENOENT") return { missing: true };
    throw new OverviewError("IO_FAILURE", `无法定位参与契约: ${error.message}`);
  }
}

function canonicalCycle(cycle) {
  const body = cycle.slice(0, -1);
  let start = 0;
  for (let index = 1; index < body.length; index += 1) {
    if (compare(body[index], body[start]) < 0) start = index;
  }
  const rotated = body.slice(start).concat(body.slice(0, start));
  return [...rotated, rotated[0]].join(" -> ");
}

function findCycles(relations) {
  const adjacency = new Map();
  for (const relation of relations) {
    const next = adjacency.get(relation.from) ?? [];
    next.push(relation.to);
    adjacency.set(relation.from, next);
  }
  for (const [key, values] of adjacency) adjacency.set(key, [...new Set(values)].sort(compare));
  const cycles = [];
  const visiting = new Set();
  const visited = new Set();
  const stack = [];
  const visit = (node) => {
    visiting.add(node);
    stack.push(node);
    for (const next of adjacency.get(node) ?? []) {
      if (visiting.has(next)) {
        const start = stack.indexOf(next);
        if (start >= 0) cycles.push(canonicalCycle([...stack.slice(start), next]));
      } else if (!visited.has(next)) visit(next);
    }
    stack.pop();
    visiting.delete(node);
    visited.add(node);
  };
  for (const node of [...adjacency.keys()].sort(compare)) if (!visited.has(node)) visit(node);
  return [...new Set(cycles)].sort(compare);
}

function render(model) {
  const lines = [
    "# 代码契约总览",
    "",
    "> 生成文件：禁止手工修改",
    "> 裁决说明：本文件不参与裁决；发生冲突时，以对应的生效代码契约为准",
    `> 来源摘要：${model.digest}`,
    "",
    "## 能力",
    ""
  ];
  if (!model.capabilities.length) lines.push("- 无", "");
  let domain;
  for (const capability of model.capabilities) {
    if (capability.domain !== domain) {
      if (domain !== undefined) lines.push("");
      domain = capability.domain;
      lines.push(`### ${capability.domain}`, "");
    }
    lines.push(`- [\`${capability.id} ${capability.stem}\`](${capability.link})：${capability.scope}`);
  }
  if (model.capabilities.length) lines.push("");
  lines.push("## 协作", "");
  if (!model.collaborations.length) lines.push("- 无", "");
  for (const collaboration of model.collaborations) {
    lines.push(`- [\`${collaboration.id} ${collaboration.stem}\`](${collaboration.link})`);
    lines.push(`  - 参与能力：${collaboration.participants.map((id) => `\`${id}\``).join("、")}`);
    for (const relation of collaboration.relations) {
      const detail = relation.interaction ? `${relation.type}: ${relation.interaction}` : relation.type;
      lines.push(`  - 关系：\`${relation.from} -> ${relation.to}\`（${detail}）`);
    }
  }
  if (model.collaborations.length) lines.push("");
  lines.push("## 关系图", "", "```mermaid", "flowchart LR");
  for (const node of model.nodes) lines.push(`  ${node.mermaidId}["${node.label}"]`);
  for (const edge of model.edges) lines.push(`  ${edge.from} -->|"${edge.label}"| ${edge.to}`);
  lines.push("```", "", "## 检查结果", "");
  lines.push("- 无法定位的参与契约：无");
  lines.push("- 重复关系声明：无");
  lines.push(`- 孤立能力：${model.isolated.length ? model.isolated.map((id) => `\`${id}\``).join("、") : "无"}`);
  lines.push(`- 循环依赖：${model.cycles.length ? `${model.cycles.join("；")}；仅提示，不自动判错` : "无"}`);
  lines.push("");
  return lines.join("\n");
}

function overviewModel(contracts, relationsByContract) {
  const capabilities = contracts
    .filter((contract) => contract.type === "能力契约")
    .sort((left, right) => compare(left.domain, right.domain) || compare(left.id, right.id) || compare(left.path, right.path));
  const collaborations = contracts
    .filter((contract) => contract.type === "协作契约")
    .sort((left, right) => compare(left.id, right.id) || compare(left.path, right.path))
    .map((contract) => ({
      ...contract,
      participants: [...relationsByContract.get(contract.path).participants].sort(compare),
      relations: [...relationsByContract.get(contract.path).relations].sort((left, right) =>
        compare(left.from, right.from) || compare(left.to, right.to) || compare(left.type, right.type) || compare(left.interaction, right.interaction))
    }));
  const participantIds = new Set(collaborations.flatMap((contract) => contract.participants));
  const relations = collaborations.flatMap((contract) => contract.relations);
  const nodes = [...capabilities];
  const seen = new Set(nodes.map((contract) => contract.id));
  for (const id of [...new Set(relations.flatMap((relation) => [relation.from, relation.to]))].sort(compare)) {
    if (!seen.has(id)) nodes.push(contracts.find((contract) => contract.id === id));
  }
  return {
    digest: sourceDigest(contracts),
    capabilities: capabilities.map((contract) => ({
      id: contract.id,
      stem: contract.stem,
      domain: contract.domain,
      link: contract.link,
      scope: contract.scope
    })),
    collaborations: collaborations.map((contract) => ({
      id: contract.id,
      stem: contract.stem,
      link: contract.link,
      participants: contract.participants,
      relations: contract.relations
    })),
    nodes: nodes.filter(Boolean).map((contract) => ({
      mermaidId: contract.id.replaceAll("-", "_"),
      label: `${contract.id} ${contract.stem}`.replaceAll('"', "'")
    })),
    edges: relations
      .map((relation) => ({
        from: relation.from.replaceAll("-", "_"),
        to: relation.to.replaceAll("-", "_"),
        label: (relation.interaction ? `${relation.type}: ${relation.interaction}` : relation.type).replaceAll('"', "'"),
        sort: `${relation.from}\0${relation.to}\0${relation.type}\0${relation.interaction}`
      }))
      .sort((left, right) => compare(left.sort, right.sort)),
    isolated: capabilities.filter((contract) => !participantIds.has(contract.id)).map((contract) => contract.id),
    cycles: findCycles(relations)
  };
}

async function analyze(io, project) {
  const contractsRoot = path.join(project, ".new-paradigm", "functional", "contracts");
  let contractsReal;
  try {
    contractsReal = await io.realpath(contractsRoot);
  } catch (error) {
    if (error.code === "ENOENT") return { contracts: [], issues: [] };
    throw new OverviewError("IO_FAILURE", `无法读取契约目录: ${error.message}`, "functional/contracts");
  }
  const issues = [];
  if (!inside(project, contractsReal)) {
    issues.push(issue("PATH_ESCAPE_BLOCKED", "functional/contracts", "契约目录符号链接越界"));
    return { contracts: [], issues };
  }
  const files = await collectFiles(io, contractsReal, project, contractsReal, issues);
  const contracts = [];
  for (const file of files) {
    let bytes;
    try {
      bytes = await io.readFile(file);
    } catch (error) {
      throw new OverviewError("IO_FAILURE", `无法读取契约: ${error.message}`, mechanismRelative(project, file));
    }
    const text = bytes.toString("utf8").replace(/^\uFEFF/, "");
    const header = parseHeader(text);
    if (!header.fields["契约类型"]) continue;
    if (header.fields["文档状态"] !== "生效") continue;
    const located = parseContract(file, project, contractsReal);
    if (located.error) {
      issues.push(located.error);
      continue;
    }
    const type = header.fields["契约类型"];
    const id = header.fields["契约标识"] ?? "";
    const scope = (header.fields["适用范围"] ?? "").replace(/\s+/g, " ").trim();
    const errors = [];
    if (type !== "能力契约" && type !== "协作契约") errors.push("契约类型不受支持");
    if (!ID_PATTERN.test(id)) errors.push("契约标识不符合 CTR-<领域代号>-NNN");
    if (!header.fields["所属领域"]) errors.push("缺少所属领域");
    if (!scope) errors.push("缺少适用范围");
    if (!header.title) errors.push("缺少标题");
    if (errors.length) {
      issues.push(issue("CONTRACT_PARSE_ERROR", located.contractPath, errors.join("；")));
      continue;
    }
    let realPath;
    try {
      realPath = await io.realpath(file);
    } catch (error) {
      throw new OverviewError("IO_FAILURE", `无法定位契约: ${error.message}`, located.contractPath);
    }
    contracts.push({
      id,
      type,
      domain: located.domain,
      path: located.contractPath,
      link: located.relativeToContracts,
      stem: located.stem,
      scope,
      title: header.title,
      bytes,
      text,
      file,
      realPath
    });
  }
  contracts.sort((left, right) => compare(left.path, right.path));
  const seenIds = new Set();
  for (const contract of contracts) {
    if (seenIds.has(contract.id)) issues.push(issue("CONTRACT_PARSE_ERROR", contract.path, `契约标识重复: ${contract.id}`));
    seenIds.add(contract.id);
  }
  const byRealPath = new Map(contracts.map((contract) => [contract.realPath, contract]));
  const relationsByContract = new Map();
  const relationOwners = new Map();
  for (const contract of contracts.filter((item) => item.type === "协作契约")) {
    const blocks = relationBlocks(contract.text);
    if (blocks.length !== 1) {
      issues.push(issue(
        "CONTRACT_PARSE_ERROR",
        contract.path,
        blocks.length ? "结构化关系声明必须唯一" : "缺少结构化关系声明"
      ));
      continue;
    }
    const document = parseDocument(blocks[0], { uniqueKeys: true, strict: true });
    if (document.errors.length) {
      issues.push(issue("CONTRACT_PARSE_ERROR", contract.path, `结构化关系声明无法解析: ${document.errors[0].message}`));
      continue;
    }
    const data = document.toJS({ maxAliasCount: 0 });
    if (!validateRelations(data)) {
      issues.push(issue("CONTRACT_PARSE_ERROR", contract.path, `结构化关系声明不符合 Schema: ${ajv.errorsText(validateRelations.errors, { separator: "；" })}`));
      continue;
    }
    const participants = [];
    const participantIds = new Set();
    const participantPaths = new Set();
    let participantFailed = false;
    for (const participant of data.participants) {
      if (participantIds.has(participant.id) || participantPaths.has(participant.contract)) {
        issues.push(issue("CONTRACT_PARSE_ERROR", contract.path, `参与方重复: ${participant.id}`));
        participantFailed = true;
        continue;
      }
      participantIds.add(participant.id);
      participantPaths.add(participant.contract);
      const resolved = await resolveParticipant(io, contractsReal, contract.file, participant.contract);
      if (resolved.error) {
        issues.push(issue("PATH_ESCAPE_BLOCKED", contract.path, `参与契约路径越界: ${participant.contract}`));
        participantFailed = true;
        continue;
      }
      const target = resolved.file ? byRealPath.get(resolved.file) : undefined;
      if (resolved.missing || !target) {
        issues.push(issue("RELATION_INVALID", contract.path, `参与契约不存在: ${participant.contract}`));
        participantFailed = true;
        continue;
      }
      if (target.id !== participant.id) {
        issues.push(issue("RELATION_INVALID", contract.path, `参与方标识与目标契约不一致: ${participant.id} -> ${target.id}`));
        participantFailed = true;
        continue;
      }
      participants.push(participant.id);
    }
    const relations = [];
    for (const relation of data.relations) {
      if (!participantIds.has(relation.from) || !participantIds.has(relation.to)) {
        issues.push(issue("RELATION_INVALID", contract.path, `关系端点未声明: ${relation.from} -> ${relation.to}`));
        participantFailed = true;
        continue;
      }
      const normalized = { ...relation, interaction: relation.interaction ?? "" };
      const key = `${normalized.from}\0${normalized.to}\0${normalized.type}\0${normalized.interaction}`;
      if (relationOwners.has(key)) {
        issues.push(issue("RELATION_INVALID", contract.path, `重复关系: ${normalized.from} -> ${normalized.to} ${normalized.type}`));
        participantFailed = true;
        continue;
      }
      relationOwners.set(key, contract.path);
      relations.push(normalized);
    }
    if (!participantFailed) relationsByContract.set(contract.path, { participants, relations });
  }
  return { contracts, issues, relationsByContract };
}

async function overviewTarget(io, project) {
  const file = path.join(project, ".new-paradigm", OVERVIEW_PATH);
  try {
    const info = await io.lstat(file);
    if (info.isSymbolicLink()) {
      const resolved = await io.realpath(file);
      if (!inside(project, resolved)) throw new OverviewError("PATH_ESCAPE_BLOCKED", "契约总览符号链接越界", OVERVIEW_PATH);
    }
  } catch (error) {
    if (error.code === "ENOENT") return file;
    throw error instanceof OverviewError ? error : new OverviewError("IO_FAILURE", `无法检查契约总览: ${error.message}`, OVERVIEW_PATH);
  }
  return file;
}

async function readOverview(io, project) {
  const file = await overviewTarget(io, project);
  try {
    return { exists: true, file, text: (await io.readFile(file)).toString("utf8") };
  } catch (error) {
    if (error.code === "ENOENT") return { exists: false, file, text: "" };
    throw error instanceof OverviewError ? error : new OverviewError("IO_FAILURE", `无法读取契约总览: ${error.message}`, OVERVIEW_PATH);
  }
}

async function atomicWrite(io, file, content) {
  const temporary = `${file}.${process.pid}.tmp`;
  const backup = `${file}.${process.pid}.bak`;
  await io.writeFile(temporary, content);
  try {
    try {
      await io.rename(temporary, file);
      return;
    } catch (error) {
      if (error.code !== "EEXIST" && error.code !== "EPERM") throw error;
    }
    await io.rename(file, backup);
    try {
      await io.rename(temporary, file);
    } catch (error) {
      await io.rename(backup, file).catch(() => {});
      throw error;
    }
    await io.rm(backup, { force: true });
  } catch (error) {
    await io.rm(temporary, { force: true }).catch(() => {});
    throw new OverviewError("IO_FAILURE", `无法写入契约总览: ${error.message}`, OVERVIEW_PATH);
  }
}

async function atomicRemove(io, file) {
  const backup = `${file}.${process.pid}.removing`;
  try {
    await io.rename(file, backup);
    try {
      await io.rm(backup, { force: true });
    } catch (error) {
      await io.rename(backup, file).catch(() => {});
      throw error;
    }
  } catch (error) {
    if (error instanceof OverviewError) throw error;
    throw new OverviewError("IO_FAILURE", `无法删除契约总览: ${error.message}`, OVERVIEW_PATH);
  }
}

export async function run(operation, request = {}, options = {}) {
  if (operation !== "generate" && operation !== "check") return argumentReport(operation, "", "未知命令");
  const io = options.io ?? defaultIo;
  let project;
  try {
    project = await resolveProject(io, request.project);
  } catch (error) {
    return argumentReport(operation, request.project ? path.resolve(request.project) : "", error.message);
  }
  try {
    const analysis = await analyze(io, project);
    const base = { operation, project, contracts: analysis.contracts };
    if (analysis.issues.length) {
      return buildReport({ ...base, status: "rejected", issues: analysis.issues, outputPath: null });
    }
    const expected = analysis.contracts.length ? render(overviewModel(analysis.contracts, analysis.relationsByContract)) : null;
    const current = await readOverview(io, project);
    const outputPath = expected == null ? null : OVERVIEW_PATH;
    const matches = expected == null ? !current.exists : current.exists && current.text === expected;
    if (operation === "check") {
      return buildReport({
        ...base,
        status: matches ? "consistent" : "stale",
        issues: matches ? [] : [issue("OVERVIEW_STALE", OVERVIEW_PATH, expected == null ? "没有生效契约时不应存在总览" : "契约总览缺失或已过期")],
        outputPath
      });
    }
    if (matches) return buildReport({ ...base, status: "consistent", issues: [], outputPath });
    if (expected == null) {
      await atomicRemove(io, current.file);
      return buildReport({ ...base, status: "removed", issues: [], outputPath: null });
    }
    await atomicWrite(io, current.file, expected);
    return buildReport({ ...base, status: "written", issues: [], outputPath });
  } catch (error) {
    const code = error instanceof OverviewError ? error.code : "IO_FAILURE";
    return buildReport({
      operation,
      project,
      contracts: [],
      status: "rejected",
      issues: [issue(code, error.path ?? "", error.message)],
      outputPath: null
    });
  }
}

export const generate = (request, options) => run("generate", request, options);
export const check = (request, options) => run("check", request, options);

export function defaultProject(moduleUrl = import.meta.url) {
  return path.resolve(path.dirname(fileURLToPath(moduleUrl)), "../../../../../");
}
