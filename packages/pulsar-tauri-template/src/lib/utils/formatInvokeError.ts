/** Format Tauri invoke / unknown errors for UI (avoid `[object Object]`). */
export function formatInvokeError(error: unknown): string {
  if (error == null) return "Unknown error";
  if (typeof error === "string") return error;
  if (typeof error === "number" || typeof error === "boolean") return String(error);

  if (typeof error === "object") {
    const record = error as Record<string, unknown>;

    // Tauri AppErrorPayload: { code, message }
    const message =
      typeof record.message === "string"
        ? record.message
        : typeof record.error === "string"
          ? record.error
          : undefined;
    const code =
      typeof record.code === "string"
        ? record.code
        : typeof record.error_code === "string"
          ? record.error_code
          : undefined;

    if (message) {
      return code ? `[${code}] ${message}` : message;
    }

    // Nested { error: { message, code } }
    if (record.error && typeof record.error === "object") {
      return formatInvokeError(record.error);
    }

    try {
      return JSON.stringify(error);
    } catch {
      return Object.prototype.toString.call(error);
    }
  }

  return String(error);
}
