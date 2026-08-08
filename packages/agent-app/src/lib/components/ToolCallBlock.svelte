<script lang="ts">
  import type { ToolCall } from "$lib/types";
  import { t } from "$lib/i18n";

  let { toolCalls }: { toolCalls: ToolCall[] } = $props();

  let expanded = $state(false);

  function toggle() {
    expanded = !expanded;
  }

  let summary = $derived(
    toolCalls.map((tc) => tc.name).join(", ")
  );
</script>

<div class="toolcall-block" class:expanded>
  <button class="summary" onclick={toggle}>
    <span class="toggle-icon">{expanded ? "▾" : "▸"}</span>
    <span class="label">🛠 {summary}</span>
  </button>

  {#if expanded}
    <div class="detail">
      {#each toolCalls as tc, i}
        <div class="call-item">
          <div class="call-header">
            <span class="call-name">{tc.name}</span>
            <span class="call-id">{tc.id}</span>
          </div>
          <div class="call-section">
            <span class="section-label">{t("toolCall.arguments")}</span>
            <pre class="json">{JSON.stringify(tc.arguments, null, 2)}</pre>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .toolcall-block { margin-top: var(--space-2); border-radius: var(--radius-md); background: color-mix(in srgb, var(--color-surface) 45%, var(--color-bg)); border: var(--border-width) solid color-mix(in srgb, var(--color-border) 45%, transparent); border-left: 3px solid color-mix(in srgb, var(--color-primary) 55%, transparent); overflow: hidden; }
  .summary { display: flex; align-items: center; gap: 6px; width: 100%; padding: var(--space-2) var(--space-3); border: none; background: transparent; color: var(--color-text); font-size: var(--fs-sm); cursor: pointer; text-align: left; transition: background var(--duration-fast) var(--ease-out); }
  .summary:hover { background: var(--color-hover); }
  .toggle-icon { font-size: 11px; color: var(--color-text-muted); flex-shrink: 0; }
  .label { font-family: var(--font-mono, monospace); font-size: var(--fs-xs); color: var(--color-text-muted); }
  .detail { border-top: var(--border-width) solid var(--color-border); padding: var(--space-2) var(--space-3); max-height: 400px; overflow-y: auto; }
  .call-item { margin-bottom: var(--space-2); }
  .call-item:last-child { margin-bottom: 0; }
  .call-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: var(--space-1); }
  .call-name { font-weight: 600; font-size: var(--fs-sm); color: var(--color-primary); }
  .call-id { font-size: var(--fs-xs); font-family: var(--font-mono, monospace); color: var(--color-text-muted); }
  .call-section { margin-bottom: var(--space-1); }
  .call-section:last-child { margin-bottom: 0; }
  .section-label { font-size: var(--fs-xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; color: var(--color-text-muted); display: block; margin-bottom: var(--space-1); }
  .json { margin: 0; padding: var(--space-2) var(--space-2); border-radius: var(--radius-sm); background: oklch(0.20 0.005 75); color: oklch(0.88 0.004 75); font-family: var(--font-mono, monospace); font-size: var(--fs-xs); line-height: 1.4; overflow-x: auto; white-space: pre; }
</style>
