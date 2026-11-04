#!/usr/bin/env node
import path from "node:path";
import { pathToFileURL } from "node:url";
import { runAdapter } from "./adapter.js";

const HELP = `JavaScript/TypeScript 指标检测适配器

用法：
  new-paradigm-js-metrics --project <路径> --test-command <命令> [选项]

选项：
  --project <路径>         目标项目路径（默认当前目录）
  --test-command <命令>    项目测试命令
  --tsconfig <路径>        相对目标项目或绝对 tsconfig 路径
  --include <glob>         源码包含范围，可重复或逗号分隔
  --exclude <glob>         源码排除范围，可重复或逗号分隔
  --overrides <路径>       指标覆盖文件（默认读取项目标准位置，存在时）
  --output <路径>          JSON 输出文件；未提供时仅写 stdout
  --help                   显示帮助
`;

function appendList(target, value) {
  target.push(...value.split(",").map((item) => item.trim()).filter(Boolean));
}

export function parseArguments(argv) {
  const options = { project: process.cwd(), include: [], exclude: [] };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--help" || argument === "-h") return { help: true };
    const value = argv[index + 1];
    if (!value || value.startsWith("--")) throw new Error(`参数 ${argument} 缺少值`);
    if (argument === "--project") options.project = path.resolve(value);
    else if (argument === "--test-command") options.testCommand = value;
    else if (argument === "--tsconfig") options.tsconfig = value;
    else if (argument === "--include") appendList(options.include, value);
    else if (argument === "--exclude") appendList(options.exclude, value);
    else if (argument === "--overrides") options.overrides = path.resolve(value);
    else if (argument === "--output") options.output = path.resolve(value);
    else throw new Error(`未知参数：${argument}`);
    index += 1;
  }
  return options;
}

async function main() {
  try {
    const options = parseArguments(process.argv.slice(2));
    if (options.help) {
      process.stdout.write(HELP);
      return;
    }
    const defaultOverrides = path.join(options.project, ".new-paradigm", "governance", "metrics", "overrides.json");
    if (!options.overrides) {
      const { access } = await import("node:fs/promises");
      try {
        await access(defaultOverrides);
        options.overrides = defaultOverrides;
      } catch {
        // 覆盖文件是可选输入。
      }
    }
    const report = await runAdapter(options);
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    process.exitCode = report.exitCode;
  } catch (error) {
    const report = {
      schemaVersion: "1.0",
      run: { finishedAt: new Date().toISOString() },
      adapter: { name: "@new-paradigm/javascript-metrics-adapter", version: "1.0.0" },
      results: [],
      exitCode: 2,
      error: { name: error.name ?? "Error", message: error.message ?? String(error) },
    };
    process.stderr.write(`${JSON.stringify(report, null, 2)}\n`);
    process.exitCode = 2;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  await main();
}

export { main, HELP };
