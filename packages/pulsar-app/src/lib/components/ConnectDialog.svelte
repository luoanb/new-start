<script lang="ts">
  import { t } from "$lib/i18n";
  import { api, createHttpClient, currentConn, DEFAULT_REMOTE_URL, isTauriEnv, switchConn } from "$lib/api";
  import type { ConnConfig, LanAddress, ServerAction, ServerInfo } from "$lib/api/types";
  import { dataStore } from "$lib/stores/dataStore.svelte";
  import { formatInvokeError } from "$lib/utils/formatInvokeError";
  import CopyButton from "./CopyButton.svelte";
  import Toggle from "./Toggle.svelte";

  let { open, onClose, locked = false }: { open: boolean; onClose: () => void; locked?: boolean } = $props();

  let mode = $state<"local" | "remote">("local");
  let url = $state("");
  let token = $state("");
  let testStatus = $state<"idle" | "testing" | "ok" | "fail">("idle");
  let testMsg = $state("");
  let saving = $state(false);
  let error = $state("");

  // 后端连接服务（内嵌 HTTP server）：服务启停 与 配置项 分离（对齐 Linux 服务管理）——
  // 「启停」只改运行态；「配置」（自启/局域网）只落盘，下次启动生效。仅桌面端可控。
  let serverRunning = $state(false);
  let autoStart = $state(false);
  let lan = $state(false);
  let serverHost = $state("");
  let serverPort = $state(0);
  let lanAddrs = $state<LanAddress[]>([]);
  let serverToken = $state<string | null>(null);
  /** 进行中的后端操作：控制动作名 / "config"（配置写入）；null = 空闲。 */
  let serverBusyAction = $state<ServerAction | "config" | null>(null);
  let serverBusy = $derived(serverBusyAction !== null);
  let serverError = $state("");

  /** 配置值（lan）与实际绑定不一致 → 需重启服务生效。 */
  let needsRestart = $derived(serverRunning && lan !== isLanHost(serverHost));
  /** 本机访问地址。 */
  let localUrl = $derived(`http://localhost:${serverPort}/`);

  /** 绑定地址是否对外开放（非 loopback）。 */
  function isLanHost(host: string): boolean {
    const value = host.trim().toLowerCase();
    if (value === "localhost" || value === "::1") return false;
    return !value.startsWith("127.");
  }

  /** 指定网卡的局域网访问地址。 */
  function networkUrl(ip: string): string {
    return `http://${ip}:${serverPort}/`;
  }

  /** 用后端返回的 ServerInfo 刷新展示状态。 */
  function applyServerInfo(info: ServerInfo) {
    serverRunning = info.running;
    autoStart = info.enabled;
    lan = info.lan;
    serverHost = info.host;
    serverPort = info.port;
    lanAddrs = info.lan_addresses ?? [];
    serverToken = info.token;
  }

  // 每次打开时从当前配置回显表单。非 Tauri 环境不支持本机模式，一律回显远程。
  $effect(() => {
    if (open) {
      const cfg = currentConn();
      mode = !isTauriEnv ? "remote" : cfg.mode;
      url = cfg.url ?? DEFAULT_REMOTE_URL;
      token = cfg.token ?? "";
      testStatus = "idle";
      testMsg = "";
      error = "";
      serverError = "";
      if (isTauriEnv) void refreshServer();
    }
  });

  // 读取后端服务状态（仅桌面端；含配置、网卡地址与令牌）。
  async function refreshServer() {
    try {
      applyServerInfo(await api.serverInfo());
    } catch (e) {
      serverError = t("connectDialog.serverToggleError", { error: formatInvokeError(e) });
    }
  }

  // 执行一次后端操作并回填状态；失败时回退本地开关并回显错误。
  async function runServerAction(
    action: ServerAction | "config",
    fn: () => Promise<ServerInfo>,
    rollback?: () => void,
  ) {
    serverBusyAction = action;
    serverError = "";
    try {
      applyServerInfo(await fn());
    } catch (e) {
      rollback?.();
      serverError = t("connectDialog.serverToggleError", { error: formatInvokeError(e) });
    } finally {
      serverBusyAction = null;
    }
  }

  // 服务控制（≈ systemctl start/stop/restart）。
  function controlServer(action: ServerAction) {
    void runServerAction(action, () => api.serverControl(action));
  }

  // 配置项：随应用启动自动开启（≈ systemctl enable/disable）。
  function toggleAutoStart(next: boolean) {
    const previous = !next;
    void runServerAction(
      "config",
      () => api.serverConfig({ enabled: next }),
      () => {
        autoStart = previous;
      },
    );
  }

  // 配置项：局域网访问（只落盘，下次启动生效）。
  function toggleLan(next: boolean) {
    const previous = !next;
    void runServerAction(
      "config",
      () => api.serverConfig({ lan: next }),
      () => {
        lan = previous;
      },
    );
  }

  // 测试连接：用目标配置构造临时客户端，不切换当前 api 实例。
  async function testConnection() {
    const target = targetConfig();
    if (target.error) {
      testStatus = "fail";
      testMsg = target.error;
      return;
    }
    testStatus = "testing";
    const ok = await createHttpClient(target.config).health();
    testStatus = ok ? "ok" : "fail";
    testMsg = ok ? t("connectDialog.reachable") : t("connectDialog.unreachable");
  }

  // 保存即热切换：解除旧订阅 → 替换 api 实例 → 全量重拉 → 重新订阅。
  async function save() {
    const target = targetConfig();
    if (target.error) {
      error = target.error;
      return;
    }
    saving = true;
    error = "";
    try {
      dataStore.unsubscribe();
      switchConn(target.config);
      await dataStore.bootstrap();
      await dataStore.subscribe();
      // bootstrap 失败会写入 state.error（连接已切换、数据未就绪），面板保留以允许改回。
      if (dataStore.state.error) {
        error = t("connectDialog.switchFailed", { error: dataStore.state.error });
      } else {
        onClose();
      }
    } finally {
      saving = false;
    }
  }

  function targetConfig(): { config: ConnConfig; error?: string } {
    // 非 Tauri 环境不支持本机 IPC：忽略 local，强制远程。
    const effective = !isTauriEnv && mode === "local" ? "remote" : mode;
    if (effective === "local") return { config: { mode: "local" } };
    const trimmed = url.trim();
    if (!trimmed) return { config: { mode: "remote", url: trimmed }, error: t("connectDialog.needUrl") };
    return {
      config: { mode: "remote", url: trimmed, token: token.trim() || undefined },
    };
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="overlay" onclick={() => { if (!locked) onClose(); }}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="modal" onclick={(e) => e.stopPropagation()}>
      <div class="modal-header">
        <h2>{t("connectDialog.title")}</h2>
        {#if !locked}
          <button class="close-btn" onclick={onClose}>×</button>
        {/if}
      </div>
      <div class="modal-body">
        {#if locked}
          <p class="locked-hint">{t("connectDialog.lockedHint")}</p>
        {/if}
        {#if isTauriEnv}
          <div class="server-block">
            <!-- 服务启停 -->
            <div class="server-row">
              <div class="server-info">
                <span class="field-label">{t("connectDialog.serverService")}</span>
                <span class="server-status" class:on={serverRunning}>
                  {serverRunning
                    ? t("connectDialog.serverRunning", { addr: `localhost:${serverPort}` })
                    : t("connectDialog.serverStopped")}
                </span>
              </div>
              <div class="server-actions">
                {#if serverRunning}
                  <button class="btn" disabled={serverBusy} onclick={() => controlServer("stop")}>
                    {#if serverBusyAction === "stop"}<span class="spinner"></span>{/if}
                    {serverBusyAction === "stop" ? t("connectDialog.serverBusy") : t("connectDialog.serverStop")}
                  </button>
                  <button class="btn" disabled={serverBusy} onclick={() => controlServer("restart")}>
                    {#if serverBusyAction === "restart"}<span class="spinner"></span>{/if}
                    {serverBusyAction === "restart" ? t("connectDialog.serverBusy") : t("connectDialog.serverRestart")}
                  </button>
                {:else}
                  <button class="btn primary" disabled={serverBusy} onclick={() => controlServer("start")}>
                    {#if serverBusyAction === "start"}<span class="spinner"></span>{/if}
                    {serverBusyAction === "start" ? t("connectDialog.serverBusy") : t("connectDialog.serverStart")}
                  </button>
                {/if}
              </div>
            </div>

            <!-- 配置项：只落盘，不改运行态 -->
            <div class="server-row">
              <span class="field-label">{t("connectDialog.autoStart")}</span>
              <Toggle bind:checked={autoStart} disabled={serverBusy} onchange={toggleAutoStart} />
            </div>
            <div class="server-row">
              <span class="field-label">{t("connectDialog.lanAccess")}</span>
              <Toggle bind:checked={lan} disabled={serverBusy} onchange={toggleLan} />
            </div>
            <p class="server-hint">{t("connectDialog.lanAccessHint")}</p>
            {#if needsRestart}
              <p class="server-hint warn">{t("connectDialog.restartHint")}</p>
            {/if}

            <!-- 运行中：访问地址 + 令牌（每行可复制，仅复制 URL / 令牌本身） -->
            {#if serverRunning}
              <div class="addr-list">
                <div class="addr-row">
                  <span class="addr-label">{t("connectDialog.addrLocal")}</span>
                  <code class="addr-url">{localUrl}</code>
                  <CopyButton text={localUrl} />
                </div>
                {#if isLanHost(serverHost)}
                  {#each lanAddrs as a (`${a.name}:${a.ip}`)}
                    <div class="addr-row">
                      <span class="addr-label">{t("connectDialog.addrNetwork")} ({a.name})</span>
                      <code class="addr-url">{networkUrl(a.ip)}</code>
                      <CopyButton text={networkUrl(a.ip)} />
                    </div>
                  {/each}
                {/if}
                {#if serverToken}
                  <div class="addr-row">
                    <span class="addr-label">{t("connectDialog.addrToken")}</span>
                    <code class="addr-url">{serverToken}</code>
                    <CopyButton text={serverToken} />
                  </div>
                {/if}
              </div>
            {/if}

            {#if serverError}
              <p class="error">{serverError}</p>
            {/if}
          </div>
        {/if}
        <div class="field">
          <span class="field-label">{t("connectDialog.mode")}</span>
          <div class="mode-options">
            {#if isTauriEnv}
              <button
                class="mode-card"
                class:selected={mode === "local"}
                onclick={() => { mode = "local"; testStatus = "idle"; testMsg = ""; }}
              >
                <strong>{t("connectDialog.modeLocal")}</strong>
                <span class="mode-desc">{t("connectDialog.modeLocalHint")}</span>
              </button>
            {/if}
            <button
              class="mode-card"
              class:selected={mode === "remote"}
              onclick={() => { mode = "remote"; testStatus = "idle"; testMsg = ""; }}
            >
              <strong>{t("connectDialog.modeRemote")}</strong>
              <span class="mode-desc">{t("connectDialog.modeRemoteHint")}</span>
            </button>
          </div>
        </div>

        {#if mode === "remote"}
          <div class="field">
            <label class="field-label" for="connect-url">{t("connectDialog.address")}</label>
            <input
              id="connect-url"
              type="text"
              bind:value={url}
              placeholder={t("connectDialog.addressPlaceholder")}
              oninput={() => { testStatus = "idle"; testMsg = ""; }}
            />
          </div>
          <div class="field">
            <label class="field-label" for="connect-token">{t("connectDialog.token")}</label>
            <input
              id="connect-token"
              type="password"
              bind:value={token}
              placeholder={t("connectDialog.tokenHint")}
              autocomplete="off"
            />
          </div>
          <div class="test-row">
            <button class="btn" onclick={testConnection} disabled={testStatus === "testing"}>
              {testStatus === "testing" ? t("connectDialog.testing") : t("connectDialog.test")}
            </button>
            {#if testStatus === "ok"}
              <span class="test-result ok">✓ {testMsg}</span>
            {:else if testStatus === "fail"}
              <span class="test-result fail">✗ {testMsg}</span>
            {/if}
          </div>
        {/if}

        {#if error}
          <p class="error">{error}</p>
        {/if}
      </div>
      <div class="modal-footer">
        {#if !locked}
          <button class="btn ghost" onclick={onClose} disabled={saving}>{t("connectDialog.cancel")}</button>
        {/if}
        <button class="btn primary" onclick={save} disabled={saving}>
          {saving ? t("connectDialog.saving") : t("connectDialog.save")}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed; inset: 0; background: rgba(0, 0, 0, 0.4);
    display: flex; align-items: center; justify-content: center; z-index: 100;
  }
  .modal {
    background: var(--color-surface); border-radius: 16px; width: 520px; max-width: 90vw;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
  }
  .modal-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 16px 20px; border-bottom: 1px solid var(--color-border);
  }
  .modal-header h2 { margin: 0; font-size: var(--fs-lg); font-weight: 500; }
  .close-btn {
    background: none; border: none; font-size: 22px; cursor: pointer;
    color: var(--color-text); padding: 0 4px; line-height: 1;
  }
  .modal-body { padding: 20px; display: flex; flex-direction: column; gap: 16px; }
  .field { display: flex; flex-direction: column; gap: 6px; }
  .field-label { font-size: var(--fs-sm); color: var(--color-text-muted); }
  .server-block { display: flex; flex-direction: column; gap: 6px; }
  .server-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .server-actions { display: flex; gap: 8px; }
  .server-actions .btn { display: inline-flex; align-items: center; gap: 6px; }
  .spinner {
    width: 12px; height: 12px; flex-shrink: 0; border-radius: 50%;
    border: 2px solid currentColor; border-top-color: transparent;
    animation: spin 0.7s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  .server-info { display: flex; flex-direction: column; gap: 2px; }
  .server-status { font-size: var(--fs-xs); color: var(--color-text-muted); }
  .server-status.on { color: var(--color-success); }
  .server-hint { margin: 0; font-size: var(--fs-xs); color: var(--color-text-muted); }
  .server-hint.warn { color: var(--color-warning); }
  .addr-list { display: flex; flex-direction: column; gap: 6px; }
  .addr-row { display: flex; align-items: center; gap: 8px; }
  .addr-label { flex-shrink: 0; min-width: 88px; font-size: var(--fs-xs); color: var(--color-text-muted); }
  .addr-url {
    flex: 1; min-width: 0; padding: 4px 8px; border-radius: 6px;
    background: var(--color-bg); border: 1px solid var(--color-border);
    font-family: monospace; font-size: var(--fs-xs); color: var(--color-text);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  input {
    width: 100%; padding: 8px 10px; border: 1px solid var(--color-border);
    border-radius: 8px; background: var(--color-bg); color: var(--color-text);
    font-size: var(--fs-sm); box-sizing: border-box;
  }
  input:focus { outline: none; border-color: var(--color-primary); }
  .mode-options { display: flex; gap: 8px; }
  .mode-card {
    flex: 1; display: flex; flex-direction: column; gap: 4px; padding: 12px 14px;
    border: 1px solid var(--color-border); border-radius: 10px;
    background: var(--color-bg); cursor: pointer; text-align: left;
    transition: border-color 0.15s, background 0.15s; color: var(--color-text);
  }
  .mode-card:hover { border-color: var(--color-primary); background: var(--color-hover); }
  .mode-card.selected { border-color: var(--color-primary); }
  .mode-desc { font-size: var(--fs-sm); color: var(--color-text-muted); }
  .test-row { display: flex; align-items: center; gap: 12px; }
  .test-result { font-size: var(--fs-sm); }
  .test-result.ok { color: var(--color-success); }
  .test-result.fail { color: var(--color-error); }
  .error { margin: 0; font-size: var(--fs-sm); color: var(--color-error); }
  .locked-hint { margin: 0; font-size: var(--fs-sm); color: var(--color-warning); }
  .modal-footer {
    display: flex; justify-content: flex-end; gap: 10px;
    padding: 14px 20px; border-top: 1px solid var(--color-border);
  }
  .btn {
    padding: 8px 16px; border-radius: 8px; border: 1px solid var(--color-border);
    background: var(--color-bg); color: var(--color-text); font-size: var(--fs-sm); cursor: pointer;
  }
  .btn:hover { border-color: var(--color-primary); }
  .btn.primary { background: var(--color-primary); border-color: var(--color-primary); color: var(--color-on-primary); }
  .btn.primary:disabled, .btn:disabled { opacity: 0.6; cursor: default; }
</style>
