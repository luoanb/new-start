#!/usr/bin/env node
// 一键发布脚本：累加版本号 → 提交并推送 main → release 分支快进推送（触发 CI）→ 打 tag 推送。
//
// 用法（在 packages/pulsar-app 目录下）：
//   node scripts/release.mjs [patch|minor|major]   （默认 patch；支持中文别名 小/中/大）
//
// 版本号同步四处：package.json / tauri.conf.json / Cargo.toml / Cargo.lock（pulsar-app 包）。
// 前置要求：当前 git 分支为 main，且工作区无未提交改动。

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const HERE = dirname(fileURLToPath(import.meta.url));
const PKG_DIR = join(HERE, ".."); // packages/pulsar-app
const REPO_DIR = join(PKG_DIR, ".."); // workspace 根（.git 所在）
const CARGO_PKG = "pulsar-app"; // Cargo 包名（Cargo.lock 中 [[package]].name）

const FILES = {
  packageJson: join(PKG_DIR, "package.json"),
  tauriConf: join(PKG_DIR, "src-tauri", "tauri.conf.json"),
  cargoToml: join(PKG_DIR, "src-tauri", "Cargo.toml"),
  cargoLock: join(PKG_DIR, "src-tauri", "Cargo.lock"),
};

// ── 参数解析：patch/minor/major（支持 小/中/大 中文别名）──
const BUMP_ALIASES = { 小: "patch", 中: "minor", 大: "major" };
const bump = BUMP_ALIASES[process.argv[2]] ?? process.argv[2] ?? "patch";
if (!["patch", "minor", "major"].includes(bump)) {
  console.error(`用法: node scripts/release.mjs [patch|minor|major]（默认 patch）`);
  process.exit(2);
}

const git = (args) =>
  execFileSync("git", args, { cwd: REPO_DIR, stdio: "inherit" });

// ── 读取当前版本（package.json 为准，校验其余三处一致）──
function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}
function readVersion() {
  const pkg = readJson(FILES.packageJson);
  const tauri = readJson(FILES.tauriConf);
  const toml = readFileSync(FILES.cargoToml, "utf8").match(/^version = "([^"]+)"/m);
  const lock = readFileSync(FILES.cargoLock, "utf8");
  const pkgBlock = lock.split("\n\n").find((b) => b.startsWith(`[[package]]\nname = "${CARGO_PKG}"\n`));
  const lockV = pkgBlock?.match(/^version = "([^"]+)"/m)?.[1];
  if (!toml || !lockV) {
    console.error("无法从 Cargo.toml / Cargo.lock 读取版本号");
    process.exit(1);
  }
  for (const [k, v] of [
    ["tauri.conf.json", tauri.version],
    ["Cargo.toml", toml[1]],
    ["Cargo.lock", lockV],
  ]) {
    if (v !== pkg.version) {
      console.error(`版本号不一致：package.json=${pkg.version} 但 ${k}=${v}，请先手工统一`);
      process.exit(1);
    }
  }
  return pkg.version;
}

// ── 累加 ──
function nextVersion(current, mode) {
  const [maj, min, pat] = current.split(".").map(Number);
  switch (mode) {
    case "major":
      return `${maj + 1}.0.0`;
    case "minor":
      return `${maj}.${min + 1}.0`;
    default:
      return `${maj}.${min}.${pat + 1}`;
  }
}

// ── 写回四处 ──
function writeVersions(version) {
  const pkg = readJson(FILES.packageJson);
  pkg.version = version;
  writeFileSync(FILES.packageJson, JSON.stringify(pkg, null, 2) + "\n");

  const tauri = readJson(FILES.tauriConf);
  tauri.version = version;
  writeFileSync(FILES.tauriConf, JSON.stringify(tauri, null, 2) + "\n");

  const toml = readFileSync(FILES.cargoToml, "utf8");
  writeFileSync(
    FILES.cargoToml,
    toml.replace(/^version = "[^"]+"/m, `version = "${version}"`)
  );

  const lock = readFileSync(FILES.cargoLock, "utf8");
  writeFileSync(
    FILES.cargoLock,
    lock.replace(
      /^(\[\[package\]\]\nname = "pulsar-app"\n)version = "[^"]+"/m,
      `$1version = "${version}"`
    )
  );
  console.log(`已同步版本号 ${version} 到 package.json / tauri.conf.json / Cargo.toml / Cargo.lock`);
}

// ── 主流程 ──
try {
  const currentBranch = execFileSync("git", ["branch", "--show-current"], { cwd: REPO_DIR, encoding: "utf8" }).trim();
  if (currentBranch !== "main") {
    console.error(`当前分支为 ${currentBranch}，发布要求从 main 发起`);
    process.exit(1);
  }
  const dirty = execFileSync("git", ["status", "--porcelain"], { cwd: REPO_DIR, encoding: "utf8" });
  if (dirty.trim()) {
    console.error("工作区有未提交改动，请先提交或暂存");
    process.exit(1);
  }

  const current = readVersion();
  const version = nextVersion(current, bump);
  console.log(`发布版本：${current} → ${version}（${bump}）`);

  writeVersions(version);

  git(["add", "packages/pulsar-app/package.json", "packages/pulsar-app/src-tauri/tauri.conf.json",
       "packages/pulsar-app/src-tauri/Cargo.toml", "packages/pulsar-app/src-tauri/Cargo.lock"]);
  git(["commit", "-m", `chore: bump version to ${version}`]);
  git(["push", "origin", "main"]);
  console.log("✓ main 已推送");

  git(["checkout", "release"]);
  git(["merge", "--ff-only", "main"]);
  git(["push", "origin", "release"]);
  console.log("✓ release 已快进并推送（CI 已触发）");

  const tag = `pulsar-v${version}`;
  git(["tag", "-a", tag, "-m", `Release ${version}`]);
  git(["push", "origin", tag]);
  console.log(`✓ tag ${tag} 已推送`);

  git(["checkout", "main"]);
  console.log(`\n发布完成：${version}（${tag}）`);
} catch (err) {
  git(["checkout", "main"]);
  console.error("\n发布中断，请检查上述错误（版本号改动可能已产生，需手工回滚）");
  process.exit(1);
}
