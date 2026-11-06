#!/usr/bin/env node
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(fileURLToPath(import.meta.url));
const directories = [
  ["指标工具", path.join(root, "governance/metrics/tools/javascript")],
  ["契约总览工具", path.join(root, "governance/tools/contract-overview")]
];

function install(name, directory) {
  return new Promise((resolve) => {
    const child = spawn(
      "pnpm",
      ["install", "--frozen-lockfile", "--ignore-scripts"],
      { cwd: directory, stdio: "inherit", shell: true, windowsHide: true }
    );
    child.on("error", (error) => {
      console.error(`无法启动 pnpm：${error.message}`);
      resolve(2);
    });
    child.on("close", (code) => {
      if (code !== 0) console.error(`${name}依赖安装失败。请确认已安装 pnpm，并且可以访问依赖源。`);
      resolve(code ?? 2);
    });
  });
}

for (const [name, directory] of directories) {
  const code = await install(name, directory);
  if (code !== 0) {
    process.exitCode = code;
    break;
  }
}
