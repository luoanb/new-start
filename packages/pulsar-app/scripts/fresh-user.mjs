#!/usr/bin/env node
// fresh-user — 把 pulsar-app 切到「新用户」数据状态，测完还原。
//
// 用法（在 packages/pulsar-app 下）：
//   pnpm fresh-user start    # .pulsar/ → .pulsar.bak-<时间戳>，下次启动应用即为全新用户态
//   pnpm fresh-user restore  # 恢复最新备份；模拟期间产生的新 .pulsar/ 先留底为 .pulsar.discarded-<时间戳>
//   pnpm fresh-user status   # 查看当前状态与备份列表
//
// 边界：全程仅 rename，绝不删除任何数据；只处理 packages/pulsar-app/.pulsar/
// （桌面 GUI 编译期固定的数据目录，server:dev 在本包目录下运行时共用）。
// WebView / 浏览器的 localStorage（pulsar:connMode 等）无法由此脚本安全清除，见 hint。

import { existsSync, readdirSync, renameSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const pkgDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const dataDir = path.join(pkgDir, ".pulsar");
const tag = "[fresh-user]";

const timestamp = () => {
  const d = new Date();
  const p = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
};

// 列出 pkgDir 下的 .pulsar.bak-* / .pulsar.discarded-*（名称含时间戳，字典序即时间序）
function listSnapshots(prefix) {
  return readdirSync(pkgDir)
    .filter((name) => name.startsWith(prefix))
    .sort();
}

function warnRunning() {
  console.log(`${tag} 注意: 请先退出应用再执行，运行中的进程会继续写旧句柄或重建目录。`);
}

function hintWebview() {
  console.log(`${tag} 提示: WebView/浏览器的 localStorage（pulsar:connMode / pulsar:remoteUrl 等）不受影响；`);
  console.log(`       如需完整全新体验，可手动清理（桌面 WebKit 存储: ~/.local/share/com.pulsar.app/，浏览器模式用 DevTools）。`);
}

function status() {
  console.log(`${tag} 数据目录: ${dataDir}`);
  if (existsSync(dataDir)) {
    console.log(`${tag} 当前: 已有数据（${readdirSync(dataDir).length} 项）— 非全新状态`);
  } else {
    console.log(`${tag} 当前: 不存在 — 全新用户状态`);
  }
  const baks = listSnapshots(".pulsar.bak-");
  if (baks.length) {
    console.log(`${tag} 备份（旧→新）:`);
    for (const name of baks) console.log(`  ${name}`);
  } else {
    console.log(`${tag} 备份: 无`);
  }
  const discarded = listSnapshots(".pulsar.discarded-");
  if (discarded.length) {
    console.log(`${tag} 留底（restore 时被替换的模拟期数据，旧→新）:`);
    for (const name of discarded) console.log(`  ${name}`);
  }
}

function start() {
  warnRunning();
  if (!existsSync(dataDir)) {
    console.log(`${tag} ${dataDir} 不存在，已是全新状态。`);
    hintWebview();
    return;
  }
  const target = `.pulsar.bak-${timestamp()}`;
  renameSync(dataDir, path.join(pkgDir, target));
  console.log(`${tag} 已改名: .pulsar → ${target}`);
  console.log(`${tag} 下次启动应用即为「新用户」状态（内置 provider 回归、神经元种子重新 bootstrap）。`);
  hintWebview();
}

function restore() {
  warnRunning();
  const baks = listSnapshots(".pulsar.bak-");
  if (!baks.length) {
    console.error(`${tag} 没有可恢复的备份（.pulsar.bak-*）。`);
    process.exitCode = 1;
    return;
  }
  const latest = baks[baks.length - 1];
  if (existsSync(dataDir)) {
    const discarded = `.pulsar.discarded-${timestamp()}`;
    renameSync(dataDir, path.join(pkgDir, discarded));
    console.log(`${tag} 模拟期间的数据已留底: .pulsar → ${discarded}`);
  }
  renameSync(path.join(pkgDir, latest), dataDir);
  console.log(`${tag} 已恢复: ${latest} → .pulsar`);
}

function usage() {
  console.log(`用法: pnpm fresh-user <start|restore|status>  或  --help

将 pulsar-app 切到「新用户」数据状态，测完还原。

指令:
  start    把 .pulsar/ 改名为 .pulsar.bak-<时间戳>，
           下次启动应用即为全新用户态（内置 provider 回归、神经元种子重新 bootstrap）
  restore  恢复最新一份备份回 .pulsar/；
           模拟期间产生的新 .pulsar/ 先留底为 .pulsar.discarded-<时间戳>
  status   查看当前数据状态与全部备份
  --help   显示本帮助

边界:
  全程仅 rename，绝不删除任何数据；只处理 ${pkgDir}/.pulsar/
  执行 start/restore 前请先退出应用
  WebView/浏览器的 localStorage 不受影响，完整全新体验需手动清理`);
}

const cmd = process.argv[2];
switch (cmd) {
  case "start":
    start();
    break;
  case "restore":
    restore();
    break;
  case "status":
    status();
    break;
  case "--help":
  case "-h":
    usage();
    break;
  default:
    usage();
    process.exitCode = cmd ? 1 : 0;
}
