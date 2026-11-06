#!/usr/bin/env node
import path from "node:path";
import { fileURLToPath } from "node:url";
import { defaultProject, EMPTY_SOURCE_DIGEST, run } from "./overview.js";

const usage = "用法: node src/cli.js [--check] [--json] [--project <path>]";

export function parseArgs(argv) {
  let check = false;
  let json = false;
  let project;
  let sawProject = false;
  const fail = (message) => ({ error: message, json });
  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === "--check") {
      if (check) return fail("重复参数: --check");
      check = true;
    } else if (value === "--json") {
      if (json) return fail("重复参数: --json");
      json = true;
    } else if (value === "--project") {
      if (sawProject) return fail("重复参数: --project");
      if (!argv[index + 1] || argv[index + 1].startsWith("--")) return fail("--project 缺少路径");
      project = argv[++index];
      sawProject = true;
    } else return fail(`未知参数: ${value}`);
  }
  return { operation: check ? "check" : "generate", request: { project, json }, json };
}

export function human(report) {
  const lines = [
    `${report.operation}: ${report.status} (exit ${report.exitCode})`,
    `project: ${report.project || "-"}`,
    `sourceDigest: ${report.sourceDigest || "-"}`,
    `output: ${report.outputPath ?? "-"}`,
    `inputs: ${report.inputs.map((item) => item.id).join(", ") || "-"}`,
    ...report.issues.map((item) => `ERROR ${item.code}: ${item.message}`)
  ];
  if (report.issues.some((item) => item.code === "INVALID_ARGUMENTS")) lines.push(usage);
  return `${lines.join("\n")}\n`;
}

export async function main(argv = process.argv.slice(2), io = process) {
  const parsed = parseArgs(argv);
  if (parsed.error) {
    const report = {
      schemaVersion: "1.0",
      operation: "generate",
      project: "",
      inputs: [],
      sourceDigest: EMPTY_SOURCE_DIGEST,
      status: "rejected",
      issues: [{ code: "INVALID_ARGUMENTS", path: "", message: parsed.error }],
      outputPath: null,
      exitCode: 2
    };
    io.stdout.write(parsed.json ? `${JSON.stringify(report)}\n` : human(report));
    return 2;
  }
  const request = { ...parsed.request, project: parsed.request.project ?? defaultProject(import.meta.url) };
  const report = await run(parsed.operation, request);
  io.stdout.write(parsed.json ? `${JSON.stringify(report)}\n` : human(report));
  return report.exitCode;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = await main();
}
