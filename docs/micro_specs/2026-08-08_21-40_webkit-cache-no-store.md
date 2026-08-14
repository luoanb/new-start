# Spec: 修复 Tauri 重启后样式错乱（WebKitGTK 磁盘缓存）

## Goal

- 消除 Linux（WebKitGTK）下 pulsar-app 重启后 UI 样式错乱问题：WebKitGTK 把旧版 `index.html` / CSS / JS 缓存在磁盘上，重启后复用旧资源与新页面混搭渲染。

## 背景事实（根因证据）

- WebKitGTK 为每个应用维护**跨重启持久**的 HTTP 磁盘缓存：`~/.local/share/com.pulsar.app/WebKitCache`（实测 108M），按 URL 命中。
- Tauri 内置 `tauri://` 资源协议**不设置任何缓存头**（`Cache-Control`/`ETag`/`Last-Modified`），WebKit 只能靠启发式缓存，旧资源长期复用；这是已知 gotcha（业界同类案例：mainframe #453）。
- Vite dev 模式模块/CSS URL 稳定（无内容哈希），重启后同 URL 命中旧缓存。
- `on_web_resource_request` 仅在 `tauri://` 协议生效，**dev server（http://localhost:1430）不触发**（tauri 2.11 源码注释明确），因此 dev/prod 需分别处理。

## Done Contract

- 生产（打包）：`tauri://` 资源响应统一附加 `Cache-Control: no-store`，WebKit 不再磁盘缓存。
- 开发（`pnpm tauri dev`）：Vite dev server 所有响应统一附加 `Cache-Control: no-store`。
- 重启后样式与最新构建一致；不再出现旧 CSS 与新 HTML 混搭。
- `cargo check`、前端构建通过；删除一次存量 `WebKitCache` 后无需再手动清缓存。

## 改动点

| 文件 | 改动 |
|---|---|
| `packages/pulsar-app/src-tauri/tauri.conf.json` | 主窗口配置加 `"create": false`（改为 setup 内手动建窗，以便挂资源请求钩子） |
| `packages/pulsar-app/src-tauri/src/lib.rs` | `setup` 末尾用 `WebviewWindowBuilder::from_config` 重建主窗口，`.on_web_resource_request` 对所有响应加 `Cache-Control: no-store` |
| `packages/pulsar-app/vite.config.js` | 新增 dev-only 插件：`configureServer` 中间件对所有响应设 `Cache-Control: no-store` |

## 兼容性

- 窗口参数（title/size/devtools/decorations）由 `from_config` 原样继承，行为不变。
- 仅清除 HTTP 缓存语义，不影响 `localstorage`（主题/布局持久化保留）。
- `Cache-Control: no-store` 对本地读取的资源请求开销可忽略。

## Validation

- `cargo check` 通过（src-tauri）。
- 前端 `pnpm --filter pulsar-app build` 通过。
- 删除存量 `~/.local/share/com.pulsar.app/WebKitCache` 一次；连续重启 ≥2 次，样式与最新构建一致。

## Change Log / Validation（2026-08-08）

- `cargo check`：通过。
- `pnpm --filter pulsar-app build`：构建成功（adapter-static 输出到 build/）。
- dev server 实测：`curl -sI http://localhost:1430/` 响应头已含 `Cache-Control: no-store`（vite 配置变更触发自动重启并加载中间件）。
- 存量缓存清理：`~/.local/share/com.pulsar.app/WebKitCache`（108M → 140K），`localstorage` 保留。
- 待用户验证：重启 `pnpm tauri dev` 连续 2 次，样式不再错乱。
