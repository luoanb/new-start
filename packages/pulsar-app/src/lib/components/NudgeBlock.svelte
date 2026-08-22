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
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="summary"
      role="button"
      tabindex="0"
      onclick={toggle}
      onkeydown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          toggle();
        }
      }}
    >
      <span class="preview">{preview}</span>
      <span class="block-header-actions" onclick={(e) => e.stopPropagation()}>
        <!-- 轮询简报的复制：仅复制简报全文（content） -->
        <CopyButton text={content} />
      </span>
      <span class="toggle-icon" aria-hidden="true">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="m9 18 6-6-6-6" />
        </svg>
      </span>
    </div>
  </div>

  {#if expanded}
    <div class="detail">
      <pre class="output">{content}</pre>
    </div>
  {/if}
</div>

<style>
  /* 消息区卡片统一规范：surface 底 + 淡边框 + radius-sm，无 accent 竖条/动画。 */
  .nudge-block {
    margin-top: var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    border: var(--border-width) solid var(--color-border);
    overflow: hidden;
  }
  .block-header { display: flex; }
  .summary {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
    width: 100%;
    padding: var(--space-1) var(--space-2);
    border: none;
    background: transparent;
    color: var(--color-text);
    font-size: var(--fs-xs);
    cursor: pointer;
    text-align: left;
    border-radius: var(--radius-sm);
    transition: background var(--duration-fast) var(--ease-out);
  }
  .summary:hover {
    background: var(--color-hover);
  }
  .block-header-actions { flex-shrink: 0; display: inline-flex; align-items: center; }
  .preview {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-text);
  }
  .toggle-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    color: var(--color-text-muted);
    flex-shrink: 0;
    transition: transform var(--duration-fast) var(--ease-out);
    transform-origin: center;
  }
  .toggle-icon svg {
    width: 12px;
    height: 12px;
    display: block;
  }
  .expanded .toggle-icon { transform: rotate(90deg); }
  .detail {
    border-top: var(--border-width) solid var(--color-border);
    padding: var(--space-2);
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
