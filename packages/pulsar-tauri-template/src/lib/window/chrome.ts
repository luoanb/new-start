/**
 * 窗口 chrome：无边框窗口（`tauri.conf.json` 的 `decorations: false`）的自绘标题栏 + 边缘缩放。
 *
 * 运行环境判定**全部收敛在本模块内部**：Tauri 环境下代理到原生窗口，非 Tauri 环境
 * （浏览器预览）降级为 no-op 桩。上层只调 `chrome.minimize()` 这类方法、只读
 * `chrome.available`，不自己判断运行环境（原先散落在调用点的 `isTauriEnv` 已移除）。
 */
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow, type Window } from "@tauri-apps/api/window";

export interface WindowChrome {
  /** 当前是否运行在 Tauri 环境；false 时其余方法均为 no-op 桩。 */
  readonly available: boolean;
  minimize(): void;
  toggleMaximize(): void;
  close(): void;
  isMaximized(): Promise<boolean>;
  /** 订阅窗口尺寸变化（最大化态同步用）；返回退订函数，非 Tauri 下为 no-op。 */
  onResized(handler: () => void): () => void;
  /** 标题栏空白区按下 → 拖拽窗口；命中交互元素或非左键时忽略。 */
  startDrag(event: MouseEvent): void;
  /** 标题栏空白区双击 → 最大化⇄还原；命中交互元素时忽略（Linux 上 Tauri 不自动处理）。 */
  toggleMaximizeOnDoubleClick(event: MouseEvent): void;
}

/** 标题栏交互元素判定：命中 button/a/input 等时不做窗口拖拽，避免「点按钮变成拖窗口」。 */
function isInteractive(target: EventTarget | null): boolean {
  return !!(target as HTMLElement | null)?.closest?.(
    "button, a, input, select, textarea, [role='button']",
  );
}

/** no-op 桩：非 Tauri 环境下所有窗口操作静默失效。 */
function noopChrome(): WindowChrome {
  return {
    available: false,
    minimize: () => {},
    toggleMaximize: () => {},
    close: () => {},
    isMaximized: async () => false,
    onResized: () => () => {},
    startDrag: () => {},
    toggleMaximizeOnDoubleClick: () => {},
  };
}

function createWindowChrome(): WindowChrome {
  if (!isTauri()) return noopChrome();

  const appWindow: Window = getCurrentWindow();
  return {
    available: true,
    minimize: () => void appWindow.minimize(),
    toggleMaximize: () => void appWindow.toggleMaximize(),
    close: () => void appWindow.close(),
    isMaximized: () => appWindow.isMaximized(),
    onResized: (handler) => {
      let unlisten: (() => void) | null = null;
      void appWindow.onResized(handler).then((fn) => (unlisten = fn));
      return () => unlisten?.();
    },
    startDrag: (event) => {
      if (event.button !== 0 || isInteractive(event.target)) return;
      void appWindow.startDragging();
    },
    toggleMaximizeOnDoubleClick: (event) => {
      if (isInteractive(event.target)) return;
      void appWindow.toggleMaximize();
    },
  };
}

/** 窗口 chrome 单例。 */
export const chrome: WindowChrome = createWindowChrome();
