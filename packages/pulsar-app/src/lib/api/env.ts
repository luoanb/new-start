/**
 * Tauri 环境检测（独立无依赖模块，供 api / 组件共同引用）。
 *
 * Tauri 运行时在 window 注入 `__TAURI_INTERNALS__`；浏览器 / 其他 WebView 宿主缺失。
 * 判定非 Tauri 时：应用退化为「仅远程访问」，本机 IPC 路径不可用，Tauri 专属 UI（窗口控制等）隐藏。
 */
export const isTauriEnv: boolean =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
