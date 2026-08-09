<script lang="ts">
  import { t } from "$lib/i18n";

  let { text }: { text: string } = $props();

  let copied = $state(false);

  async function handleCopy(event: MouseEvent) {
    event.stopPropagation();
    await navigator.clipboard.writeText(text);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

<button
  class="copy-btn"
  class:copied
  onclick={handleCopy}
  title={copied ? t("chatMessage.copied") : t("chatMessage.copy")}
  aria-label={copied ? t("chatMessage.copied") : t("chatMessage.copy")}
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
    width: 26px;
    height: 26px;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    border-radius: var(--radius-sm);
    flex-shrink: 0;
    opacity: 0;
    transition:
      opacity var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out),
      background var(--duration-fast) var(--ease-out);
  }
  /* 悬停折叠块（或键盘聚焦）时显示复制按钮；复制成功后常显 */
  :global(.block-header:hover) .copy-btn,
  :global(.block-header:focus-within) .copy-btn,
  .copy-btn.copied {
    opacity: 1;
  }
  .copy-btn:hover {
    color: var(--color-text);
    background: var(--color-hover);
  }
  .copy-btn.copied {
    color: var(--color-primary);
  }
</style>
