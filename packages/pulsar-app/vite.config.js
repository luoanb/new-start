import { defineConfig, loadEnv } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";

const host = process.env.TAURI_DEV_HOST;

// WebKitGTK 会把 dev server 的响应写入磁盘缓存（~/.local/share/<id>/WebKitCache），
// 重启后按 URL 复用旧版 CSS/JS 导致样式错乱。dev server 不走 tauri:// 协议，
// Rust 侧 on_web_resource_request 拦截不到，因此在 dev 中间件统一加 no-store。
function noStoreDev() {
  return {
    name: "no-store-dev",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use((_req, res, next) => {
        res.setHeader("Cache-Control", "no-store");
        next();
      });
    },
  };
}

// https://vite.dev/config/
export default defineConfig(async ({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "");
  const port = Number(env.DEV_PORT || 1430);
  const hmrPort = Number(env.DEV_HMR_PORT || port + 1);

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
