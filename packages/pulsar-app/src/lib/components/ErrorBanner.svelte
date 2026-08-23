<script lang="ts">
  import { t } from "$lib/i18n";
  import { CopyToClipboard } from "$lib/utils";
  let { message, onDismiss }: { message: string; onDismiss: () => void } = $props();

  let copied = $state(false);
  let copyFailed = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | undefined;

  // 点击错误文案复制到剪贴板：统一走公共复制方法（Clipboard API + execCommand 兜底）。
  async function copyMessage() {
    if (!message) return;
    const ok = await CopyToClipboard.copyText(message);
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
