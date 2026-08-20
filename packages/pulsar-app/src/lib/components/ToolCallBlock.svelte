<script lang="ts">
  import type { ToolCall } from "$lib/types";
  import { t } from "$lib/i18n";
  import CopyButton from "./CopyButton.svelte";

  let { toolCalls }: { toolCalls: ToolCall[] } = $props();

  let expanded = $state(false);

  function toggle() {
    expanded = !expanded;
  }

  let summary = $derived(toolCalls.map((tc) => tc.name).join(", "));

  // 复制内容 = 完整的工具调用信息（工具名 + 参数），不含思考文本。
  let copyText = $derived(JSON.stringify(toolCalls, null, 2));
</script>

<div class="toolcall-block" class:expanded>
  <div class="block-header">
    <button class="summary" onclick={toggle}>
      <span class="label">🛠 {summary}</span>
      <span class="toggle-icon" aria-hidden="true">></span>
    </button>
    <CopyButton text={copyText} />
  </div>

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
  .toolcall-block { margin-top: var(--space-2); }
  .block-header { display: inline-flex; align-items: center; gap: 2px; }
  .summary {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    max-width: 100%;
    min-width: 0;
    padding: var(--space-1) var(--space-1);
    border: none;
    background: transparent;
    color: var(--color-text);
    font-size: var(--fs-sm);
    cursor: pointer;
    text-align: left;
    border-radius: var(--radius-sm);
    transition: background var(--duration-fast) var(--ease-out);
  }
  .summary:hover { background: var(--color-hover); }
  .toggle-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    font-size: 12px;
    font-weight: 700;
    line-height: 1;
    color: var(--color-text-muted);
    flex-shrink: 0;
    transition: transform var(--duration-fast) var(--ease-out);
    transform-origin: center;
  }
  .expanded .toggle-icon { transform: rotate(90deg); }
  .label {
    font-family: var(--font-mono, monospace);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .detail { padding: var(--space-1) var(--space-3) var(--space-2); max-height: 400px; overflow-y: auto; }
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
