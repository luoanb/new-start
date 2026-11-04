import { mkdtemp, readFile, rm } from "node:fs/promises";
import { readFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";
import { ESLint } from "eslint";
import js from "@eslint/js";
import tsParser from "@typescript-eslint/parser";
import sonarjs from "eslint-plugin-sonarjs";
import react from "eslint-plugin-react";
import reactHooks from "eslint-plugin-react-hooks";
import jsxA11y from "eslint-plugin-jsx-a11y";
import next from "@next/eslint-plugin-next";
import { createConfigForNuxt } from "@nuxt/eslint-config";
import vue from "eslint-plugin-vue";
import vueParser from "vue-eslint-parser";
import vueA11y from "eslint-plugin-vuejs-accessibility";
import angular from "@angular-eslint/eslint-plugin";
import angularTemplate from "@angular-eslint/eslint-plugin-template";
import angularTemplateParser from "@angular-eslint/template-parser";
import svelte from "eslint-plugin-svelte";
import svelteParser from "svelte-eslint-parser";
import { compile as compileSvelte } from "svelte/compiler";
import ts from "typescript";

const moduleDirectory = path.dirname(fileURLToPath(import.meta.url));
const adapterPackage = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
const toolVersion = (name) => `${name}@${adapterPackage.dependencies[name]}`;
const ANGULAR_ACCESSIBILITY_RULES = [
  "alt-text",
  "click-events-have-key-events",
  "interactive-supports-focus",
  "label-has-associated-control",
  "mouse-events-have-key-events",
  "no-autofocus",
  "no-distracting-elements",
  "no-positive-tabindex",
  "role-has-required-aria",
  "table-scope",
  "valid-aria",
];
const ANGULAR_ACCESSIBILITY_RULE_IDS = new Set(
  ANGULAR_ACCESSIBILITY_RULES.map((name) => `@angular-eslint/template/${name}`),
);

function commandResult(command, { cwd, env = {}, allowFailure = false } = {}) {
  const childEnv = { ...process.env, ...env };
  for (const key of Object.keys(childEnv)) {
    if (key.startsWith("NODE_CHANNEL") || key.startsWith("NODE_TEST_")) delete childEnv[key];
  }
  return new Promise((resolve, reject) => {
    const child = spawn(command, {
      cwd,
      env: childEnv,
      shell: true,
      windowsHide: true,
    });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("error", reject);
    child.on("close", (code) => {
      const result = { command, exitCode: code ?? 2, stdout, stderr };
      if (code !== 0 && !allowFailure) {
        reject(Object.assign(new Error(`命令执行失败 (${code})：${command}`), { result }));
      } else resolve(result);
    });
  });
}

function pluginRules(plugin, configName = "recommended") {
  const config = plugin.configs?.[configName];
  if (Array.isArray(config)) return Object.assign({}, ...config.map(({ rules }) => rules ?? {}));
  if (config?.rules) return config.rules;
  const namespace = plugin.meta?.name
    ?.replace(/^eslint-plugin-/, "")
    .replace(/^@([^/]+)\/eslint-plugin-?/, "@$1/");
  if (!namespace) return {};
  return Object.fromEntries(
    Object.entries(plugin.rules ?? {})
      .filter(([, rule]) => rule.meta?.docs?.recommended)
      .map(([name]) => [`${namespace}/${name}`, "error"]),
  );
}

function relativeFiles(project, workspace, predicate) {
  return workspace.files
    .filter(predicate)
    .map((file) => path.relative(project.root, file).replaceAll("\\", "/"));
}

function scopePattern(pattern, workspacePath) {
  if (Array.isArray(pattern)) return pattern.map((item) => scopePattern(item, workspacePath));
  if (workspacePath === ".") return pattern;
  if (pattern.startsWith("!")) return `!${workspacePath}/${pattern.slice(1)}`;
  return `${workspacePath}/${pattern.replace(/^\.\//, "")}`;
}

function scopeNuxtConfig(config, project, workspace) {
  const fallbackFiles = relativeFiles(project, workspace, () => true);
  return {
    ...config,
    name: `${config.name ?? "nuxt/config"}@${workspace.relativePath}`,
    files: config.files
      ? config.files.map((pattern) => scopePattern(pattern, workspace.relativePath))
      : fallbackFiles,
    ...(config.ignores ? {
      ignores: config.ignores.map((pattern) => scopePattern(pattern, workspace.relativePath)),
    } : {}),
  };
}

async function eslintConfig(project, complexityLimit) {
  const configs = [
    {
      ignores: [
        "**/node_modules/**", "**/dist/**", "**/build/**", "**/coverage/**",
        "**/.next/**", "**/.nuxt/**", "**/.svelte-kit/**", "**/.new-paradigm/**",
        ...project.exclude,
      ],
    },
    {
      files: ["**/*.{js,cjs,mjs,jsx,ts,cts,mts,tsx}"],
      ...js.configs.recommended,
      languageOptions: {
        ecmaVersion: "latest",
        sourceType: "module",
        parserOptions: { ecmaFeatures: { jsx: true } },
        globals: { console: "readonly", process: "readonly", Buffer: "readonly" },
      },
      plugins: { sonarjs },
      rules: {
        ...pluginRules(sonarjs),
        "sonarjs/cognitive-complexity": ["error", complexityLimit],
      },
    },
    {
      files: ["**/*.{cjs,cts}"],
      languageOptions: { sourceType: "commonjs" },
    },
    {
      files: ["**/*.{ts,cts,mts,tsx}"],
      languageOptions: {
        parser: tsParser,
        parserOptions: { ecmaFeatures: { jsx: true } },
      },
    },
  ];
  for (const workspace of project.workspaces) {
    const frameworks = new Set(workspace.frameworks);
    const scriptFiles = relativeFiles(project, workspace, (file) => /\.[cm]?[jt]sx?$/.test(file));
    if (frameworks.has("react") || frameworks.has("next")) {
      configs.push({
        files: scriptFiles,
        plugins: { react, "react-hooks": reactHooks, "jsx-a11y": jsxA11y },
        settings: { react: { version: "detect" } },
        rules: {
          ...pluginRules(react),
          ...pluginRules(reactHooks, "recommended-latest"),
          ...pluginRules(jsxA11y),
          ...pluginRules(react, "jsx-runtime"),
          "react/react-in-jsx-scope": "off",
          "react/jsx-uses-react": "off",
        },
      });
    }
    if (frameworks.has("next")) {
      configs.push({
        files: scriptFiles,
        plugins: { "@next/next": next },
        rules: pluginRules(next, "core-web-vitals"),
      });
    }
    if (frameworks.has("vue") || frameworks.has("nuxt")) {
      configs.push({
        files: relativeFiles(project, workspace, (file) => file.endsWith(".vue")),
        languageOptions: {
          parser: vueParser,
          parserOptions: { parser: tsParser, extraFileExtensions: [".vue"], sourceType: "module" },
        },
        plugins: { vue, "vuejs-accessibility": vueA11y, sonarjs },
        rules: {
          ...pluginRules(vue, "essential"),
          ...pluginRules(vueA11y),
          "sonarjs/cognitive-complexity": ["error", complexityLimit],
        },
      });
    }
    if (frameworks.has("nuxt")) {
      const officialNuxtConfig = await createConfigForNuxt({
        features: { tooling: false, stylistic: false },
      });
      configs.push(...officialNuxtConfig
        .filter(({ name }) => name !== "gitignore")
        .map((config) => scopeNuxtConfig(config, project, workspace)));
    }
    if (frameworks.has("angular")) {
      configs.push({
        files: relativeFiles(project, workspace, (file) => /\.[cm]?tsx?$/.test(file)),
        plugins: { "@angular-eslint": angular },
        rules: pluginRules(angular),
      }, {
        files: relativeFiles(project, workspace, (file) => file.endsWith(".component.html")),
        languageOptions: { parser: angularTemplateParser },
        plugins: { "@angular-eslint/template": angularTemplate },
        rules: {
          ...pluginRules(angularTemplate),
          ...Object.fromEntries(ANGULAR_ACCESSIBILITY_RULES
            .filter((name) => angularTemplate.rules?.[name])
            .map((name) => [`@angular-eslint/template/${name}`, "error"])),
        },
      });
    }
    if (frameworks.has("svelte") || frameworks.has("sveltekit")) {
      configs.push({
        files: relativeFiles(project, workspace, (file) => file.endsWith(".svelte")),
        languageOptions: {
          parser: svelteParser,
          parserOptions: { parser: tsParser, extraFileExtensions: [".svelte"] },
        },
        plugins: { svelte, sonarjs },
        rules: {
          ...pluginRules(svelte, "flat/recommended"),
          "sonarjs/cognitive-complexity": ["error", complexityLimit],
        },
      });
    }
  }
  return configs.filter((config) => !Array.isArray(config.files) || config.files.length > 0);
}

export async function collectEslint(project, definition) {
  if (project.files.length === 0) return { applicable: false, reason: "未发现支持的源码" };
  const complexityLimit = definition.measurement.parameters.complexityLimit;
  const eslint = new ESLint({
    cwd: project.root,
    overrideConfigFile: true,
    overrideConfig: await eslintConfig(project, complexityLimit),
    errorOnUnmatchedPattern: true,
  });
  const results = await eslint.lintFiles(project.files);
  const messages = results.flatMap((result) => result.messages
    .filter(({ severity }) => severity === 2)
    .map((message) => ({
      file: path.relative(project.root, result.filePath),
      line: message.line,
      column: message.column,
      ruleId: message.ruleId,
      message: message.message,
    })));
  for (const file of project.files.filter((item) => item.endsWith(".svelte"))) {
    const source = await readFile(file, "utf8");
    const compiled = compileSvelte(source, { filename: file, generate: false });
    for (const warning of compiled.warnings.filter(({ code }) => code.startsWith("a11y_"))) {
      messages.push({
        file: path.relative(project.root, file),
        line: warning.start?.line ?? null,
        column: warning.start?.column ?? null,
        ruleId: `svelte/${warning.code.replaceAll("_", "-")}`,
        message: warning.message,
      });
    }
  }
  const complexity = messages.filter(({ ruleId }) => ruleId === "sonarjs/cognitive-complexity");
  const isAccessibility = ({ ruleId = "" }) => (
    ruleId.startsWith("jsx-a11y/")
    || ruleId.startsWith("vuejs-accessibility/")
    || ANGULAR_ACCESSIBILITY_RULE_IDS.has(ruleId)
    || ruleId.startsWith("svelte/a11y-")
  );
  const accessibility = messages.filter(isAccessibility);
  const staticMessages = messages.filter((message) => (
    message.ruleId !== "sonarjs/cognitive-complexity" && !isAccessibility(message)
  ));
  return {
    static: { value: staticMessages.length, evidence: { violations: staticMessages } },
    complexity: { value: complexity.length, evidence: { violations: complexity, complexityLimit } },
    accessibility: project.frontend
      ? { value: accessibility.length, evidence: { violations: accessibility } }
      : { applicable: false, reason: "未识别到受支持的前端框架" },
    toolVersion: ESLint.version,
  };
}

export async function collectTypescript(project, tsconfigPaths) {
  if (project.typescriptFiles.length === 0) return { applicable: false, reason: "未发现 TypeScript 源码" };
  if (!Array.isArray(tsconfigPaths) || tsconfigPaths.length === 0) {
    throw new Error("发现 TypeScript 源码，但所有适用 workspace 均未找到或指定 tsconfig");
  }
  const diagnosticsByKey = new Map();
  const configurations = [];
  for (const tsconfigPath of tsconfigPaths) {
    const configFile = ts.readConfigFile(tsconfigPath, ts.sys.readFile);
    if (configFile.error) throw new Error(ts.flattenDiagnosticMessageText(configFile.error.messageText, "\n"));
    const parsed = ts.parseJsonConfigFileContent(configFile.config, ts.sys, path.dirname(tsconfigPath));
    if (parsed.errors.length > 0) {
      throw new Error(parsed.errors.map((item) => ts.flattenDiagnosticMessageText(item.messageText, "\n")).join("\n"));
    }
    const program = ts.createProgram(parsed.fileNames, { ...parsed.options, noEmit: true });
    const diagnostics = ts.getPreEmitDiagnostics(program)
      .filter(({ category }) => category === ts.DiagnosticCategory.Error);
    configurations.push({
      tsconfig: path.relative(project.root, tsconfigPath) || "tsconfig.json",
      sourceFileCount: parsed.fileNames.length,
      diagnosticCount: diagnostics.length,
    });
    for (const diagnostic of diagnostics) {
      const key = [
        diagnostic.file?.fileName ?? "",
        diagnostic.start ?? "",
        diagnostic.code,
        ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"),
      ].join(":");
      diagnosticsByKey.set(key, diagnostic);
    }
  }
  const diagnostics = [...diagnosticsByKey.values()];
  const evidence = diagnostics.map((diagnostic) => {
    const position = diagnostic.file && diagnostic.start !== undefined
      ? diagnostic.file.getLineAndCharacterOfPosition(diagnostic.start)
      : null;
    return {
      file: diagnostic.file ? path.relative(project.root, diagnostic.file.fileName) : null,
      line: position ? position.line + 1 : null,
      code: diagnostic.code,
      message: ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"),
    };
  });
  return {
    value: diagnostics.length,
    evidence: { diagnostics: evidence, configurations },
    toolVersion: ts.version,
  };
}

function parseTestCounts(output) {
  const nodeTotal = output.match(/^\s*(?:#|ℹ)\s*tests\s+(\d+)\s*$/im)?.[1];
  const nodePassed = output.match(/^\s*(?:#|ℹ)\s*pass\s+(\d+)\s*$/im)?.[1];
  if (nodeTotal !== undefined && nodePassed !== undefined) {
    return { total: Number(nodeTotal), passed: Number(nodePassed) };
  }

  const jestLine = output.match(/^Tests:\s+(.+)$/im)?.[1];
  if (jestLine) {
    const total = Number(jestLine.match(/(\d+)\s+total/i)?.[1]);
    const passed = Number(jestLine.match(/(\d+)\s+passed/i)?.[1] ?? 0);
    if (Number.isInteger(total)) return { total, passed };
  }

  const vitestLine = output.match(/^\s*Tests\s+(.+)$/im)?.[1];
  if (vitestLine) {
    const total = Number(vitestLine.match(/\((\d+)\)/)?.[1]);
    const passed = Number(vitestLine.match(/(\d+)\s+passed/i)?.[1] ?? 0);
    if (Number.isInteger(total)) return { total, passed };
  }
  return null;
}

export async function collectTests(project, testCommand, execute = commandResult) {
  if (!testCommand) throw new Error("缺少必需的测试命令");
  const result = await execute(testCommand, { cwd: project.root, allowFailure: true });
  const counts = parseTestCounts(`${result.stdout}\n${result.stderr}`);
  if (!counts || counts.total === 0) throw new Error("测试输出未提供可确定的通过数和总数");
  return {
    value: counts.passed / counts.total * 100,
    evidence: { command: testCommand, exitCode: result.exitCode, ...counts },
    toolVersion: process.version,
  };
}

export async function collectCoverage(project, testCommand) {
  if (!testCommand) throw new Error("缺少覆盖率采集所需的测试命令");
  const temporary = await mkdtemp(path.join(os.tmpdir(), "np-c8-"));
  try {
    const executable = path.join(moduleDirectory, "..", "node_modules", ".bin", process.platform === "win32" ? "c8.cmd" : "c8");
    const command = `"${executable}" --reporter=json-summary --report-dir="${temporary}" ${testCommand}`;
    const result = await commandResult(command, { cwd: project.root, allowFailure: true });
    let summary;
    try {
      summary = JSON.parse(await readFile(path.join(temporary, "coverage-summary.json"), "utf8"));
    } catch (error) {
      throw new Error(`覆盖率结果不可用 (${result.exitCode})：${error.message}`, { cause: error });
    }
    const total = summary.total;
    if (!Number.isFinite(total?.lines?.pct) || !Number.isFinite(total?.branches?.pct) || !Number.isFinite(total?.functions?.pct)) {
      throw new Error(`覆盖率结果缺少行、分支或函数汇总 (${result.exitCode})`);
    }
    return {
      lines: { value: total.lines.pct, evidence: { ...total.lines, exitCode: result.exitCode } },
      branches: { value: total.branches.pct, evidence: { ...total.branches, exitCode: result.exitCode } },
      functions: { value: total.functions.pct, evidence: { ...total.functions, exitCode: result.exitCode } },
      toolVersion: toolVersion("c8"),
    };
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

export async function collectDuplication(project, definition) {
  if (project.sourceFiles.length < 2) return { applicable: false, reason: "源码不足两份，不形成可检测重复范围" };
  const temporary = await mkdtemp(path.join(os.tmpdir(), "np-jscpd-"));
  try {
    const executable = path.join(moduleDirectory, "..", "node_modules", ".bin", process.platform === "win32" ? "jscpd.cmd" : "jscpd");
    const parameters = definition.measurement.parameters;
    const ignored = ["**/node_modules/**", "**/.new-paradigm/**", ...project.exclude].join(",");
    const command = `"${executable}" . --reporters json --output "${temporary}" --min-tokens ${parameters.minimumTokens} --min-lines ${parameters.minimumLines} --ignore "${ignored}"`;
    await commandResult(command, { cwd: project.root, allowFailure: true });
    const report = JSON.parse(await readFile(path.join(temporary, "jscpd-report.json"), "utf8"));
    const percentage = report.statistics?.total?.percentage;
    if (typeof percentage !== "number") throw new Error("jscpd 未返回总重复率");
    if ((report.statistics?.total?.sources ?? 0) === 0) {
      return { applicable: false, reason: "源码未达到指标定义的最小令牌和最小重复行参数", toolVersion: toolVersion("jscpd") };
    }
    return {
      value: percentage,
      evidence: { statistics: report.statistics.total, parameters },
      toolVersion: toolVersion("jscpd"),
    };
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

export async function collectCycles(project, tsconfigPaths = []) {
  if (project.sourceFiles.length < 2) return { applicable: false, reason: "少于两个源码模块" };
  const executable = path.join(moduleDirectory, "..", "node_modules", ".bin", process.platform === "win32" ? "depcruise.cmd" : "depcruise");
  const options = [
    `"${executable}"`,
    ...project.sourceFiles.map((file) => `"${file}"`),
    "--output-type json",
    ...(tsconfigPaths[0] ? [`--ts-config "${tsconfigPaths[0]}"`] : []),
    "--no-config",
  ];
  const result = await commandResult(options.join(" "), { cwd: project.root });
  const report = JSON.parse(result.stdout);
  const graph = new Map((report.modules ?? []).map((module) => [
    module.source,
    (module.dependencies ?? []).filter(({ resolved }) => resolved).map(({ resolved }) => resolved),
  ]));
  const cyclesByKey = new Map();
  const visiting = [];
  const active = new Set();
  function visit(module) {
    if (active.has(module)) {
      const cycle = visiting.slice(visiting.indexOf(module));
      const rotations = cycle.map((_, index) => [...cycle.slice(index), ...cycle.slice(0, index)]);
      const canonical = rotations.map((item) => item.join(" -> ")).sort()[0];
      cyclesByKey.set(canonical, cycle);
      return;
    }
    active.add(module);
    visiting.push(module);
    for (const dependency of graph.get(module) ?? []) {
      if (graph.has(dependency)) visit(dependency);
    }
    visiting.pop();
    active.delete(module);
  }
  for (const module of graph.keys()) visit(module);
  const cycles = [...cyclesByKey.values()];
  return { value: cycles.length, evidence: { cycles }, toolVersion: toolVersion("dependency-cruiser") };
}

function parseJsonDocuments(output) {
  try {
    return [JSON.parse(output)];
  } catch {
    return output.split(/\r?\n/).filter(Boolean).map((line) => {
      try {
        return JSON.parse(line);
      } catch {
        return null;
      }
    }).filter(Boolean);
  }
}

export function normalizeAuditOutput(output) {
  const documents = parseJsonDocuments(output);
  for (const report of documents) {
    const counts = report.metadata?.vulnerabilities ?? report.data?.vulnerabilities;
    if (Number.isInteger(counts?.high) && Number.isInteger(counts?.critical)) {
      return { high: counts.high, critical: counts.critical };
    }
  }
  const advisories = documents.flatMap((report) => {
    if (report.type === "auditAdvisory" && report.data?.advisory) return [report.data.advisory];
    if (report.advisories) return Object.values(report.advisories);
    if (Array.isArray(report.vulnerabilities)) return report.vulnerabilities;
    if (report.vulnerabilities && typeof report.vulnerabilities === "object") {
      return Object.values(report.vulnerabilities);
    }
    return [];
  });
  if (advisories.length === 0) return null;
  return {
    high: advisories.filter(({ severity }) => severity === "high").length,
    critical: advisories.filter(({ severity }) => severity === "critical").length,
  };
}

export async function collectAudit(project, execute = commandResult) {
  const commands = {
    npm: ["npm audit --json"],
    pnpm: ["pnpm audit --json"],
    yarn: ["yarn npm audit --json", "yarn audit --json"],
  };
  if (!commands[project.packageManager]) {
    throw new Error(`不支持的 packageManager：${project.packageManager}`);
  }
  if (!project.lockfile) return { applicable: false, reason: "未发现受支持的锁文件" };
  let counts = null;
  let selectedCommand = null;
  for (const command of commands[project.packageManager]) {
    const result = await execute(command, { cwd: project.root, allowFailure: true });
    counts = normalizeAuditOutput(result.stdout);
    if (counts) {
      selectedCommand = command;
      break;
    }
  }
  if (!counts) throw new Error(`${project.packageManager} audit 未返回可归一化的 JSON`);
  return {
    value: counts.high + counts.critical,
    evidence: {
      packageManager: project.packageManager,
      lockfile: project.lockfile,
      command: selectedCommand,
      ...counts,
    },
    toolVersion: project.packageManager,
  };
}

export { commandResult, parseTestCounts };
