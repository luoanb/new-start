import { readFileSync } from "node:fs";
import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import Icons from "unplugin-icons/vite";

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
export default defineConfig(async () => {
  // dev 端口已由 devUrl 单一来源决定，见文件顶部 tauriConf/devPort。
  return {
    plugins: [
      sveltekit(),
      // 构建期图标：把 `~icons/lucide/*` 编译为本地 Svelte 组件（离线可用、按需打包）。
      // 必须显式设 compiler: "svelte"，否则 unplugin-icons 产出 JSX/HTML 而非 Svelte 组件。
      Icons({ compiler: "svelte" }),
      noStoreDev(),
    ],

    // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
    //
    // 1. prevent Vite from obscuring rust errors
    clearScreen: false,
    // 2. tauri expects a fixed port, fail if that port is not available
    server: {
      port: devPort,
      strictPort: true,
      host: host || false,
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
