// 指针拖拽 hook —— rAF 节流，拖动结束触发 onEnd
//
// 关键点：
// - pointerdown 时 preventDefault：阻止原生文本选择 / 原生 drag 手势接管
//   （一旦浏览器进入 drag 状态，会吞掉后续 pointermove/pointerup，分割线失联）
// - move/up 同时挂到 window：配合 setPointerCapture 双保险，鼠标移出分割条仍持续跟踪
// - 拖动期间给 body 挂 resizing class：全局禁用 user-select，避免拖过文本区被选中

const BODY_RESIZING = "resizing";

export function useResizable(options: {
  axis: "x" | "y";
  onResize: (deltaPx: number) => void;
  onEnd?: () => void;
}) {
  let startX = 0;
  let startY = 0;
  let lastDelta = 0;
  let rafId = 0;
  let active = false;

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0) return; // 仅响应左键
    active = true;
    startX = e.clientX;
    startY = e.clientY;
    lastDelta = 0;
    e.preventDefault();
    (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
    window.addEventListener("pointercancel", onPointerUp);
    document.body.classList.add(BODY_RESIZING);
  }

  function onPointerMove(e: PointerEvent) {
    if (!active) return;
    const delta = options.axis === "x" ? e.clientX - startX : e.clientY - startY;
    cancelAnimationFrame(rafId);
    rafId = requestAnimationFrame(() => {
      options.onResize(delta - lastDelta);
      lastDelta = delta;
    });
  }

  function onPointerUp() {
    if (!active) return;
    active = false;
    cancelAnimationFrame(rafId);
    window.removeEventListener("pointermove", onPointerMove);
    window.removeEventListener("pointerup", onPointerUp);
    window.removeEventListener("pointercancel", onPointerUp);
    document.body.classList.remove(BODY_RESIZING);
    options.onEnd?.();
  }

  function attach(el: HTMLElement) {
    el.addEventListener("pointerdown", onPointerDown);
  }

  function detach(el: HTMLElement) {
    el.removeEventListener("pointerdown", onPointerDown);
  }

  return { attach, detach };
}
