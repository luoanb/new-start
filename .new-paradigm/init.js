#!/usr/bin/env node
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const toolDirectory = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "governance/metrics/tools/javascript",
);
const child = spawn(
  "pnpm",
  ["install", "--frozen-lockfile", "--ignore-scripts"],
  { cwd: toolDirectory, stdio: "inherit", shell: true, windowsHide: true },
);

child.on("error", (error) => {
  console.error(`无法启动 pnpm：${error.message}`);
  process.exitCode = 2;
});
child.on("close", (code) => {
  if (code !== 0) console.error("指标工具依赖安装失败。请确认已安装 pnpm，并且可以访问依赖源。");
  process.exitCode = code ?? 2;
});
