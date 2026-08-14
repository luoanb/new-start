<script lang="ts">
  import { t } from "$lib/i18n";
  import { createHttpClient, currentConn, isTauriEnv, switchConn } from "$lib/api";
  import type { ConnConfig } from "$lib/api/types";
  import { dataStore } from "$lib/stores/dataStore.svelte";

  let { open, onClose }: { open: boolean; onClose: () => void } = $props();

  let mode = $state<"local" | "remote">("local");
  let url = $state("");
  let token = $state("");
  let testStatus = $state<"idle" | "testing" | "ok" | "fail">("idle");
  let testMsg = $state("");
  let saving = $state(false);
  let error = $state("");

  // 每次打开时从当前配置回显表单。非 Tauri 环境不支持本机模式，一律回显远程。
  $effect(() => {
    if (open) {
      const cfg = currentConn();
      mode = !isTauriEnv ? "remote" : cfg.mode;
      url = cfg.url ?? "http://127.0.0.1:8787";
      token = cfg.token ?? "";
      testStatus = "idle";
      testMsg = "";
      error = "";
    }
  });

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
  <div class="overlay" onclick={onClose}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="modal" onclick={(e) => e.stopPropagation()}>
      <div class="modal-header">
        <h2>{t("connectDialog.title")}</h2>
        <button class="close-btn" onclick={onClose}>×</button>
      </div>
      <div class="modal-body">
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
        <button class="btn ghost" onclick={onClose} disabled={saving}>{t("connectDialog.cancel")}</button>
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
    background: var(--color-surface); border-radius: 16px; width: 420px; max-width: 90vw;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
  }
  .modal-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 16px 20px; border-bottom: 1px solid var(--color-border);
  }
  .modal-header h2 { margin: 0; font-size: 18px; font-weight: 600; }
  .close-btn {
    background: none; border: none; font-size: 22px; cursor: pointer;
    color: var(--color-text); padding: 0 4px; line-height: 1;
  }
  .modal-body { padding: 20px; display: flex; flex-direction: column; gap: 16px; }
  .field { display: flex; flex-direction: column; gap: 6px; }
  .field-label { font-size: 13px; color: var(--color-text-muted); }
  input {
    width: 100%; padding: 8px 10px; border: 1px solid var(--color-border);
    border-radius: 8px; background: var(--color-bg); color: var(--color-text);
    font-size: 14px; box-sizing: border-box;
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
  .mode-desc { font-size: 12px; color: var(--color-text-muted); }
  .test-row { display: flex; align-items: center; gap: 12px; }
  .test-result { font-size: 13px; }
  .test-result.ok { color: var(--color-success, #2e7d32); }
  .test-result.fail { color: var(--color-danger, #c62828); }
  .error { margin: 0; font-size: 13px; color: var(--color-danger, #c62828); }
  .modal-footer {
    display: flex; justify-content: flex-end; gap: 10px;
    padding: 14px 20px; border-top: 1px solid var(--color-border);
  }
  .btn {
    padding: 8px 16px; border-radius: 8px; border: 1px solid var(--color-border);
    background: var(--color-bg); color: var(--color-text); font-size: 14px; cursor: pointer;
  }
  .btn:hover { border-color: var(--color-primary); }
  .btn.primary { background: var(--color-primary); border-color: var(--color-primary); color: #fff; }
  .btn.primary:disabled, .btn:disabled { opacity: 0.6; cursor: default; }
</style>
