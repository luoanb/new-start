<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { PollerStatus } from "$lib/types";
  import { t } from "$lib/i18n";
  import { errorMessage } from "$lib/errorMessage";

  let {
    pollerStatus = $bindable(null),
  }: { pollerStatus: PollerStatus | null } = $props();

  let operating = $state(false);
  let errorMsg = $state("");

  async function refresh() {
    errorMsg = "";
    try {
      pollerStatus = await invoke<PollerStatus>("poll_status");
    } catch (e) {
      errorMsg = `Refresh failed: ${errorMessage(e)}`;
    }
  }

  async function handlePause() {
    operating = true;
    errorMsg = "";
    try {
      await invoke<void>("poll_pause");
      await refresh();
    } catch (e) {
      errorMsg = `Pause failed: ${errorMessage(e)}`;
    } finally {
      operating = false;
    }
  }

  async function handleResume() {
    operating = true;
    errorMsg = "";
    try {
      await invoke<void>("poll_resume");
      await refresh();
    } catch (e) {
      errorMsg = `Resume failed: ${errorMessage(e)}`;
    } finally {
      operating = false;
    }
  }

  async function handleTrigger() {
    operating = true;
    errorMsg = "";
    try {
      await invoke<void>("poll_trigger");
      await refresh();
    } catch (e) {
      errorMsg = `Trigger failed: ${errorMessage(e)}`;
    } finally {
      operating = false;
    }
  }

  function formatInterval(ms: number): string {
    if (ms >= 60000) return `${(ms / 60000).toFixed(1)}m`;
    if (ms >= 1000) return `${(ms / 1000).toFixed(0)}s`;
    return `${ms}ms`;
  }
</script>

<div class="poller-panel">
  {#if errorMsg}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="error-banner" onclick={() => (errorMsg = "")}>{errorMsg}</div>
  {/if}

  {#if !pollerStatus}
    <p class="empty">{t("pollerPanel.noPoller")}</p>
  {:else}
    <!-- Status card -->
    <div class="status-card">
      <div class="status-row">
        <span class="label">{t("pollerPanel.status")}</span>
        <span
          class="state-badge"
          class:running={pollerStatus.state === "running"}
          class:paused={pollerStatus.state === "paused"}
        >
          {pollerStatus.state === "running" ? t("pollerPanel.running") : t("pollerPanel.paused")}
        </span>
      </div>

      <div class="status-row">
        <span class="label">{t("pollerPanel.tickCount")}</span>
        <span class="value">{pollerStatus.tick_count}</span>
      </div>

      <div class="status-row">
        <span class="label">{t("pollerPanel.taskCount")}</span>
        <span class="value">{pollerStatus.task_count}</span>
      </div>

      <div class="status-row">
        <span class="label">{t("pollerPanel.interval")}</span>
        <span class="value">{formatInterval(pollerStatus.base_interval_ms)}</span>
      </div>

      {#if pollerStatus.pending_trigger}
        <div class="status-row">
          <span class="label">{t("pollerPanel.pendingTrigger")}</span>
          <span class="value pending">Yes</span>
        </div>
      {/if}
    </div>

    <!-- Controls -->
    <div class="controls">
      {#if pollerStatus.state === "running"}
        <button class="btn btn-warning" onclick={handlePause} disabled={operating}>
          {t("pollerPanel.pause")}
        </button>
      {:else}
        <button class="btn btn-primary" onclick={handleResume} disabled={operating}>
          {t("pollerPanel.resume")}
        </button>
      {/if}
      <button class="btn" onclick={handleTrigger} disabled={operating}>
        {operating ? t("pollerPanel.triggering") : t("pollerPanel.trigger")}
      </button>
    </div>
  {/if}
</div>

<style>
  .poller-panel { display: flex; flex-direction: column; gap: var(--space-2); }
  .error-banner { background: var(--color-danger, #ef4444); color: #fff; padding: var(--space-1) var(--space-2); border-radius: var(--radius-md); font-size: var(--fs-xs); cursor: pointer; }
  .empty { text-align: center; color: var(--color-text-muted); font-size: var(--fs-sm); padding: var(--space-4); }
  .status-card { display: flex; flex-direction: column; gap: var(--space-1); padding: var(--space-2); background: var(--color-bg); border: var(--border-width) solid var(--color-border); border-radius: var(--radius-md); }
  .status-row { display: flex; justify-content: space-between; align-items: center; font-size: var(--fs-sm); }
  .status-row .label { color: var(--color-text-muted); }
  .status-row .value { font-weight: 600; font-family: monospace; }
  .status-row .value.pending { color: var(--color-warning, #f59e0b); }
  .state-badge { font-size: var(--fs-xs); font-weight: 600; padding: 1px 10px; border-radius: var(--radius-sm); color: #fff; }
  .state-badge.running { background: var(--color-success, #22c55e); }
  .state-badge.paused { background: var(--color-warning, #f59e0b); }
  .controls { display: flex; gap: var(--space-1); }

  .btn { font-size: var(--fs-sm); padding: var(--space-1) var(--space-3); border: var(--border-width) solid var(--color-border); border-radius: var(--radius-sm); background: transparent; color: var(--color-text); cursor: pointer; }
  .btn-primary { background: var(--color-primary); color: var(--color-on-primary); border-color: var(--color-primary); }
  .btn-warning { background: var(--color-warning, #f59e0b); color: #fff; border-color: var(--color-warning, #f59e0b); }
  .btn:disabled { opacity: 0.4; cursor: default; }
</style>
