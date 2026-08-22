<script lang="ts">
  import { t } from "$lib/i18n";
  let { message, onDismiss }: { message: string; onDismiss: () => void } = $props();

  let copied = $state(false);
  let copyFailed = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | undefined;

  // 点击错误文案复制到剪贴板：优先 Clipboard API，非安全上下文/WebKitGTK 下回退 execCommand。
  async function copyMessage() {
    if (!message) return;
    let ok = false;
    try {
      await navigator.clipboard.writeText(message);
      ok = true;
    } catch {
      try {
        const ta = document.createElement("textarea");
        ta.value = message;
        ta.setAttribute("readonly", "");
        ta.style.position = "fixed";
        ta.style.opacity = "0";
        document.body.appendChild(ta);
        ta.select();
        ok = document.execCommand("copy");
        document.body.removeChild(ta);
      } catch {
        ok = false;
      }
    }
    clearTimeout(copyTimer);
    if (ok) {
      copied = true;
      copyFailed = false;
      copyTimer = setTimeout(() => (copied = false), 1500);
    } else {
      copyFailed = true;
      copyTimer = setTimeout(() => (copyFailed = false), 1500);
    }
  }

  // Auto-dismiss after 5 seconds
  $effect(() => {
    if (!message) return;
    const timer = setTimeout(onDismiss, 5000);
    return () => clearTimeout(timer);
  });
</script>

{#if message}
  <div class="error-banner">
    <span
      class="error-text"
      role="button"
      tabindex="0"
      onclick={copyMessage}
      onkeydown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          copyMessage();
        }
      }}
      title={t("common.clickToCopy")}
    >{copied ? t("common.copied") : copyFailed ? t("common.copyFailed") : message}</span>
    <button class="dismiss-btn" onclick={onDismiss}>×</button>
  </div>
{/if}

<style>
  .error-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 16px;
    background: var(--color-error-bg);
    color: var(--color-error);
    font-size: var(--fs-sm);
    border-bottom: 1px solid var(--color-error-border);
  }

  .error-text {
    flex: 1;
    cursor: pointer;
    border-radius: 4px;
    padding: 2px 4px;
    margin: -2px -4px;
    transition: background 0.15s ease;
  }

  .error-text:hover {
    background: color-mix(in srgb, var(--color-error) 12%, transparent);
  }

  .dismiss-btn {
    background: none;
    border: none;
    font-size: 22px;
    cursor: pointer;
    color: inherit;
    padding: 0 4px;
    line-height: 1;
    opacity: 0.6;
  }

  .dismiss-btn:hover {
    opacity: 1;
  }
</style>
