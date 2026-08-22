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
      <span class="label">🧠 {t("thinking.title")}</span>
      {#if streaming}
        <span class="streaming-dot"></span>
      {/if}
      <span class="block-header-actions" onclick={(e) => e.stopPropagation()}>
        <CopyButton text={reasoning} />
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
      <pre>{reasoning}</pre>
    </div>
  {/if}
</div>

<style>
  /* 消息区卡片统一规范：surface 底 + 淡边框 + radius-sm，无 accent 竖条/动画。 */
  .thinking-block {
    margin-top: var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    border: var(--border-width) solid var(--color-border);
    overflow: hidden;
  }
  .block-header { display: flex; }
  .summary { display: flex; align-items: center; gap: 6px; flex: 1; min-width: 0; width: 100%; padding: var(--space-1) var(--space-2); border: none; background: transparent; color: var(--color-text); font-size: var(--fs-xs); cursor: pointer; text-align: left; border-radius: var(--radius-sm); transition: background var(--duration-fast) var(--ease-out); }
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
  .label { font-family: var(--font-mono, monospace); font-size: var(--fs-xs); color: var(--color-text-muted); }
  .streaming-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--color-primary); animation: thinking-pulse 1.2s ease-in-out infinite; }
  .detail { border-top: var(--border-width) solid var(--color-border); padding: var(--space-2); max-height: 400px; overflow-y: auto; }
  .detail pre { margin: 0; font-family: var(--font-mono, monospace); font-size: var(--fs-xs); line-height: 1.5; color: var(--color-text); white-space: pre-wrap; word-break: break-word; }
  @keyframes thinking-pulse { 0%, 100% { opacity: 0.35; } 50% { opacity: 1; } }
</style>
