<script lang="ts">
  let {
    label = "",
    position = "bottom",
  }: { label?: string; position?: "top" | "bottom" | "left" | "right" } = $props();
</script>

<span class="tooltip {position}">
  <slot />
  {#if label}
    <span class="tooltip-tip" role="tooltip">{label}</span>
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

  .tooltip:hover .tooltip-tip,
  .tooltip:focus-within .tooltip-tip {
    opacity: 1;
  }

  .tooltip.bottom .tooltip-tip {
    top: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%) scale(0.95);
  }
  .tooltip.bottom:hover .tooltip-tip,
  .tooltip.bottom:focus-within .tooltip-tip {
    transform: translateX(-50%) scale(1);
  }

  .tooltip.top .tooltip-tip {
    bottom: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%) scale(0.95);
  }
  .tooltip.top:hover .tooltip-tip,
  .tooltip.top:focus-within .tooltip-tip {
    transform: translateX(-50%) scale(1);
  }
</style>
