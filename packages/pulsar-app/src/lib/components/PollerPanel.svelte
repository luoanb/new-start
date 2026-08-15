<script lang="ts">
  import { t } from "$lib/i18n";
  import { errorMessage } from "$lib/errorMessage";
  import { dataStore } from "$lib/stores/dataStore.svelte";

  let pollerStatus = $derived(dataStore.state.poller);

  let operating = $state(false);
  let errorMsg = $state("");
  let parallelism = $state(0);

  $effect(() => {
    if (pollerStatus && parallelism === 0) {
      parallelism = pollerStatus.assistant_poll_parallelism;
    }
  });

  const parallelismDirty = $derived(
    parallelism !== pollerStatus?.assistant_poll_parallelism,
  );

  async function handleSaveParallelism() {
    if (!Number.isInteger(parallelism) || parallelism < 1 || parallelism > 8) {
      errorMsg = "Parallelism must be an integer between 1 and 8";
      return;
    }
    operating = true;
    errorMsg = "";
    try {
      await dataStore.setPollParallelism(parallelism);
    } catch (e) {
      errorMsg = `Save failed: ${errorMessage(e)}`;
    } finally {
      operating = false;
    }
  }

  async function handlePause() {
    operating = true;
    errorMsg = "";
    try {
      await dataStore.pausePoller();
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
      await dataStore.resumePoller();
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
      await dataStore.triggerPoller();
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
    <div class="columns">
      <!-- 左栏：状态 -->
      <div class="col col-status">
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
      </div>

      <!-- 右栏：并发推进 + 恢复/触发 -->
      <div class="col col-actions">
        <div class="parallelism-card">
          <div class="parallelism-header">
            <span class="parallelism-title">{t("pollerPanel.parallelism")}</span>
            <span class="parallelism-count" class:dirty={parallelismDirty}>×{parallelism}</span>
          </div>
          <input
            type="range"
            min="1"
            max="8"
            step="1"
            bind:value={parallelism}
            disabled={operating}
            aria-label={t("pollerPanel.parallelism")}
          />
          <div class="parallelism-footer">
            <span class="parallelism-hint">{t("pollerPanel.parallelismHint")}</span>
            <button
              class="btn btn-primary"
              onclick={handleSaveParallelism}
              disabled={!parallelismDirty || operating}
            >
              {t("pollerPanel.save")}
            </button>
          </div>
        </div>

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
      </div>
    </div>
  {/if}
</div>

<style>
  .poller-panel { display: flex; flex-direction: column; flex: 1; min-height: 0; gap: var(--space-2); padding: var(--space-3); overflow-y: auto; }
  .error-banner { background: var(--color-danger, #ef4444); color: #fff; padding: var(--space-1) var(--space-2); border-radius: var(--radius-md); font-size: var(--fs-xs); cursor: pointer; }
  .empty { text-align: center; color: var(--color-text-muted); font-size: var(--fs-sm); padding: var(--space-4); }

  /* 左右分栏：窄容器（侧栏/信息栏）下自动换行堆叠 */
  .columns { display: flex; flex-wrap: wrap; gap: var(--space-2); align-items: flex-start; }
  .col { display: flex; flex-direction: column; gap: var(--space-2); }
  .col-status { flex: 1 1 0; min-width: 220px; }
  .col-actions { flex: 1 1 0; min-width: 280px; }

  .status-card { display: flex; flex-direction: column; gap: var(--space-1); padding: var(--space-2); background: var(--color-bg); border: var(--border-width) solid var(--color-border); border-radius: var(--radius-md); }
  .status-row { display: flex; justify-content: flex-start; align-items: center; gap: var(--space-2); font-size: var(--fs-sm); }
  .status-row .label { color: var(--color-text-muted); }
  .status-row .value { font-weight: 600; font-family: monospace; }
  .status-row .value.pending { color: var(--color-warning, #f59e0b); }
  .state-badge { font-size: var(--fs-xs); font-weight: 600; padding: 1px 10px; border-radius: var(--radius-sm); color: #fff; }
  .state-badge.running { background: var(--color-success, #22c55e); }
  .state-badge.paused { background: var(--color-warning, #f59e0b); }

  .parallelism-card { display: flex; flex-direction: column; gap: var(--space-2); padding: var(--space-2); background: var(--color-bg); border: var(--border-width) solid var(--color-border); border-radius: var(--radius-md); }
  .parallelism-header { display: flex; justify-content: space-between; align-items: center; font-size: var(--fs-sm); }
  .parallelism-title { color: var(--color-text); font-weight: 600; }
  .parallelism-count { font-family: monospace; font-weight: 600; color: var(--color-text); }
  .parallelism-count.dirty { color: var(--color-primary); }
  .parallelism-card input[type="range"] { width: 100%; margin: 0; accent-color: var(--color-primary); }
  .parallelism-footer { display: flex; justify-content: space-between; align-items: center; gap: var(--space-2); }
  .parallelism-hint { font-size: var(--fs-xs); color: var(--color-text-muted); line-height: 1.4; }

  .controls { display: flex; gap: var(--space-1); }
  .btn { font-size: var(--fs-sm); padding: var(--space-1) var(--space-3); border: var(--border-width) solid var(--color-border); border-radius: var(--radius-sm); background: transparent; color: var(--color-text); cursor: pointer; }
  .btn-primary { background: var(--color-primary); color: var(--color-on-primary); border-color: var(--color-primary); }
  .btn-warning { background: var(--color-warning, #f59e0b); color: #fff; border-color: var(--color-warning, #f59e0b); }
  .btn:disabled { opacity: 0.4; cursor: default; }
</style>
