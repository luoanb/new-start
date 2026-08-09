<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label = "",
    position = "bottom",
    children,
  }: {
    label?: string;
    position?: "top" | "bottom" | "left" | "right";
    children?: Snippet;
  } = $props();

  // JS 驱动显示：纯 CSS :hover 在点击触发重新渲染（按钮变 disabled/spinning）后
  // 会残留 hover 状态导致 tooltip 不消失，因此改用事件管理 + 点击即隐藏。
  let visible = $state(false);

  function show() {
    visible = true;
  }
  function hide() {
    visible = false;
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<span class="tooltip" role="presentation" onmouseenter={show} onmouseleave={hide} onfocusin={show} onfocusout={hide} onclick={hide}>
  {@render children?.()}
  {#if label}
    <span class="tooltip-tip {position}" class:show={visible} role="tooltip">{label}</span>
  {/if}
</span>

<style>
  .tooltip {
    position: relative;
    display: inline-flex;
  }

  .tooltip-tip {
    position: absolute;
    z-index: 1100;
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    background: var(--color-text);
    color: var(--color-bg);
    font-size: var(--fs-xs);
    line-height: 1.4;
    white-space: nowrap;
    opacity: 0;
    pointer-events: none;
    transform: scale(0.95);
    transition: opacity var(--duration-fast) var(--ease-out),
                transform var(--duration-fast) var(--ease-out);
  }

  .tooltip-tip.show {
    opacity: 1;
  }

  .tooltip-tip.bottom {
    top: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%) scale(0.95);
  }
  .tooltip-tip.bottom.show {
    transform: translateX(-50%) scale(1);
  }

  .tooltip-tip.top {
    bottom: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%) scale(0.95);
  }
  .tooltip-tip.top.show {
    transform: translateX(-50%) scale(1);
  }
</style>
