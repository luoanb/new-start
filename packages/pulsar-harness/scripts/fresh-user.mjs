#!/usr/bin/env node
// fresh-user — 把 pulsar-harness 切到「新用户」数据状态，测完还原。
//
// 用法（在 packages/pulsar-harness 下）：
//   pnpm fresh-user start    # .pulsar-harness/ → .pulsar-harness.bak-<时间戳>，下次启动应用即为全新用户态
//   pnpm fresh-user restore  # 恢复最新备份；模拟期间产生的新 .pulsar-harness/ 先留底为 .pulsar-harness.discarded-<时间戳>
//   pnpm fresh-user status   # 查看当前状态与备份列表
//
// 边界：全程仅 rename，绝不删除任何数据；只处理 packages/pulsar-harness/.pulsar-harness/
// （桌面 GUI 编译期固定的数据目录）。

import { existsSync, readdirSync, renameSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const pkgDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const DIR_NAME = ".pulsar-harness";
const dataDir = path.join(pkgDir, DIR_NAME);
const tag = "[fresh-user]";

const timestamp = () => {
  const d = new Date();
  const p = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
};

// 列出 pkgDir 下的 .pulsar-harness.bak-* / .pulsar-harness.discarded-*（名称含时间戳，字典序即时间序）
function listSnapshots(prefix) {
  return readdirSync(pkgDir)
    .filter((name) => name.startsWith(prefix))
    .sort();
}

function warnRunning() {
  console.log(`${tag} 注意: 请先退出应用再执行，运行中的进程会继续写旧句柄或重建目录。`);
}

function hintWebview() {
  console.log(`${tag} 提示: WebView/浏览器的 localStorage 不受影响；`);
  console.log(`       如需完整全新体验，可手动清理（桌面 WebKit 存储: ~/.local/share/com.pulsar.harness/）。`);
}

function status() {
  console.log(`${tag} 数据目录: ${dataDir}`);
  if (existsSync(dataDir)) {
    console.log(`${tag} 当前: 已有数据（${readdirSync(dataDir).length} 项）— 非全新状态`);
  } else {
    console.log(`${tag} 当前: 不存在 — 全新用户状态`);
  }
  const baks = listSnapshots(`${DIR_NAME}.bak-`);
  if (baks.length) {
    console.log(`${tag} 备份（旧→新）:`);
    for (const name of baks) console.log(`  ${name}`);
  } else {
    console.log(`${tag} 备份: 无`);
  }
  const discarded = listSnapshots(`${DIR_NAME}.discarded-`);
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
  const target = `${DIR_NAME}.bak-${timestamp()}`;
  renameSync(dataDir, path.join(pkgDir, target));
  console.log(`${tag} 已改名: ${DIR_NAME} → ${target}`);
  console.log(`${tag} 下次启动应用即为「新用户」状态。`);
  hintWebview();
}

function restore() {
  warnRunning();
  const baks = listSnapshots(`${DIR_NAME}.bak-`);
  if (!baks.length) {
    console.error(`${tag} 没有可恢复的备份（${DIR_NAME}.bak-*）。`);
    process.exitCode = 1;
    return;
  }
  const latest = baks[baks.length - 1];
  if (existsSync(dataDir)) {
    const discarded = `${DIR_NAME}.discarded-${timestamp()}`;
    renameSync(dataDir, path.join(pkgDir, discarded));
    console.log(`${tag} 模拟期间的数据已留底: ${DIR_NAME} → ${discarded}`);
  }
  renameSync(path.join(pkgDir, latest), dataDir);
  console.log(`${tag} 已恢复: ${latest} → ${DIR_NAME}`);
}

function usage() {
  console.log(`用法: pnpm fresh-user <start|restore|status>  或  --help

将 pulsar-harness 切到「新用户」数据状态，测完还原。

指令:
  start    把 ${DIR_NAME}/ 改名为 ${DIR_NAME}.bak-<时间戳>，
           下次启动应用即为全新用户态
  restore  恢复最新一份备份回 ${DIR_NAME}/；
           模拟期间产生的新 ${DIR_NAME}/ 先留底为 ${DIR_NAME}.discarded-<时间戳>
  status   查看当前数据状态与全部备份
  --help   显示本帮助

边界:
  全程仅 rename，绝不删除任何数据；只处理 ${pkgDir}/${DIR_NAME}/
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
