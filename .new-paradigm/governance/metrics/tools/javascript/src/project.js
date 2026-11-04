import { access, readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { parse as parseYaml } from "yaml";

const SOURCE_EXTENSIONS = new Set([".js", ".cjs", ".mjs", ".jsx", ".ts", ".cts", ".mts", ".tsx", ".vue", ".svelte"]);
const ALWAYS_IGNORED = new Set(["node_modules", ".git", ".new-paradigm", "dist", "build", "coverage", ".next", ".nuxt", ".svelte-kit"]);

async function exists(file) {
  try {
    await access(file);
    return true;
  } catch {
    return false;
  }
}

async function readPackage(directory) {
  try {
    return JSON.parse(await readFile(path.join(directory, "package.json"), "utf8"));
  } catch (error) {
    throw new Error(`无法读取 package.json：${directory}：${error.message}`, { cause: error });
  }
}

function packageNames(pkg) {
  return new Set([
    ...Object.keys(pkg.dependencies ?? {}),
    ...Object.keys(pkg.devDependencies ?? {}),
    ...Object.keys(pkg.peerDependencies ?? {}),
  ]);
}

export function identifyFrameworks(pkg) {
  const names = packageNames(pkg);
  const frameworks = [];
  if (names.has("next")) frameworks.push("next");
  else if (names.has("react")) frameworks.push("react");
  if (names.has("nuxt")) frameworks.push("nuxt");
  else if (names.has("vue")) frameworks.push("vue");
  if (names.has("@angular/core")) frameworks.push("angular");
  if (names.has("@sveltejs/kit")) frameworks.push("sveltekit");
  else if (names.has("svelte")) frameworks.push("svelte");
  return frameworks.length > 0 ? frameworks : ["vanilla"];
}

function packageWorkspacePatterns(pkg) {
  if (Array.isArray(pkg.workspaces)) return pkg.workspaces;
  if (Array.isArray(pkg.workspaces?.packages)) return pkg.workspaces.packages;
  return [];
}

async function workspacePatterns(root, pkg) {
  const manifestPatterns = packageWorkspacePatterns(pkg);
  const pnpmWorkspace = path.join(root, "pnpm-workspace.yaml");
  if (!(await exists(pnpmWorkspace))) return manifestPatterns;
  let document;
  try {
    document = parseYaml(await readFile(pnpmWorkspace, "utf8"));
  } catch (error) {
    throw new Error(`无法解析 pnpm-workspace.yaml：${error.message}`, { cause: error });
  }
  if (!Array.isArray(document?.packages) || document.packages.some((item) => typeof item !== "string")) {
    throw new Error("pnpm-workspace.yaml 必须包含字符串 packages 数组");
  }
  return [...new Set([...manifestPatterns, ...document.packages])];
}

async function expandWorkspacePattern(root, pattern) {
  const normalized = pattern.replaceAll("\\", "/").replace(/\/+$/, "");
  if (!normalized.includes("*")) return [path.resolve(root, normalized)];
  const segments = normalized.split("/");
  let directories = [root];
  for (const segment of segments) {
    const next = [];
    for (const directory of directories) {
      if (segment === "*") {
        for (const entry of await readdir(directory, { withFileTypes: true }).catch(() => [])) {
          if (entry.isDirectory() && !ALWAYS_IGNORED.has(entry.name)) next.push(path.join(directory, entry.name));
        }
      } else if (!segment.includes("*")) {
        next.push(path.join(directory, segment));
      } else {
        const regex = new RegExp(`^${segment.replace(/[.+?^${}()|[\]\\]/g, "\\$&").replaceAll("*", ".*")}$`);
        for (const entry of await readdir(directory, { withFileTypes: true }).catch(() => [])) {
          if (entry.isDirectory() && regex.test(entry.name)) next.push(path.join(directory, entry.name));
        }
      }
    }
    directories = next;
  }
  return directories;
}

function matchesPattern(relative, patterns) {
  const value = relative.replaceAll("\\", "/");
  return patterns.some((pattern) => {
    const normalized = pattern.replaceAll("\\", "/").replace(/^\.\//, "");
    const regex = new RegExp(`^${normalized
      .replace(/[.+?^${}()|[\]\\]/g, "\\$&")
      .replaceAll("**", "\0")
      .replaceAll("*", "[^/]*")
      .replaceAll("\0", ".*")}$`);
    return regex.test(value) || value.startsWith(`${normalized.replace(/\/\*\*$/, "")}/`);
  });
}

async function walk(root, directory, include, exclude, files) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const absolute = path.join(directory, entry.name);
    const relative = path.relative(root, absolute).replaceAll("\\", "/");
    if (entry.isDirectory()) {
      if (!ALWAYS_IGNORED.has(entry.name) && !matchesPattern(relative, exclude)) {
        await walk(root, absolute, include, exclude, files);
      }
    } else if (
      (SOURCE_EXTENSIONS.has(path.extname(entry.name)) || entry.name.endsWith(".component.html"))
      && !matchesPattern(relative, exclude)
      && (include.length === 0 || matchesPattern(relative, include))
    ) {
      files.push(absolute);
    }
  }
}

export async function inspectProject(projectRoot, { include = [], exclude = [] } = {}) {
  const root = path.resolve(projectRoot);
  const rootPackage = await readPackage(root);
  const workspaceDirectories = new Set([root]);
  for (const pattern of await workspacePatterns(root, rootPackage)) {
    for (const directory of await expandWorkspacePattern(root, pattern)) {
      if (await exists(path.join(directory, "package.json"))) workspaceDirectories.add(directory);
    }
  }

  const workspaces = [];
  for (const directory of workspaceDirectories) {
    const pkg = directory === root ? rootPackage : await readPackage(directory);
    workspaces.push({
      path: directory,
      relativePath: path.relative(root, directory).replaceAll("\\", "/") || ".",
      package: pkg,
      frameworks: identifyFrameworks(pkg),
    });
  }

  const files = [];
  await walk(root, root, include, exclude, files);
  const sortedWorkspaces = workspaces.toSorted((left, right) => right.path.length - left.path.length);
  for (const workspace of workspaces) workspace.files = [];
  for (const file of files) {
    const workspace = sortedWorkspaces.find(({ path: workspacePath }) => (
      file === workspacePath || file.startsWith(`${workspacePath}${path.sep}`)
    ));
    workspace?.files.push(file);
  }
  const frameworkSet = new Set(workspaces.flatMap(({ frameworks }) => frameworks));
  for (const workspace of workspaces) {
    const frameworks = new Set(workspace.frameworks);
    if (workspace.files.some((file) => file.endsWith(".vue"))
      && ![...frameworks].some((name) => ["vue", "nuxt"].includes(name))) {
      throw new Error(`workspace ${workspace.relativePath} 发现 Vue SFC，但无法确定 Vue/Nuxt 解析配置`);
    }
    if (workspace.files.some((file) => file.endsWith(".svelte"))
      && ![...frameworks].some((name) => ["svelte", "sveltekit"].includes(name))) {
      throw new Error(`workspace ${workspace.relativePath} 发现 Svelte 组件，但无法确定 Svelte 解析配置`);
    }
    if (workspace.files.some((file) => file.endsWith(".component.html")) && !frameworks.has("angular")) {
      throw new Error(`workspace ${workspace.relativePath} 发现 Angular 模板，但无法确定 Angular 解析配置`);
    }
  }

  const lockfiles = ["package-lock.json", "pnpm-lock.yaml", "yarn.lock"];
  const lockfile = (await Promise.all(lockfiles.map(async (name) => ({
    name,
    present: await exists(path.join(root, name)),
  })))).find(({ present }) => present)?.name ?? null;
  const declaredManager = rootPackage.packageManager?.split("@")[0];
  const packageManager = declaredManager
    ?? (lockfile === "pnpm-lock.yaml" ? "pnpm" : lockfile === "yarn.lock" ? "yarn" : "npm");

  return {
    root,
    package: rootPackage,
    workspaces,
    frameworks: [...frameworkSet].sort(),
    files,
    sourceFiles: files.filter((file) => !file.endsWith(".html")),
    typescriptFiles: files.filter((file) => /\.[cm]?tsx?$/.test(file)),
    frontend: [...frameworkSet].some((name) => name !== "vanilla"),
    packageManager,
    lockfile,
    include,
    exclude,
  };
}

export async function resolveTsconfigs(project, requested) {
  if (requested) {
    const resolved = path.resolve(project.root, requested);
    if (!(await exists(resolved))) throw new Error(`指定的 tsconfig 不存在：${resolved}`);
    return [resolved];
  }
  const configurations = [];
  const missing = [];
  const rootConfiguration = path.join(project.root, "tsconfig.json");
  const hasRootConfiguration = await exists(rootConfiguration);
  for (const workspace of project.workspaces) {
    if (!workspace.files.some((file) => /\.[cm]?tsx?$/.test(file))) continue;
    const candidate = path.join(workspace.path, "tsconfig.json");
    if (await exists(candidate)) configurations.push(candidate);
    else if (workspace.path !== project.root && hasRootConfiguration) configurations.push(rootConfiguration);
    else missing.push(workspace.relativePath);
  }
  if (missing.length > 0) throw new Error(`以下 TypeScript workspace 缺少 tsconfig：${missing.join(", ")}`);
  return [...new Set(configurations)];
}

export { matchesPattern, workspacePatterns };
