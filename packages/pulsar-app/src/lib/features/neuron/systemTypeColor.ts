/**
 * 系统类型徽标色板映射（对齐 app.html 的 `--color-system-*` 令牌）。
 * - `session.*` → 核心色（会话规格）
 * - `assistant_*` → 助手色
 * - 其余 → 按类型名回落（未定义的名称经 var() 回落默认色）
 */
export function systemTypeColor(type: string | null | undefined): string {
  if (!type) return "var(--color-system-default)";
  if (type.startsWith("session.")) return "var(--color-system-core)";
  if (type.startsWith("assistant_")) return "var(--color-system-assistant)";
  return `var(--color-system-${type}, var(--color-system-default))`;
}
