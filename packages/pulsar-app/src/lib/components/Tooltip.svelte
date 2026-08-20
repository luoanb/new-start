<script lang="ts">
  import type { Snippet } from "svelte";

  // Tooltip：浮动提示，支持两种用法：
  // 1) 包装用法：<Tooltip label="提示" position="top"><button/></Tooltip>
  //    悬停 / 键盘聚焦子元素时，在其旁显示浮动提示。
  // 2) Popper 用法：<Tooltip target={el} content="提示" placement="right" />
  //    由外部控制目标元素（target 非 null 时显示），滚动/缩放自动重算位置。
  // 提示层 portal 到 body，规避 overflow / transform 祖先裁切。
  let {
    children,
    label,
    position = "top",
    target,
    content,
    placement = "right",
    offset = 8,
  }: {
    children?: Snippet;
    label?: string;
    position?: "top" | "bottom" | "left" | "right";
    target?: HTMLElement | null;
    content?: string;
    placement?: "top" | "bottom" | "left" | "right";
    offset?: number;
  } = $props();

  // 包装用法：由 hover / focus 驱动；Popper 用法：由外部 target 驱动
  const isWrap = $derived(label != null);

  let show = $state(false);
  let wrapEl = $state<HTMLElement>();

  const effTarget = $derived(isWrap ? (show ? wrapEl : null) : (target ?? null));
  const effContent = $derived(isWrap ? (label ?? "") : (content ?? ""));
  const effPlacement = $derived(isWrap ? position : placement);

  let pos = $state<{ top: number; left: number } | null>(null);

  function portal(node: Element) {
    document.body.appendChild(node);
    return () => {
      node.remove();
    };
  }

  function update() {
    const el = effTarget;
    if (!el) {
      pos = null;
      return;
    }
    const r = el.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const estW = Math.min(320, vw - 16);
    const estH = 28;
    let top = 0;
    let left = 0;
    if (effPlacement === "right") {
      left = r.right + offset;
      if (left + estW > vw - 8) left = Math.max(8, r.left - estW - offset);
      top = r.top;
    } else if (effPlacement === "left") {
      left = r.left - estW - offset;
      if (left < 8) left = r.right + offset;
      top = r.top;
    } else if (effPlacement === "bottom") {
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

{#if isWrap}
  <span
    class="tooltip-wrap"
    bind:this={wrapEl}
    onmouseenter={() => (show = true)}
    onmouseleave={() => (show = false)}
    onfocusin={() => (show = true)}
    onfocusout={() => (show = false)}
  >
    {@render children?.()}
  </span>
{/if}

{#if pos}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    {@attach portal}
    class="tooltip"
    role="presentation"
    style="top: {pos.top}px; left: {pos.left}px;"
  >{effContent}</div>
{/if}

<style>
  .tooltip-wrap { display: inline-flex; }
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
