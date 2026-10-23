import type { CopyToClipboardStatic } from "./copyToClipboard.interfaces";

/** 编译期校验静态侧契约：缺方法/签名不匹配即报错。 */
function staticImplements<T>() {
  return <U extends T>(ctor: U) => ctor;
}

/**
 * 剪贴板复制公共工具类。
 *
 * 统一复制入口，避免各组件各自实现/遗漏兜底导致复制按钮点击无效。
 * 优先 Clipboard API，失败时回退 `execCommand("copy")`，永不抛异常。
 */
class CopyToClipboard {
  /**
   * 将文本复制到剪贴板，返回是否成功。
   *
   * - 优先 `navigator.clipboard.writeText`（需安全上下文，如 Tauri 生产环境）；
   * - 不可用/失败时回退隐藏 textarea + `document.execCommand("copy")`；
   * - 两种方案均失败时返回 false，交由 UI 层提示。
   */
  static async copyText(text: string): Promise<boolean> {
    if (!text) return false;

    // 方案一：异步 Clipboard API
    if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
      try {
        await navigator.clipboard.writeText(text);
        return true;
      } catch {
        // 继续走回退
      }
    }

    // 方案二：同步 execCommand 兜底
    try {
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.setAttribute("readonly", "");
      ta.style.position = "fixed";
      ta.style.top = "0";
      ta.style.left = "0";
      ta.style.opacity = "0";
      ta.style.pointerEvents = "none";
      document.body.appendChild(ta);
      ta.focus();
      ta.select();
      const ok = document.execCommand("copy");
      ta.remove();
      return ok;
    } catch {
      return false;
    }
  }
}

staticImplements<CopyToClipboardStatic>()(CopyToClipboard);

export { CopyToClipboard };
