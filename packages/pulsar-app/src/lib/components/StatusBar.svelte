<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { t } from "$lib/i18n";
  import { isTauriEnv } from "$lib/api";

  let {
    appName,
    sidebarVisible = true,
    infoVisible = true,
    panelVisible = true,
    // 小屏抽屉开关态：用于移动端布局图标的高亮（对齐大屏的 active 表现）
    drawerSidebar = false,
    drawerInfo = false,
    drawerPanel = false,
    // 小屏导航栏（ActivityBar）显隐态与切换回调：点击顶栏 logo 触发
    activityOpen = false,
    onToggleActivity,
    onToggleSidebar,
    onToggleInfo,
    onTogglePanel,
  }: {
    appName: string;
    sidebarVisible?: boolean;
    infoVisible?: boolean;
    panelVisible?: boolean;
    drawerSidebar?: boolean;
    drawerInfo?: boolean;
    drawerPanel?: boolean;
    activityOpen?: boolean;
    onToggleActivity?: () => void;
    onToggleSidebar?: () => void;
    onToggleInfo?: () => void;
    onTogglePanel?: () => void;
  } = $props();

  // ── 响应式：800px 以下为小屏（抽屉模式），用于布局按钮 active 状态切换 ──
  let isMobile = $state(false);
  $effect(() => {
    const mq = window.matchMedia("(max-width: 800px)");
    const update = () => (isMobile = mq.matches);
    update();
    mq.addEventListener("change", update);
    return () => mq.removeEventListener("change", update);
  });

  // 左栏开关按钮：小屏跟随抽屉开关态，大屏跟随栏位可见态
  let sidebarActive = $derived(isMobile ? drawerSidebar : sidebarVisible);

  // ── 自绘标题栏：窗口控制（decorations: false，标题栏由本组件承载）──
  // 仅 Tauri 环境可用；浏览器等环境不构造窗口句柄，窗口控制按钮整体隐藏。
  const appWindow = isTauriEnv ? getCurrentWindow() : null;
  let isMaximized = $state(false);

  onMount(() => {
    if (!appWindow) return;
    void appWindow.isMaximized().then((v) => (isMaximized = v));
    const unlisten = appWindow.onResized(() => {
      void appWindow.isMaximized().then((v) => (isMaximized = v));
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  });

  function minimize() {
    if (!appWindow) return;
    void appWindow.minimize();
  }
  function toggleMaximize() {
    if (!appWindow) return;
    void appWindow.toggleMaximize();
  }
  function closeWindow() {
    if (!appWindow) return;
    void appWindow.close();
  }

  // 窗口拖拽：弃用 data-tauri-drag-region（WebKitGTK 下与自定义 mousedown 处理冲突，易失效），
  // 改为在标题栏空白区 mousedown 时显式调用 startDragging（配合 allow-start-dragging 权限）。
  function onBarMouseDown(e: MouseEvent) {
    if (!appWindow) return;
    const target = e.target as HTMLElement;
    // 交互元素不参与窗口拖拽
    if (target.closest("button, a, input, select, [role='button']")) return;
    // 仅左键发起拖拽
    if (e.button !== 0) return;
    void appWindow.startDragging();
  }

  // 双击标题栏空白区域最大化/还原（Linux 上 Tauri 不自动处理 drag-region 双击）
  function onBarDblClick(e: MouseEvent) {
    if (!appWindow) return;
    const target = e.target as HTMLElement;
    if (target.closest("button, a, input, select, [role='button']")) return;
    void appWindow.toggleMaximize();
  }
</script>

<!-- 标题栏承担窗口拖拽/双击最大化，属于桌面 chrome 而非语义内容，跳过静态元素交互 a11y 检查 -->
<!-- svelte-ignore a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions -->
<header
  class="status-bar"
  onmousedown={onBarMouseDown}
  ondblclick={onBarDblClick}
>
  <div class="bar-left">
    <!-- 品牌标识：Spark Node（星点方徽），颜色跟随主题 CSS 变量；
         小屏下作为左侧导航栏开关：点击切换显隐（触屏可点，不依赖 hover） -->
    <button
      class="logo-btn"
      class:clickable={isMobile}
      class:active={activityOpen}
      onclick={isMobile ? onToggleActivity : undefined}
      tabindex={isMobile ? 0 : -1}
      title={isMobile ? t("statusBar.toggleNav") : undefined}
      aria-label={isMobile ? t("statusBar.toggleNav") : undefined}
      aria-expanded={isMobile ? activityOpen : undefined}
    >
      <svg class="app-logo" viewBox="0 0 48 48" aria-hidden="true">
        <rect x="2" y="2" width="44" height="44" rx="14" fill="var(--color-primary)" />
        <path d="M24 13 C25.2 19.4 28.6 22.8 35 24 C28.6 25.2 25.2 28.6 24 35 C22.8 28.6 19.4 25.2 13 24 C19.4 22.8 22.8 19.4 24 13 Z" fill="var(--color-on-primary)" />
        <circle cx="34" cy="34" r="3.2" fill="var(--color-on-primary)" fill-opacity="0.85" />
      </svg>
    </button>
    <span class="app-name">{appName}</span>

    <!-- 左栏开关按钮（大屏/小屏均显示）：小屏跟随抽屉态，大屏跟随栏位可见态 -->
    <button
      class="layout-btn"
      class:active={sidebarActive}
      onclick={onToggleSidebar}
      title={t("statusBar.toggleSidebar")}
      aria-label={t("statusBar.toggleSidebar")}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <rect class="frame" x="1.5" y="2.5" width="13" height="11" rx="1.5" />
        <rect class="fill" x="1.5" y="2.5" width="4.5" height="11" />
        <line class="frame" x1="6" y1="2.5" x2="6" y2="13.5" />
      </svg>
    </button>

    <button
      class="layout-btn mobile-only"
      class:active={drawerPanel}
      onclick={onTogglePanel}
      title={t("statusBar.togglePanel")}
      aria-label={t("statusBar.togglePanel")}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <rect class="frame" x="1.5" y="2.5" width="13" height="11" rx="1.5" />
        <rect class="fill" x="1.5" y="9.5" width="13" height="4" />
        <line class="frame" x1="1.5" y1="9.5" x2="14.5" y2="9.5" />
      </svg>
    </button>
  </div>

  <div class="bar-center"></div>

  <div class="bar-right">
    <button
      class="layout-btn desktop-only"
      class:active={infoVisible}
      onclick={onToggleInfo}
      title={t("statusBar.toggleInfo")}
      aria-label={t("statusBar.toggleInfo")}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <rect class="frame" x="1.5" y="2.5" width="13" height="11" rx="1.5" />
        <rect class="fill" x="10" y="2.5" width="4.5" height="11" />
        <line class="frame" x1="10" y1="2.5" x2="10" y2="13.5" />
      </svg>
    </button>

    <button
      class="layout-btn desktop-only"
      class:active={panelVisible}
      onclick={onTogglePanel}
      title={t("statusBar.togglePanel")}
      aria-label={t("statusBar.togglePanel")}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <rect class="frame" x="1.5" y="2.5" width="13" height="11" rx="1.5" />
        <rect class="fill" x="1.5" y="9.5" width="13" height="4" />
        <line class="frame" x1="1.5" y1="9.5" x2="14.5" y2="9.5" />
      </svg>
    </button>

    <button
      class="layout-btn mobile-only"
      class:active={drawerInfo}
      onclick={onToggleInfo}
      title={t("statusBar.toggleInfo")}
      aria-label={t("statusBar.toggleInfo")}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <rect class="frame" x="1.5" y="2.5" width="13" height="11" rx="1.5" />
        <rect class="fill" x="10" y="2.5" width="4.5" height="11" />
        <line class="frame" x1="10" y1="2.5" x2="10" y2="13.5" />
      </svg>
    </button>

    {#if appWindow}
      <span class="window-sep"></span>
      <div class="window-controls">
        <button class="win-btn" onclick={minimize} title="Minimize" aria-label="Minimize">
          <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true">
            <line x1="1" y1="6" x2="11" y2="6" />
          </svg>
        </button>
        <button
          class="win-btn"
          onclick={toggleMaximize}
          title={isMaximized ? "Restore" : "Maximize"}
          aria-label={isMaximized ? "Restore" : "Maximize"}
        >
          {#if isMaximized}
            <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true">
              <path d="M4.5 4.5 V3 A1.5 1.5 0 0 1 6 1.5 H9 A1.5 1.5 0 0 1 10.5 3 V6 A1.5 1.5 0 0 1 9 7.5 H7.5" />
              <rect x="1.5" y="4.5" width="6" height="6" rx="1.5" />
            </svg>
          {:else}
            <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true">
              <rect x="1.5" y="1.5" width="9" height="9" rx="1.5" />
            </svg>
          {/if}
        </button>
        <button class="win-btn win-btn-close" onclick={closeWindow} title="Close" aria-label="Close">
          <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true">
            <line x1="1.5" y1="1.5" x2="10.5" y2="10.5" />
            <line x1="10.5" y1="1.5" x2="1.5" y2="10.5" />
          </svg>
        </button>
      </div>
    {/if}
  </div>
</header>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 var(--space-4);
    height: 40px;
    background: var(--color-surface);
    border-bottom: var(--border-width) solid var(--color-border);
    font-size: var(--fs-sm);
    /* 自绘标题栏：整行禁选文本，避免选中内容干扰窗口拖拽 */
    user-select: none;
    -webkit-user-select: none;
  }

  .bar-left, .bar-center, .bar-right {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .app-logo {
    display: block;
    width: 20px;
    height: 20px;
    flex-shrink: 0;
  }

  /* logo 按钮：仅小屏可点击（切换左侧导航栏显隐），大屏不响应 */
  .logo-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    cursor: default;
  }
  .logo-btn.clickable {
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-out);
  }
  .logo-btn.clickable:hover { background: var(--color-hover); }
  .logo-btn.clickable.active { box-shadow: inset 0 0 0 1px var(--color-primary); }
  .logo-btn:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: 1px;
  }

  .bar-center { flex: 1; justify-content: center; }

  .app-name { font-weight: 600; font-size: var(--fs-base); }

  .layout-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-out),
                color var(--duration-fast) var(--ease-out);
  }

  .layout-btn svg {
    width: 16px;
    height: 16px;
    display: block;
  }

  /* 线框：描边不填充；填充块：表示该栏可见 */
  .layout-btn svg :global(.frame) {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.2;
    stroke-linecap: round;
  }

  .layout-btn svg :global(.fill) {
    fill: currentColor;
    stroke: none;
    opacity: 0;
    transition: opacity var(--duration-fast) var(--ease-out);
  }

  .layout-btn:hover { background: var(--color-hover); color: var(--color-text); }
  .layout-btn:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: 1px;
  }

  .layout-btn.active { color: var(--color-primary); }
  .layout-btn.active svg :global(.fill) { opacity: 0.9; }

  /* ── 窗口控制按钮（自绘标题栏）── */
  .window-sep {
    width: 1px;
    height: 20px;
    background: var(--color-border);
    margin: 0 var(--space-1);
  }

  .window-controls {
    display: flex;
    align-items: center;
    margin-left: var(--space-1);
  }

  .win-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 26px;
    padding: 0;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    line-height: 1;
    transition: background var(--duration-fast) var(--ease-out),
                color var(--duration-fast) var(--ease-out);
  }

  .win-btn svg {
    display: block;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .win-btn:hover { background: var(--color-hover); color: var(--color-text); }
  .win-btn-close:hover { background: var(--color-error); color: #fff; }
  .win-btn:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: 1px;
  }

  @media (max-width: 800px) {
    .mobile-only { display: flex; }
    .desktop-only { display: none; }
  }
  @media (min-width: 801px) {
    .mobile-only { display: none; }
  }
</style>
