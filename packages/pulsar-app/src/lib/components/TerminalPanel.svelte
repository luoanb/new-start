<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import { isTauriEnv, readConnConfig, DEFAULT_REMOTE_URL } from "$lib/api";
  import {
    ipcTransport,
    wsTransport,
    type TerminalConnStatus,
    type TerminalTransport,
  } from "$lib/terminal/transport";

  /**
   * 浏览器模式直连 Tauri 进程内嵌的 WS 公共通道（见 spec terminal-browser-ws）。
   * 地址从远程连接配置（pulsar:remoteUrl）自动推导：http(s)://host:port → ws(s)://host:port/ws，
   * 配置了 token 时追加 ?token=；未配置时回落默认地址。WS 与 HTTP RPC 同端口同监听。
   */
  function deriveWsUrl(): string {
    const cfg = readConnConfig();
    const base = cfg.url || DEFAULT_REMOTE_URL;
    const scheme = base.replace(/^http/, "ws"); // http→ws / https→wss
    const query = cfg.token ? `?token=${encodeURIComponent(cfg.token)}` : "";
    return `${scheme}/ws${query}`;
  }
  const WS_URL = typeof window !== "undefined" ? deriveWsUrl() : "";

  type TerminalTab = {
    sessionId: string;
    title: string;
    exited: boolean;
  };

  // VS Code 风格深色主题；浅色模式适配留到 M4 打磨（终端固定深色语义）。
  const TERMINAL_THEME = {
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
        errorMsg = `Terminal init failed: ${e}`;
      });
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
      ?.write(`\r\n\x1b[90m[process exited with code ${exitCode}]\x1b[0m\r\n`);
    tabs = tabs.map((t) => (t.sessionId === sessionId ? { ...t, exited: true } : t));
  }

  async function newTab() {
    errorMsg = "";
    try {
      const sessionId = await transport.spawn();
      tabs = [...tabs, { sessionId, title: sessionId, exited: false }];
      activeId = sessionId;
    } catch (e) {
      errorMsg = `Spawn failed: ${e}`;
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
      theme: TERMINAL_THEME,
      // 等宽栈：Linux(WebKitGTK) 不识别 ui-monospace/Menlo/Consolas。
      // Ubuntu Mono / Liberation Mono 字形比 DejaVu Sans Mono 更紧凑，优先命中。
      fontFamily:
        '"JetBrains Mono", "Fira Code", "Cascadia Mono", "Ubuntu Mono", "Liberation Mono", "Noto Sans Mono", "DejaVu Sans Mono", Consolas, Menlo, Monaco, monospace',
      fontSize: 11,
      lineHeight: 1.0,
      letterSpacing: 0,
      cursorBlink: true,
      scrollback: 5000,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);

    let destroyed = false;
    const init = () => {
      if (destroyed) return;
      term.open(node);
      fit.fit();
      terms.set(sessionId, term);

      term.onData((data) => {
        // 已退出会话（如 agent 的一次性命令）不再回写输入，避免往已死 PTY 写数据报错。
        if (tabs.find((t) => t.sessionId === sessionId)?.exited) return;
        transport.write(sessionId, data).catch((e) => {
          errorMsg = `Write failed: ${e}`;
        });
      });
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
        ? "Connecting to terminal server…"
        : "Terminal connection lost. Retrying…"}
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
        {#if tab.exited}<span class="tab-exited" title="exited">●</span>{/if}
        <span
          class="tab-close"
          role="button"
          tabindex="0"
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
    <button type="button" class="tab-new" onclick={newTab} title="New terminal">+</button>
  </div>

  <div class="term-host-wrap">
    {#if tabs.length === 0}
      <p class="empty">No terminal. Click + to create one.</p>
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
  .tab-exited { color: #c0392b; font-size: 10px; }
  .tab-close {
    font-size: 12px;
    line-height: 1;
    padding: 0 2px;
    border-radius: 2px;
  }
  .tab-close:hover { background: var(--color-border); }
  .tab-new { font-size: 14px; padding: 2px 8px !important; }
  .term-host-wrap {
    flex: 1;
    min-height: 0;
    background: var(--color-bg);
    padding: 6px;
    overflow: hidden;
  }
  .term-host { width: 100%; height: 100%; }
  .empty {
    padding: var(--space-3);
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
  }
  .error-banner {
    font-size: var(--fs-xs);
    color: #c0392b;
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
    color: #c0392b;
  }
</style>
