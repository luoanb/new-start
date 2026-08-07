<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import ThemeSwitcher from "./ThemeSwitcher.svelte";
  import LocaleSwitcher from "./LocaleSwitcher.svelte";
  import { t } from "$lib/i18n";

  let {
    appName,
    sessionId,
    mode,
    neuronActive = false,
    sidebarVisible = true,
    infoVisible = true,
    panelVisible = true,
    onToggleSidebar,
    onToggleInfo,
    onToggleNeuron,
    onTogglePanel,
  }: {
    appName: string;
    sessionId: string;
    mode: string;
    neuronActive?: boolean;
    sidebarVisible?: boolean;
    infoVisible?: boolean;
    panelVisible?: boolean;
    onToggleSidebar?: () => void;
    onToggleInfo?: () => void;
    onToggleNeuron?: () => void;
    onTogglePanel?: () => void;
  } = $props();

  function shortId(id: string): string {
    if (id.length <= 16) return id;
    return `${id.slice(0, 8)}..${id.slice(-4)}`;
  }

  const modeLabel: Record<string, string> = {
    chat: "Chat",
    agent: "Agent",
    assistant: "Assistant",
  };

  // ── 自绘标题栏：窗口控制（decorations: false，标题栏由本组件承载）──
  const appWindow = getCurrentWindow();
  let isMaximized = $state(false);

  onMount(() => {
    void appWindow.isMaximized().then((v) => (isMaximized = v));
    const unlisten = appWindow.onResized(() => {
      void appWindow.isMaximized().then((v) => (isMaximized = v));
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  });

  function minimize() {
    void appWindow.minimize();
  }
  function toggleMaximize() {
    void appWindow.toggleMaximize();
  }
  function closeWindow() {
    void appWindow.close();
  }

  // 窗口拖拽：弃用 data-tauri-drag-region（WebKitGTK 下与自定义 mousedown 处理冲突，易失效），
  // 改为在标题栏空白区 mousedown 时显式调用 startDragging（配合 allow-start-dragging 权限）。
  function onBarMouseDown(e: MouseEvent) {
    const target = e.target as HTMLElement;
    // 交互元素不参与窗口拖拽
    if (target.closest("button, a, input, select, [role='button']")) return;
    // 仅左键发起拖拽
    if (e.button !== 0) return;
    void appWindow.startDragging();
  }

  // 双击标题栏空白区域最大化/还原（Linux 上 Tauri 不自动处理 drag-region 双击）
  function onBarDblClick(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (target.closest("button, a, input, select, [role='button']")) return;
    void appWindow.toggleMaximize();
  }
</script>

<header
  class="status-bar"
  onmousedown={onBarMouseDown}
  ondblclick={onBarDblClick}
>
  <div class="bar-left">
    <button class="drawer-btn mobile-only" onclick={onToggleSidebar} title={t("drawer.sessions")}>
      ☰
    </button>
    <span class="app-name">{appName}</span>
  </div>

  <div class="bar-center">
    {#if sessionId}
      <span class="session-info desktop-only">
        {t("common.session")}: <strong>{shortId(sessionId)}</strong>
      </span>
      <span class="mode-tag">{modeLabel[mode] ?? mode}</span>
    {/if}
  </div>

  <div class="bar-right">
    <button
      class="neuron-btn"
      class:active={neuronActive}
      onclick={onToggleNeuron}
      title="Neuron Manager"
    >
      <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="5" cy="6" r="2" />
        <circle cx="19" cy="7" r="2" />
        <circle cx="12" cy="18" r="2" />
        <line x1="6.5" y1="7" x2="11" y2="16" />
        <line x1="17.5" y1="8" x2="13" y2="16" />
      </svg>
    </button>

    <span class="layout-sep"></span>

    <button
      class="layout-btn desktop-only"
      class:active={sidebarVisible}
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

    <button class="drawer-btn mobile-only" onclick={onToggleInfo} title={t("drawer.info")}>
      ⓘ
    </button>

    <LocaleSwitcher />
    <ThemeSwitcher />

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

  .bar-center { flex: 1; justify-content: center; }

  .app-name { font-weight: 600; font-size: var(--fs-base); }
  .session-info { color: var(--color-text-muted); }

  .mode-tag {
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    padding: 2px var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--color-primary);
    color: var(--color-on-primary);
    letter-spacing: 0.03em;
  }

  .drawer-btn {
    display: none;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text);
    cursor: pointer;
    font-size: 16px;
    line-height: 1;
    transition: background var(--duration-fast) var(--ease-out);
  }

  .neuron-btn { display: inline-flex; align-items: center; padding: 0 var(--space-1); background: none; border: none; cursor: pointer; line-height: 1; opacity: 0.5; transition: opacity var(--duration-fast) var(--ease-out); }
  .neuron-btn.active { opacity: 1; }
  .neuron-btn:hover { opacity: 0.8; }

  .layout-sep {
    width: 1px;
    height: 20px;
    background: var(--color-border);
    margin: 0 var(--space-1);
  }

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

  .drawer-btn:hover { background: var(--color-hover); }

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
