<script lang="ts">
  import CopyButton from "./CopyButton.svelte";

  let { content }: { content: string } = $props();

  let expanded = $state(false);

  function toggle() {
    expanded = !expanded;
  }

  // 折叠条默认只显示开头摘要，展开看全文。
  let preview = $derived(
    content.length > 60 ? `${content.slice(0, 60)}…` : content
  );
</script>

<div class="nudge-block" class:expanded>
  <div class="block-header">
    <button class="summary" onclick={toggle}>
      <span class="toggle-icon">{expanded ? "▾" : "▸"}</span>
      <span class="preview">{preview}</span>
    </button>
    <!-- 轮询简报的复制：仅复制简报全文（content） -->
    <CopyButton text={content} />
  </div>

  {#if expanded}
    <div class="detail">
      <pre class="output">{content}</pre>
    </div>
  {/if}
</div>

<style>
  .nudge-block {
    margin-top: var(--space-2);
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--color-surface) 45%, var(--color-bg));
    border: var(--border-width) solid color-mix(in srgb, var(--color-border) 45%, transparent);
    border-left: 3px solid color-mix(in srgb, var(--color-text-muted) 55%, transparent);
    overflow: hidden;
  }
  .block-header { display: flex; align-items: center; gap: var(--space-1); padding-right: var(--space-1); }
  .summary {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
    padding: var(--space-2) var(--space-3);
    border: none;
    background: transparent;
    color: var(--color-text);
    font-size: var(--fs-sm);
    cursor: pointer;
    text-align: left;
    transition: background var(--duration-fast) var(--ease-out);
  }
  .summary:hover {
    background: var(--color-hover);
  }
  .toggle-icon {
    font-size: 11px;
    color: var(--color-text-muted);
    flex-shrink: 0;
  }
  .preview {
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    opacity: 0.7;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }
  .detail {
    border-top: var(--border-width) solid var(--color-border);
    padding: var(--space-2) var(--space-3);
    max-height: 400px;
    overflow-y: auto;
  }
  .output {
    margin: 0;
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    background: oklch(0.2 0.005 75);
    color: oklch(0.88 0.004 75);
    font-family: var(--font-mono, monospace);
    font-size: var(--fs-xs);
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>
