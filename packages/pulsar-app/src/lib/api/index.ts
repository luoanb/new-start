/**
 * API 客户端工厂：按连接配置选择本机（Tauri IPC）或远程（HTTP + SSE）客户端。
 *
 * - 连接配置存 localStorage：`pulsar:connMode`（local | remote）、`pulsar:remoteUrl`、`pulsar:remoteToken`。
 * - `api` 是模块级 live binding：运行时 `switchConn` 切换后，所有 import 方自动使用新实例，
 *   业务层（dataStore / 组件）无感知；切换后需重新 `dataStore.bootstrap()`。
 */
import { createHttpClient } from "./httpClient";
import { tauriClient } from "./tauriClient";
import { isTauriEnv } from "./env";
import type { ApiClient, ConnConfig, ServerInfo } from "./types";

const MODE_KEY = "pulsar:connMode";
const URL_KEY = "pulsar:remoteUrl";
const TOKEN_KEY = "pulsar:remoteToken";

/**
 * 远程模式默认地址：构建期由 `PUBLIC_REMOTE_URL` 注入（.env / 部署环境管理），
 * 未注入时兜底本机默认端口。非 Tauri 且页面由 pulsar-server 托管时，
 * 启动阶段的 `discoverRemote()` 会以同源自动发现覆盖此值（见 +page.svelte）。
 */
const injectedRemoteUrl: string | undefined = (
  import.meta.env as Record<string, string | undefined>
).PUBLIC_REMOTE_URL;
export const DEFAULT_REMOTE_URL = injectedRemoteUrl || "http://127.0.0.1:9999";

export { isTauriEnv } from "./env";

export function readConnConfig(): ConnConfig {
  if (typeof localStorage === "undefined") return { mode: "local" };
  const mode = localStorage.getItem(MODE_KEY);
  return {
    mode: mode === "remote" ? "remote" : "local",
    url: localStorage.getItem(URL_KEY) ?? undefined,
    token: localStorage.getItem(TOKEN_KEY) ?? undefined,
  };
}

export function writeConnConfig(cfg: ConnConfig): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(MODE_KEY, cfg.mode);
  if (cfg.url) localStorage.setItem(URL_KEY, cfg.url);
  else localStorage.removeItem(URL_KEY);
  if (cfg.token) localStorage.setItem(TOKEN_KEY, cfg.token);
  else localStorage.removeItem(TOKEN_KEY);
}

export function createClient(cfg: ConnConfig): ApiClient {
  if (cfg.mode === "remote") {
    if (!cfg.url) {
      throw new Error("远程模式需要 pulsar:remoteUrl（如 http://127.0.0.1:9999）");
    }
    return createHttpClient(cfg);
  }
  // 本机 IPC 仅 Tauri 可用：非 Tauri 环境（浏览器等）忽略本机配置，强制走远程，
  // 未配置地址时回落默认地址。
  if (!isTauriEnv) {
    return createHttpClient({
      mode: "remote",
      url: cfg.url ?? DEFAULT_REMOTE_URL,
      token: cfg.token,
    });
  }
  return tauriClient;
}

// 模块初始化：非 Tauri 环境即使 localStorage 存的是本机模式，也进入远程模式。
let initial = readConnConfig();
if (!isTauriEnv && initial.mode === "local") {
  initial = { mode: "remote", url: initial.url ?? DEFAULT_REMOTE_URL, token: initial.token };
}
/** 当前 API 客户端实例（live binding，运行时切换后 import 方自动生效）。 */
export let api: ApiClient = createClient(initial);

/** 运行时切换连接模式：写回 localStorage 并替换 api 实例（业务层随后重新 bootstrap）。 */
export function switchConn(cfg: ConnConfig): void {
  writeConnConfig(cfg);
  api = createClient(cfg);
}

/** 读取当前连接配置（供设置 UI 回显）。 */
export function currentConn(): ConnConfig {
  return readConnConfig();
}

/**
 * 同源自动发现：非 Tauri 环境且用户未显式配置远程地址时，探测当前页面来源
 * （location.origin）是否就是 pulsar-server 托管的页面——`GET /api/config` 可达即证明。
 * 命中则返回该 origin（前端零写死端口），否则返回 null 保持现有配置。
 */
export async function discoverRemote(): Promise<string | null> {
  if (isTauriEnv) return null;
  if (readConnConfig().url) return null; // 用户已显式配置过，不覆盖
  try {
    const res = await fetch(`${location.origin}/api/config`, { cache: "no-store" });
    if (!res.ok) return null;
    const info = (await res.json()) as ServerInfo;
    return info.enabled ? location.origin : null;
  } catch {
    return null;
  }
}

export type { ApiClient, ConnConfig, ServerInfo, StateChangePayload, StateEventKind } from "./types";
export { STATE_CHANGED_EVENT } from "./types";
export { createHttpClient } from "./httpClient";
export { c, type Contract } from "./contracts";
