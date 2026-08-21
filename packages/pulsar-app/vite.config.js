import { defineConfig, loadEnv } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";

const host = process.env.TAURI_DEV_HOST;

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
  const port = Number(env.DEV_PORT || 1432);
  const hmrPort = Number(env.DEV_HMR_PORT || port + 1);
  // dev 下后端 API 统一走 /api 前缀（后端路由见 net::router），vite 将其代理到
  // pulsar-server，前端以同源方式自动发现并连接（与 prod 由 server 托管的行为一致）。
  // dev 后端固定跑在 8899（见 package.json server:dev），与生产 9999 分开，
  // 避免开发与生产进程抢占同一端口；需要自定义时用 DEV_PROXY_TARGET 覆盖。
  const proxyTarget = env.DEV_PROXY_TARGET || "http://127.0.0.1:8899";

  return {
    plugins: [sveltekit(), noStoreDev()],

    // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
    //
    // 1. prevent Vite from obscuring rust errors
    clearScreen: false,
    // 2. tauri expects a fixed port, fail if that port is not available
    server: {
      port,
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
            port: hmrPort,
          }
        : undefined,
      watch: {
        // 3. tell Vite to ignore watching `src-tauri`
        ignored: ["**/src-tauri/**"],
      },
    },
  };
});
