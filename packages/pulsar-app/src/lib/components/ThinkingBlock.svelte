<script lang="ts">
  import { t } from "$lib/i18n";
  import CopyButton from "./CopyButton.svelte";

  let { reasoning, streaming = false }: { reasoning: string; streaming?: boolean } = $props();

  let expanded = $state(false);

  function toggle() {
    expanded = !expanded;
  }

  // 流式期间自动展开（让用户实时看到思考增量）；结束后收起（避免抢占正文注意力）。
  $effect(() => {
    if (streaming) expanded = true;
  });
</script>

<div class="thinking-block" class:expanded>
  <div class="block-header">
    <button class="summary" onclick={toggle}>
      <span class="toggle-icon">{expanded ? "▾" : "▸"}</span>
      <span class="label">🧠 {t("thinking.title")}</span>
      {#if streaming}
        <span class="streaming-dot"></span>
      {/if}
    </button>
    <CopyButton text={reasoning} />
  </div>

  {#if expanded}
    <div class="detail">
      <pre>{reasoning}</pre>
    </div>
  {/if}
</div>

<style>
  .thinking-block {
    margin-top: var(--space-2);
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--color-surface) 45%, var(--color-bg));
    border: var(--border-width) solid color-mix(in srgb, var(--color-border) 45%, transparent);
    border-left: 3px solid color-mix(in srgb, var(--color-warning) 55%, transparent);
    overflow: hidden;
  }
  .block-header { display: flex; align-items: center; gap: var(--space-1); padding-right: var(--space-1); }
  .summary { display: flex; align-items: center; gap: 6px; flex: 1; min-width: 0; padding: var(--space-2) var(--space-3); border: none; background: transparent; color: var(--color-text); font-size: var(--fs-sm); cursor: pointer; text-align: left; transition: background var(--duration-fast) var(--ease-out); }
  .summary:hover { background: var(--color-hover); }
  .toggle-icon { font-size: var(--fs-xs); color: var(--color-text-muted); flex-shrink: 0; }
  .label { font-family: var(--font-mono, monospace); font-size: var(--fs-xs); color: var(--color-text-muted); }
  .streaming-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--color-primary); animation: thinking-pulse 1.2s ease-in-out infinite; }
  .detail { border-top: var(--border-width) solid var(--color-border); padding: var(--space-2) var(--space-3); max-height: 400px; overflow-y: auto; }
  .detail pre { margin: 0; font-family: var(--font-mono, monospace); font-size: var(--fs-xs); line-height: 1.5; color: var(--color-text-muted); white-space: pre-wrap; word-break: break-word; }
  @keyframes thinking-pulse { 0%, 100% { opacity: 0.35; } 50% { opacity: 1; } }
</style>
