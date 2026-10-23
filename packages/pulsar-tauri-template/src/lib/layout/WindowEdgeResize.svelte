<script lang="ts">
  // 无边框 Tauri 窗口（decorations: false）在 Linux/WebKitGTK 下系统不渲染边缘 resize 光标，
  // 本组件用四边/四角透明热区 + CSS cursor 补上提示；仅由 Tauri 环境上层挂载。
</script>

<div class="window-edge-resize" aria-hidden="true">
  <div class="edge edge-top"></div>
  <div class="edge edge-bottom"></div>
  <div class="edge edge-left"></div>
  <div class="edge edge-right"></div>
  <div class="corner corner-tl"></div>
  <div class="corner corner-tr"></div>
  <div class="corner corner-bl"></div>
  <div class="corner corner-br"></div>
</div>

<style>
  .window-edge-resize {
    position: fixed;
    inset: 0;
    z-index: 9999;
    /* 容器不拦截指针，仅各热区子元素命中 hover */
    pointer-events: none;
  }
  .window-edge-resize > div {
    position: fixed;
    pointer-events: auto;
  }
  /* 四边：4px，水平/垂直缩放光标（边在角处让出空间，避免光标重叠） */
  .edge-top,
  .edge-bottom {
    left: 10px;
    right: 10px;
    height: 4px;
    cursor: ns-resize;
  }
  .edge-top { top: 0; }
  .edge-bottom { bottom: 0; }
  .edge-left,
  .edge-right {
    top: 10px;
    bottom: 10px;
    width: 4px;
    cursor: ew-resize;
  }
  .edge-left { left: 0; }
  .edge-right { right: 0; }
  /* 四角：10px，对角缩放光标 */
  .corner { width: 10px; height: 10px; }
  .corner-tl { top: 0; left: 0; cursor: nwse-resize; }
  .corner-tr { top: 0; right: 0; cursor: nesw-resize; }
  .corner-bl { bottom: 0; left: 0; cursor: nesw-resize; }
  .corner-br { bottom: 0; right: 0; cursor: nwse-resize; }
</style>
