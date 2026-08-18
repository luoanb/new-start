<script lang="ts">
  // Tooltip：跟随目标元素的浮动提示（Popper 语义）。
  // - target 非 null 时显示，自动定位到目标旁（默认右侧，视口右缘不足翻到左侧）
  // - portal 到 body，规避 overflow / transform 祖先裁切；滚动（含子元素滚动）与缩放时重算位置
  let {
    content,
    target,
    placement = "right",
    offset = 8,
  }: {
    content: string;
    target: HTMLElement | null;
    placement?: "right" | "left" | "top" | "bottom";
    offset?: number;
  } = $props();

  let pos = $state<{ top: number; left: number } | null>(null);

  function portal(node: Element) {
    document.body.appendChild(node);
    return () => {
      node.remove();
    };
  }

  function update() {
    if (!target) {
      pos = null;
      return;
    }
    const r = target.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const estW = Math.min(320, vw - 16);
    const estH = 28;
    let top = 0;
    let left = 0;
    if (placement === "right") {
      left = r.right + offset;
      if (left + estW > vw - 8) left = Math.max(8, r.left - estW - offset);
      top = r.top;
    } else if (placement === "left") {
      left = r.left - estW - offset;
      if (left < 8) left = r.right + offset;
      top = r.top;
    } else if (placement === "bottom") {
      left = r.left;
      top = r.bottom + offset;
    } else {
      // top
      left = r.left;
      top = r.top - offset;
    }
    left = Math.max(8, Math.min(left, vw - estW - 8));
    top = Math.max(8, Math.min(top, vh - estH - 8));
    pos = { top, left };
  }

  $effect(() => {
    update();
    // capture: true 捕获任意子元素滚动（如下拉列表滚动）并重算位置
    window.addEventListener("scroll", update, true);
    window.addEventListener("resize", update);
    return () => {
      window.removeEventListener("scroll", update, true);
      window.removeEventListener("resize", update);
    };
  });
</script>

{#if pos}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    {@attach portal}
    class="tooltip"
    role="presentation"
    style="top: {pos.top}px; left: {pos.left}px;"
  >{content}</div>
{/if}

<style>
  .tooltip {
    position: fixed;
    /* 覆盖 app.html 全局 `html > body > div { inset: 0 }`：tooltip portal 到 body 后是
       body 直系 div，会被拉满视口。恢复 inset:auto 让宽高内容自适应 */
    inset: auto;
    z-index: 902;
    max-width: min(480px, calc(100vw - 16px));
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    background: var(--color-elevated);
    border: 1px solid var(--color-border);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.14);
    font-size: var(--fs-xs);
    color: var(--color-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    pointer-events: none;
  }
</style>
