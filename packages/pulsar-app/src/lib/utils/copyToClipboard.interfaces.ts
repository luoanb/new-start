/**
 * 剪贴板复制公共能力 —— 契约（静态侧接口）。
 *
 * 统一封装"复制文本到剪贴板"，内部自动处理：
 *  - 优先使用异步 Clipboard API（安全上下文可用时）
 *  - 失败/不可用时回退到 `execCommand("copy")` 的同步兜底
 *    （非安全上下文 / Tauri WebKitGTK webview / 旧浏览器）
 *
 * 调用约定：`copyText` 返回 `Promise<boolean>`，成功为 true，失败为 false。
 * 返回值用于 UI 层的"复制成功/失败"反馈，不在此方法内抛出异常。
 */
export interface CopyToClipboardStatic {
  /**
   * 将文本复制到剪贴板。
   * @param text 要复制的文本
   * @returns 复制是否成功（true=成功，false=失败）
   */
  copyText(text: string): Promise<boolean>;
}
