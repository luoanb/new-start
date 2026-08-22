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
      <span class="label">
        <svg
          class="label-ico"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <polyline points="4 17 10 11 4 5" />
          <line x1="12" x2="20" y1="19" y2="19" />
        </svg>
        <span class="label-text">{summary}</span>
      </span>
      <span class="block-header-actions" onclick={(e) => e.stopPropagation()}>
        <CopyButton text={copyText} />
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
  /* 消息区卡片统一规范：surface 底 + 淡边框 + radius-sm，无 accent 竖条/动画。 */
  .toolcall-block { margin-top: var(--space-2); border-radius: var(--radius-sm); background: var(--color-surface); border: var(--border-width) solid var(--color-border); overflow: hidden; }
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
  .summary:hover { background: var(--color-hover); }
  .block-header-actions { flex-shrink: 0; display: inline-flex; align-items: center; }
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
  .label {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-mono, monospace);
    font-size: var(--fs-xs);
    color: var(--color-text);
  }
  .label-ico {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    color: var(--color-text-muted);
  }
  .label-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .detail { border-top: var(--border-width) solid var(--color-border); padding: var(--space-2); max-height: 400px; overflow-y: auto; }
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
