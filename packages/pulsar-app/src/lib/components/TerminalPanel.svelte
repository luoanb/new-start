<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import { isTauriEnv, readConnConfig, DEFAULT_REMOTE_URL } from "$lib/api";
  import { t } from "$lib/i18n";
  import {
    ipcTransport,
    wsTransport,
    type TerminalConnStatus,
    type TerminalTransport,
  } from "$lib/terminal/transport";

  /**
   * 浏览器模式直连 Tauri 进程内嵌的 WS 公共通道（见 spec terminal-browser-ws）。
   * 地址从远程连接配置（pulsar:remoteUrl）自动推导：http(s)://host:port → ws(s)://host:port/api/ws，
   * 配置了 token 时追加 ?token=；未配置时回落默认地址。WS 与 HTTP RPC 同端口同监听。
   */
  function deriveWsUrl(): string {
    const cfg = readConnConfig();
    const base = cfg.url || DEFAULT_REMOTE_URL;
    const scheme = base.replace(/^http/, "ws"); // http→ws / https→wss
    const query = cfg.token ? `?token=${encodeURIComponent(cfg.token)}` : "";
    return `${scheme}/api/ws${query}`;
  }
  const WS_URL = typeof window !== "undefined" ? deriveWsUrl() : "";

  type TerminalTab = {
    sessionId: string;
    title: string;
    exited: boolean;
  };

  // VS Code 风格深色主题（跟随应用主题；light/dark 双套，动态切换）。
  const DARK_TERMINAL_THEME = {
    background: "#1e1e1e",
    foreground: "#d4d4d4",
    cursor: "#aeafad",
    cursorAccent: "#1e1e1e",
    selectionBackground: "#264f78",
    black: "#000000",
    red: "#cd3131",
    green: "#0dbc79",
    yellow: "#e5e510",
    blue: "#2472c8",
    magenta: "#bc3fbc",
    cyan: "#11a8cd",
    white: "#e5e5e5",
    brightBlack: "#666666",
    brightRed: "#f14c4c",
    brightGreen: "#23d18b",
    brightYellow: "#f5f543",
    brightBlue: "#3b8eea",
    brightMagenta: "#d670d6",
    brightCyan: "#29b8db",
    brightWhite: "#e5e5e5",
  };
  // VS Code 风格浅色主题：背景白、前景深灰，配合浅色应用主题。
  const LIGHT_TERMINAL_THEME = {
    background: "#ffffff",
    foreground: "#333333",
    cursor: "#333333",
    cursorAccent: "#ffffff",
    selectionBackground: "#add6ff",
    black: "#000000",
    red: "#cd3131",
    green: "#107c10",
    yellow: "#795e26",
    blue: "#0451a5",
    magenta: "#bc3fbc",
    cyan: "#0598bc",
    white: "#808080",
    brightBlack: "#666666",
    brightRed: "#f14c4c",
    brightGreen: "#107c10",
    brightYellow: "#795e26",
    brightBlue: "#0451a5",
    brightMagenta: "#bc3fbc",
    brightCyan: "#0598bc",
    brightWhite: "#a5a5a5",
  };

  /** 应用当前主题：html[data-theme] 显式设置优先，否则跟随系统 prefers-color-scheme。 */
  function resolveAppTheme(): "dark" | "light" {
    const explicit = document.documentElement.dataset.theme;
    if (explicit === "light" || explicit === "dark") return explicit;
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  function terminalTheme(name: "dark" | "light") {
    return name === "light" ? LIGHT_TERMINAL_THEME : DARK_TERMINAL_THEME;
  }

  let themeName = $state<"dark" | "light">("dark");

  let tabs = $state<TerminalTab[]>([]);
  let activeId = $state<string | null>(null);
  let errorMsg = $state("");
  /** 传输连接状态（IPC 恒为 connected；浏览器 WS 连接中断时显示重连提示）。 */
  let connStatus = $state<TerminalConnStatus>("connecting");

  let transport: TerminalTransport;
  const terms = new Map<string, Terminal>();
  const observers = new Map<string, ResizeObserver>();
  let unsubscribes: (() => void)[] = [];

  onMount(() => {
    themeName = resolveAppTheme();
    // 监听应用主题变化（ThemeSwitcher 写入 html[data-theme] / 启动 applyThemeOnBoot）
    const themeObserver = new MutationObserver(() => (themeName = resolveAppTheme()));
    themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });
    // system 模式下跟随 OS 深浅色切换
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onMqChange = () => (themeName = resolveAppTheme());
    mq.addEventListener("change", onMqChange);

    transport = isTauriEnv ? ipcTransport() : wsTransport(WS_URL);
    unsubscribes = [
      transport.onStatusChange((s) => (connStatus = s)),
      transport.onOutput(routeOutput),
      transport.onExit(handleExit),
    ];
    connStatus = transport.status();
    transport
      .list()
      .then((sessions) => {
        tabs = sessions.map((s) => ({
          sessionId: s.sessionId,
          // 交互 shell 显示 shell 名；agent 可见执行会话后端以命令文本作 shell 字段，
          // 直接复用为 tab 标题（超长文本由 CSS ellipsis 收敛）。
          title: s.shell || s.sessionId,
          exited: s.exitCode != null,
        }));
        activeId = tabs.at(-1)?.sessionId ?? null;
      })
      .catch((e) => {
        errorMsg = t("terminal.initFailed", { error: `${e}` });
      });

    return () => {
      themeObserver.disconnect();
      mq.removeEventListener("change", onMqChange);
    };
  });

  // 主题变化时同步所有存活终端实例的配色（xterm 支持动态替换 options.theme）。
  $effect(() => {
    const theme = terminalTheme(themeName);
    for (const term of terms.values()) {
      term.options.theme = theme;
    }
  });

  onDestroy(() => {
    for (const fn of unsubscribes) fn();
    unsubscribes = [];
    transport?.dispose?.();
  });

  function routeOutput(sessionId: string, data: Uint8Array) {
    terms.get(sessionId)?.write(data);
  }

  function handleExit(sessionId: string, exitCode: number) {
    terms
      .get(sessionId)
      ?.write(`\r\n\x1b[90m[${t("terminal.exitCode", { code: exitCode })}]\x1b[0m\r\n`);
    tabs = tabs.map((t) => (t.sessionId === sessionId ? { ...t, exited: true } : t));
  }

  async function newTab() {
    errorMsg = "";
    try {
      const sessionId = await transport.spawn();
      tabs = [...tabs, { sessionId, title: sessionId, exited: false }];
      activeId = sessionId;
    } catch (e) {
      errorMsg = t("terminal.spawnFailed", { error: `${e}` });
    }
  }

  async function closeTab(sessionId: string) {
    try {
      await transport.kill(sessionId);
    } catch {
      // 会话已退出或后端已移除：忽略
    }
    tabs = tabs.filter((t) => t.sessionId !== sessionId);
    if (activeId === sessionId) activeId = tabs.at(-1)?.sessionId ?? null;
  }

  /** xterm action：节点挂载时创建终端并绑定输入/尺寸同步，销毁时释放。
   * 配合 {#key activeId}，切换 tab 时整块销毁重建（xterm 实例不能复用节点）。
   *
   * 字体时序：WebKitGTK 字体异步加载，若在字体就绪前 open，xterm 会用默认宽度
   * 测量 cell，导致字符间距异常变宽。因此先等 document.fonts.ready（带 1s 兜底）
   * 再 open + fit，确保按真实等宽字体度量渲染。 */
  function mountTerminal(node: HTMLDivElement, sessionId: string) {
    const term = new Terminal({
      theme: terminalTheme(themeName),
      // 等宽栈：Linux(WebKitGTK) 不识别 ui-monospace/Menlo/Consolas。
      // Ubuntu Mono / Liberation Mono 字形比 DejaVu Sans Mono 更紧凑，优先命中。
      // 已装字体前移：WebKitGTK 对未安装族名走 fontconfig 弱绑定回退（实测落到 Noto Sans
      // CJK 全角，字距明显变宽），而 Chrome 按 CSS 规范跳过未命中族；把系统已装字体放
      // 栈首可让两个引擎命中同一字体（Ubuntu Mono），消除字距差异。
      fontFamily:
        '"Ubuntu Mono", "Liberation Mono", "Noto Sans Mono", "DejaVu Sans Mono", "JetBrains Mono", "Fira Code", "Cascadia Mono", Consolas, Menlo, Monaco, monospace',
      fontSize: 11,
      lineHeight: 1.0,
      letterSpacing: 0,
      cursorBlink: true,
      scrollback: 5000,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);

    let destroyed = false;
    // 触摸拖动滚动的监听清理器（在 init 内注册，destroy 时统一移除）。
    let touchDisposers: (() => void)[] = [];
    const init = () => {
      if (destroyed) return;
      term.open(node);
      fit.fit();
      terms.set(sessionId, term);

      term.onData((data) => {
        // 已退出会话（如 agent 的一次性命令）不再回写输入，避免往已死 PTY 写数据报错。
        if (tabs.find((t) => t.sessionId === sessionId)?.exited) return;
        transport.write(sessionId, data).catch((e) => {
          errorMsg = t("terminal.writeFailed", { error: `${e}` });
        });
      });

      // 移动端触摸拖动滚动：xterm 6.0 的 Viewport 触摸处理只有类型声明而无实现
      // （IViewport.handleTouchStart/handleTouchMove），移动端需自行桥接。
      // 单指垂直拖动 → preventDefault 抑制原生滚动 + term.scrollLines() 换算行数；
      // 多指忽略（pinch 缩放放行给浏览器，由 .term-host 的 touch-action 允许）。
      let touchAnchorY: number | null = null;
      const onTouchStart = (e: TouchEvent) => {
        touchAnchorY = e.touches.length === 1 ? e.touches[0].clientY : null;
      };
      const onTouchMove = (e: TouchEvent) => {
        if (e.touches.length !== 1 || touchAnchorY === null) return;
        e.preventDefault();
        const rowsEl = term.element?.querySelector<HTMLElement>(".xterm-rows");
        const cellHeight = rowsEl ? rowsEl.clientHeight / term.rows : 14;
        const delta = touchAnchorY - e.touches[0].clientY;
        const lines = Math.trunc(delta / cellHeight);
        if (lines !== 0) {
          term.scrollLines(lines);
          // 消费掉的像素推进基线，子行余量留待下次 move 累计。
          touchAnchorY -= lines * cellHeight;
        }
      };
      const onTouchEnd = () => {
        touchAnchorY = null;
      };
      const termEl = term.element!;
      termEl.addEventListener("touchstart", onTouchStart, { passive: true });
      termEl.addEventListener("touchmove", onTouchMove); // 非 passive：需 preventDefault
      termEl.addEventListener("touchend", onTouchEnd);
      touchDisposers = [
        () => termEl.removeEventListener("touchstart", onTouchStart),
        () => termEl.removeEventListener("touchmove", onTouchMove),
        () => termEl.removeEventListener("touchend", onTouchEnd),
      ];

      term.focus();
    };
    const fontsReady = document.fonts
      ? Promise.race([document.fonts.ready, new Promise((r) => setTimeout(r, 1000))])
      : Promise.resolve();
    void fontsReady.then(init);

    // WebKitGTK 下系统字体在 open 时可能尚未完成度量（fonts.ready 对系统字体立即
    // resolve，renderer 可能拿到偏宽的 cell 宽度 → 字母间出现空隙）。等渲染稳定后
    // 重赋 fontFamily 强制 xterm 重新测量，并用 fit 重算列数。
    const remeasure = () => {
      if (destroyed || !terms.has(sessionId)) return;
      term.options.fontFamily = term.options.fontFamily; // 触发 renderer 重测
      fit.fit();
    };
    requestAnimationFrame(() => {
      remeasure();
      setTimeout(remeasure, 300);
    });

    const ro = new ResizeObserver(() => {
      if (destroyed || !terms.has(sessionId)) return;
      fit.fit();
      const { cols, rows } = term;
      transport.resize(sessionId, cols, rows).catch(() => {});
    });
    ro.observe(node);
    observers.set(sessionId, ro);

    return {
      destroy() {
        destroyed = true;
        for (const fn of touchDisposers) fn();
        touchDisposers = [];
        ro.disconnect();
        observers.delete(sessionId);
        terms.delete(sessionId);
        term.dispose();
      },
    };
  }
</script>

<div class="terminal-panel">
  {#if errorMsg}
    <button class="error-banner" type="button" onclick={() => (errorMsg = "")}>{errorMsg}</button>
  {/if}

  {#if connStatus !== "connected"}
    <p class="conn-banner" class:disconnected={connStatus === "disconnected"}>
      {connStatus === "connecting"
        ? t("terminal.connecting")
        : t("terminal.disconnected")}
    </p>
  {/if}

  <div class="tabbar">
    {#each tabs as tab (tab.sessionId)}
      <button
        type="button"
        class:active={tab.sessionId === activeId}
        onclick={() => (activeId = tab.sessionId)}
      >
        <span class="tab-title">{tab.title}</span>
        {#if tab.exited}<span class="tab-exited" title={t("terminal.exited")}>●</span>{/if}
        <span
          class="tab-close"
          role="button"
          tabindex="0"
          title={t("terminal.closeTab")}
          onkeydown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.stopPropagation();
              closeTab(tab.sessionId);
            }
          }}
          onclick={(e) => {
            e.stopPropagation();
            closeTab(tab.sessionId);
          }}
        >×</span>
      </button>
    {/each}
    <button
      type="button"
      class="tab-new"
      onclick={newTab}
      title={t("terminal.newTab")}
      aria-label={t("terminal.newTab")}
    >
      <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
    </button>
  </div>

  <div class="term-host-wrap">
    {#if tabs.length === 0}
      <p class="empty">{t("terminal.empty")}</p>
    {:else if activeId}
      {#key activeId}
        <div class="term-host" use:mountTerminal={activeId}></div>
      {/key}
    {/if}
  </div>
</div>

<style>
  .terminal-panel {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
  .tabbar {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px 6px 0;
    background: var(--color-surface);
    border-bottom: var(--border-width) solid var(--color-border);
    overflow-x: auto;
    flex-shrink: 0;
  }
  .tabbar button {
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    background: transparent;
    border: var(--border-width) solid transparent;
    border-bottom: none;
    border-radius: var(--radius-sm) var(--radius-sm) 0 0;
    padding: 4px 10px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
  }
  .tabbar button.active {
    color: var(--color-text);
    background: var(--color-bg);
    border-color: var(--color-border);
  }
  .tab-title {
    max-width: 140px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tab-exited { color: var(--color-error); font-size: var(--fs-xs); }
  .tab-close {
    font-size: 12px;
    line-height: 1;
    padding: 0 2px;
    border-radius: var(--radius-sm);
  }
  .tab-close:hover { background: var(--color-border); }
  .tab-new {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 4px 8px !important;
    align-self: flex-start;
    border-radius: var(--radius-sm) var(--radius-sm) 0 0;
    color: var(--color-text-muted);
  }
  .tab-new:hover { color: var(--color-text); background: var(--color-hover); }
  .tab-new svg { display: block; }
  .term-host-wrap {
    flex: 1;
    min-height: 0;
    background: var(--color-bg);
    padding: 6px;
    overflow: hidden;
  }
  /* 移动端手势策略：垂直拖动由 JS（scrollLines）接管，水平 pan 与双指缩放放行浏览器
     （touch-action 取交集作用于整个手势区域）；overscroll-behavior 阻止滚动到头时
     触发浏览器下拉刷新/橡皮筋滚动链。 */
  .term-host {
    width: 100%;
    height: 100%;
    touch-action: pan-x pinch-zoom;
    overscroll-behavior: contain;
  }
  .empty {
    padding: var(--space-3);
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
  }
  .error-banner {
    font-size: var(--fs-xs);
    color: var(--color-error);
    cursor: pointer;
    padding: 4px 8px;
    background: var(--color-surface);
  }
  .conn-banner {
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    padding: 4px 8px;
    background: var(--color-surface);
    border-bottom: var(--border-width) solid var(--color-border);
  }
  .conn-banner.disconnected {
    color: var(--color-error);
  }
</style>
