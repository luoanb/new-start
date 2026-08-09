<script lang="ts">
  let { message, onDismiss }: { message: string; onDismiss: () => void } = $props();

  // Auto-dismiss after 5 seconds
  $effect(() => {
    if (!message) return;
    const timer = setTimeout(onDismiss, 5000);
    return () => clearTimeout(timer);
  });
</script>

{#if message}
  <div class="error-banner">
    <span class="error-text">{message}</span>
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
    font-size: 13px;
    border-bottom: 1px solid var(--color-error-border);
  }

  .error-text {
    flex: 1;
  }

  .dismiss-btn {
    background: none;
    border: none;
    font-size: 18px;
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
