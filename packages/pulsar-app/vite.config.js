import { readFileSync } from "node:fs";
import { defineConfig, loadEnv } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";

const host = process.env.TAURI_DEV_HOST;

// 前端 dev 端口单一事实源：
//  - 默认：tauri.conf.json 的 build.devUrl（Tauri 只认它，不支持 env 替换）
//  - 自定义：DEV_FRONT_PORT 环境变量（由 scripts/dev-tauri.mjs 注入，tauri 用 --config 同步，
//    保证 vite 与 tauri 读同一个端口）。未设置回落 devUrl，杜绝 url 错位。
const tauriConf = JSON.parse(
  readFileSync(new URL("./src-tauri/tauri.conf.json", import.meta.url), "utf8"),
);
const devPort = Number(process.env.DEV_FRONT_PORT) || Number(new URL(tauriConf.build.devUrl).port);

// WebKitGTK 会把 dev server 的响应写入磁盘缓存（~/.local/share/<id>/WebKitCache），
// 重启后按 URL 复用旧版 CSS/JS 导致样式错乱。dev server 不走 tauri:// 协议，
// Rust 侧 on_web_resource_request 拦截不到，因此在 dev 中间件统一加 no-store。
function noStoreDev() {
  return /** @type {import("vite").Plugin} */ ({
    name: "no-store-dev",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use((_req, res, next) => {
        res.setHeader("Cache-Control", "no-store");
        next();
      });
    },
  });
}

// https://vite.dev/config/
export default defineConfig(async ({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "");
  // dev 端口已由 devUrl 单一来源决定，见文件顶部 tauriConf/devPort。
  // dev 后端端口单一来源 PULSAR_PORT（Rust 端 core::config 与 vite 代理读同一环境变量，
  // 由 scripts/dev-tauri.mjs 注入；默认 8899，可用 --backend-port 自定义）。
  // 生产 9999 分开，避免开发与生产进程抢占同一端口；仍可用 DEV_PROXY_TARGET 显式覆盖。
  const backPort = process.env.PULSAR_PORT || 8899;
  const proxyTarget = env.DEV_PROXY_TARGET || `http://127.0.0.1:${backPort}`;

  return {
    plugins: [sveltekit(), noStoreDev()],

    // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
    //
    // 1. prevent Vite from obscuring rust errors
    clearScreen: false,
    // 2. tauri expects a fixed port, fail if that port is not available
    server: {
      port: devPort,
      strictPort: true,
      host: host || false,
      proxy: {
        "/api": {
          target: proxyTarget,
          changeOrigin: true,
          // 终端 WS 网关（/api/ws）也是同源访问：http-proxy 需显式开启
          // upgrade 转发，否则浏览器 WebSocket 握手（101）在 dev 下失败。
          ws: true,
        },
      },
      hmr: host
        ? {
            protocol: "ws",
            host,
            port: devPort + 1,
          }
        : undefined,
      watch: {
        // 3. tell Vite to ignore watching `src-tauri`
        ignored: ["**/src-tauri/**"],
      },
    },
  };
});
