<script lang="ts">
  import { useResizable } from "./useResizable.svelte";
  import type { ActionReturn } from "svelte/action";

  let {
    orientation = "vertical",
    onResize,
    onResizeEnd,
    extraClass,
  }: {
    orientation?: "vertical" | "horizontal";
    onResize: (deltaPx: number) => void;
    onResizeEnd?: () => void;
    extraClass?: string;
  } = $props();

  const resizable = useResizable({
    axis: orientation === "vertical" ? "x" : "y",
    onResize,
    onEnd: onResizeEnd,
  });

  // svelte:action —— attach/detach 随元素生命周期
  function splitterAction(node: HTMLElement): ActionReturn {
    resizable.attach(node);
    return { destroy: () => resizable.detach(node) };
  }
</script>

<div
  class="splitter {orientation === 'horizontal' ? 'horizontal' : 'vertical'} {extraClass}"
  use:splitterAction
  role="separator"
  aria-orientation={orientation}
  aria-label="Resize"
></div>

<style>
  :global(body.resizing) {
    user-select: none;
    -webkit-user-select: none;
    cursor: grabbing;
  }

  .splitter {
    position: relative;
    z-index: 2;
    flex-shrink: 0;
    touch-action: none;
    cursor: col-resize;
    user-select: none;
    -webkit-user-select: none;
    transition: background var(--duration-fast) var(--ease-out);
  }

  .splitter.vertical { width: 4px; margin: 0 -2px; cursor: col-resize; }
  .splitter.horizontal { height: 4px; margin: -2px 0; cursor: row-resize; }

  .splitter::after {
    content: "";
    position: absolute;
    inset: 0;
    background: var(--color-border);
    opacity: 0.9;
  }

  .splitter.vertical::after { left: 1px; right: 1px; }
  .splitter.horizontal::after { top: 1px; bottom: 1px; }

  .splitter:hover::after,
  .splitter:active::after {
    background: var(--color-primary);
    opacity: 0.6;
  }
</style>
