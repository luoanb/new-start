#!/usr/bin/env node
// 带自定义前后端端口的 tauri 启动器（pnpm tauri:dev 使用本脚本）。
//
// 端口单一来源：
//  - 前端：DEV_FRONT_PORT 环境变量；未设置回退 tauri.conf.json 的 build.devUrl。
//  - 后端：PULSAR_PORT 环境变量；未设置默认 8899。
// vite 侧读同一批环境变量（vite.config.js 的 DEV_FRONT_PORT / PULSAR_PORT），
// tauri 侧用 `tauri dev --config` 把前端端口注入 devUrl、后端由 Rust core::config 读 PULSAR_PORT，
// 两端读同一个值，任何情况下都不会端口错位。
//
// 用法（在 packages/pulsar-app 下）：
//   pnpm tauri:dev                          # 默认端口（1432 / 8899）
//   pnpm tauri:dev --frontend-port 1450 --backend-port 9000
//   DEV_FRONT_PORT=1450 PULSAR_PORT=9000 pnpm tauri:dev
import { spawn, spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const confPath = new URL("../src-tauri/tauri.conf.json", import.meta.url);
const conf = JSON.parse(readFileSync(fileURLToPath(confPath), "utf8"));
const pkgRoot = fileURLToPath(new URL("..", import.meta.url));
const baseFrontPort = Number(new URL(conf.build.devUrl).port);
const defaultBackPort = 8899;

// Rust 侧 default feature `embed-static` 使 net/static_assets.rs 的
// `#[folder = "../build/"]` 在**编译期**就要求 build/ 存在；而 dev 路径只跑
// beforeDevCommand（vite dev server），不产出该目录，新克隆/清理产物后编译必挂。
// 故启动前按需补一次生产构建（dev 下内嵌副本不被使用，仅需存在）。
const buildIndex = fileURLToPath(new URL("../build/index.html", import.meta.url));

function ensureFrontendBuild() {
  if (existsSync(buildIndex)) return;
  console.log("[dev-tauri] 缺少前端产物 build/，先执行 pnpm build ...");
  const built = spawnSync("pnpm", ["build"], { cwd: pkgRoot, stdio: "inherit" });
  if (built.status !== 0 || !existsSync(buildIndex)) {
    console.error("[dev-tauri] 前端构建未产出 build/index.html，终止启动");
    process.exit(1);
  }
}

// 解析 --<name>=<n> 或 --<name> <n> 形式的端口参数
function parsePort(argv, name) {
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === `--${name}`) {
      const v = Number(argv[i + 1]);
      if (Number.isFinite(v)) return v;
    }
    if (a.startsWith(`--${name}=`)) {
      const v = Number(a.split("=")[1]);
      if (Number.isFinite(v)) return v;
    }
  }
  return null;
}

// 将任意输入解析为合法端口；undefined/空串/NaN 等一律视为未设置（返回 null）
function toPort(value) {
  const n = Number(value);
  return Number.isInteger(n) && n > 0 ? n : null;
}

const argv = process.argv.slice(2);
// 优先级：命令行参数 > 环境变量 > 默认值
// 注意：Number(undefined) === NaN，而 ?? 只回退 null/undefined，必须用 toPort 归一化
const frontPort = parsePort(argv, "frontend-port") ?? toPort(process.env.DEV_FRONT_PORT) ?? baseFrontPort;
const backPort = parsePort(argv, "backend-port") ?? toPort(process.env.PULSAR_PORT) ?? defaultBackPort;

const args = ["exec", "tauri", "dev"];
// 仅当覆盖默认前端端口时才注入配置合并，默认路径保持最小
if (frontPort !== baseFrontPort) {
  args.push("--config", JSON.stringify({ build: { devUrl: `http://localhost:${frontPort}` } }));
}
// 透传其它 CLI 参数（如 --release、--features 等），剔除自定义端口参数
const passthrough = argv.filter((a) => a.startsWith("-") && !a.startsWith("--frontend-port") && !a.startsWith("--backend-port"));
args.push(...passthrough);

ensureFrontendBuild();

const child = spawn("pnpm", args, {
  stdio: "inherit",
  env: {
    ...process.env,
    // 供 beforeDevCommand 内启动的 vite 读取，保证与 tauri 注入的端口一致
    DEV_FRONT_PORT: String(frontPort),
    // 后端：Rust core::config 读 PULSAR_PORT；vite 代理默认也指向同一端口。
    // 注意：不要注入 PULSAR_HOST——env 优先级高于 config.json，注入会覆盖「局域网访问」
    // 开关导致其失效；绑定地址交由 config `server.lan` 决定（缺省仍为 127.0.0.1）。
    PULSAR_PORT: String(backPort),
  },
});
child.on("error", (err) => {
  console.error("[dev-tauri] 启动 tauri 失败:", err);
  process.exit(1);
});
child.on("exit", (code) => process.exit(code ?? 0));