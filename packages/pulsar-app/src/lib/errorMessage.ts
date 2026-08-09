/** Extract a readable message from a Tauri error payload or unknown error value. */
export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object") {
    const err = e as { message?: unknown; error?: unknown };
    if (typeof err.message === "string") return err.message;
    if (typeof err.error === "string") return err.error;
  }
  return String(e);
}
