<script lang="ts">
  import { t } from "$lib/i18n";
  import { CopyToClipboard } from "$lib/utils";

  let { text }: { text: string } = $props();

  let copied = $state(false);
  let copyFailed = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | undefined;

  async function handleCopy(event: MouseEvent) {
    event.stopPropagation();
    const ok = await CopyToClipboard.copyText(text);
    clearTimeout(copyTimer);
    copied = ok;
    copyFailed = !ok;
    copyTimer = setTimeout(() => {
      copied = false;
      copyFailed = false;
    }, 1500);
  }
</script>

<button
  class="copy-btn"
  class:copied
  class:failed={copyFailed}
  onclick={handleCopy}
  title={copied ? t("chatMessage.copied") : copyFailed ? t("common.copyFailed") : t("chatMessage.copy")}
  aria-label={copied ? t("chatMessage.copied") : copyFailed ? t("common.copyFailed") : t("chatMessage.copy")}
>
  {#if copied}
    <svg
      viewBox="0 0 24 24"
      width="14"
      height="14"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <polyline points="20 6 9 17 4 12" />
    </svg>
  {:else}
    <svg
      viewBox="0 0 24 24"
      width="14"
      height="14"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <rect width="14" height="14" x="8" y="8" rx="2" ry="2" />
      <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
    </svg>
  {/if}
</button>

<style>
  .copy-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    border-radius: var(--radius-sm);
    flex-shrink: 0;
    transition:
      opacity var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out),
      background var(--duration-fast) var(--ease-out);
  }
  /* 复制成功后常显（所有设备） */
  .copy-btn.copied {
    opacity: 1;
    visibility: visible;
  }
  /* 复制失败显示警示色 */
  .copy-btn.failed {
    opacity: 1;
    visibility: visible;
    color: var(--color-error, #e5484d);
  }
  /* hover-reveal 仅适用于折叠块头部（.block-header 内）场景；
     独立场景（如 ChatMessage 错误卡片）按钮始终可见可点击。
     见 .cursor/rules/ui-hover-reveal.mdc */
  @media (hover: hover) {
    :global(.block-header) .copy-btn {
      opacity: 0;
      visibility: hidden;
    }
    :global(.block-header:hover) .copy-btn,
    :global(.block-header:focus-within) .copy-btn {
      opacity: 1;
      visibility: visible;
    }
  }
  .copy-btn:hover {
    color: var(--color-text);
    background: var(--color-hover);
  }
  .copy-btn.copied {
    color: var(--color-primary);
  }
</style>
