<script lang="ts">
  import { api, c } from "$lib/api";
  import type { Neuron, Connection } from "$lib/types";
  import { t } from "$lib/i18n";
  import { errorMessage } from "$lib/errorMessage";

  let {
    neuronId,
    onBack,
    onViewNetwork,
    onJumpTo,
  }: {
    neuronId: string;
    onBack: () => void;
    onViewNetwork: (id: string) => void;
    onJumpTo: (id: string) => void;
  } = $props();

  let neuron = $state<Neuron | null>(null);
  let connections = $state<Connection[]>([]);
  let loading = $state(true);
  let errorMsg = $state("");

  // ── Edit state ──
  let editing = $state(false);
  let editDesc = $state("");
  let editContent = $state("");
  let saving = $state(false);

  async function load() {
    loading = true;
    errorMsg = "";
    try {
      const [n, conns] = await Promise.all([
        api.call(c.getNeuron, { id: neuronId }),
        api.call(c.getConnections, { id: neuronId }),
      ]);
      neuron = n;
      connections = conns;
      editDesc = n.desc;
      editContent = n.content;
    } catch (e) {
      errorMsg = `Load failed: ${errorMessage(e)}`;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (neuronId) load();
  });

  async function handleSave() {
    if (!neuron) return;
    saving = true;
    errorMsg = "";
    try {
      const updated = await api.call(c.updateNeuron, {
        id: neuron.id,
        desc: editDesc !== neuron.desc ? editDesc : null,
        content: editContent !== neuron.content ? editContent : null,
      });
      neuron = updated;
      editing = false;
    } catch (e) {
      errorMsg = `Save failed: ${errorMessage(e)}`;
    } finally {
      saving = false;
    }
  }

  function formatTime(ts: number): string {
    return new Date(ts).toLocaleString();
  }
</script>

<div class="neuron-detail">
  <button class="back-btn" onclick={onBack}>← {t("neuronPanel.back")}</button>

  {#if loading}
    <p class="status-msg">{t("neuronPanel.loading")}</p>
  {:else if errorMsg}
    <div class="error-msg">{errorMsg}</div>
  {:else if neuron}
    <div class="detail-card">
      <div class="detail-header">
        <span class="neuron-id">{neuron.id}</span>
        {#if neuron.system_type}
          <span class="sys-tag">{neuron.system_type}</span>
        {/if}
      </div>

      <div class="weight-bar">
        <span>{t("neuronPanel.weight")}: {neuron.weight.toFixed(2)}</span>
        <div class="weight-bg"><div class="weight-fill" style="width: {Math.min(neuron.weight * 10, 100)}%"></div></div>
      </div>

      <div class="detail-section">
        {#if editing}
          <div class="edit-form">
            <label for="edit-desc">{t("neuronPanel.description")}</label>
            <textarea id="edit-desc" bind:value={editDesc} disabled={saving} rows="2"></textarea>
            <label for="edit-content">{t("neuronPanel.content")}</label>
            <textarea id="edit-content" bind:value={editContent} disabled={saving} rows="4"></textarea>
            <div class="edit-actions">
              <button class="btn btn-primary" onclick={handleSave} disabled={saving}>
                {saving ? t("neuronPanel.saving") : t("neuronPanel.save")}
              </button>
              <button class="btn" onclick={() => (editing = false)} disabled={saving}>
                {t("neuronPanel.cancel")}
              </button>
            </div>
          </div>
        {:else}
          <div class="view-fields">
            <div class="field">
              <span class="field-label">{t("neuronPanel.description")}</span>
              <span>{neuron.desc}</span>
            </div>
            <div class="field">
              <span class="field-label">{t("neuronPanel.content")}</span>
              <pre class="content-pre">{neuron.content}</pre>
            </div>
            <div class="field">
              <span class="field-label">{t("neuronPanel.createdAt")}</span>
              <span>{formatTime(neuron.created_at)}</span>
            </div>
            <div class="field">
              <span class="field-label">{t("neuronPanel.updatedAt")}</span>
              <span>{formatTime(neuron.updated_at)}</span>
            </div>
            {#if neuron.tool_ids.length > 0}
              <div class="field">
                <span class="field-label">{t("neuronPanel.toolIds")}</span>
                <span>{neuron.tool_ids.join(", ")}</span>
              </div>
            {/if}
          </div>
          <button class="btn" onclick={() => (editing = true)}>{t("neuronPanel.edit")}</button>
        {/if}
      </div>
    </div>

    <!-- Connections -->
    <div class="connections-section">
      <h3>{t("neuronPanel.connections")} ({connections.length})</h3>
      {#if connections.length === 0}
        <p class="empty-small">{t("neuronPanel.noNeurons")}</p>
      {:else}
        <div class="conn-list">
          {#each connections as c}
            <div class="conn-item">
              <button class="conn-node" onclick={() => onJumpTo(c.source)}>{c.source.slice(0, 16)}</button>
              <span class="conn-arrow">→</span>
              <span class="conn-weight">{c.weight.toFixed(2)}</span>
              <button class="conn-node" onclick={() => onJumpTo(c.target)}>{c.target.slice(0, 16)}</button>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Network action -->
    <button class="btn btn-primary network-btn" onclick={() => { if (neuron) onViewNetwork(neuron.id); }}>
      {t("neuronPanel.viewNetwork")}
    </button>
  {/if}
</div>

<style>
  .neuron-detail { display: flex; flex-direction: column; gap: var(--space-2); }
  .back-btn { align-self: flex-start; font-size: var(--fs-sm); padding: var(--space-1) var(--space-2); border: none; background: transparent; color: var(--color-primary); cursor: pointer; }
  .back-btn:hover { text-decoration: underline; }
  .status-msg, .error-msg { text-align: center; color: var(--color-text-muted); font-size: var(--fs-sm); padding: var(--space-4); }
  .error-msg { color: var(--color-error); }
  .detail-card { border: var(--border-width) solid var(--color-border); border-radius: var(--radius-md); padding: var(--space-2); background: var(--color-bg); }
  .detail-header { display: flex; gap: var(--space-2); align-items: center; margin-bottom: var(--space-2); }
  .neuron-id { font-family: var(--font-mono); font-size: var(--fs-xs); color: var(--color-text-muted); }
  .sys-tag { font-size: var(--fs-xs); font-weight: 600; padding: 1px 6px; border-radius: var(--radius-sm); background: var(--color-primary); color: var(--color-on-primary); }
  .weight-bar { font-size: var(--fs-xs); color: var(--color-text-muted); margin-bottom: var(--space-2); }
  .weight-bg { height: 4px; background: var(--color-border); border-radius: 2px; margin-top: 2px; overflow: hidden; }
  .weight-fill { height: 100%; background: var(--color-primary); border-radius: 2px; }
  .view-fields { display: flex; flex-direction: column; gap: var(--space-2); margin-bottom: var(--space-2); }
  .field { display: flex; flex-direction: column; gap: 2px; }
  .field-label { font-size: var(--fs-xs); font-weight: 600; color: var(--color-text-muted); }
  .field span { font-size: var(--fs-sm); }
  .content-pre { font-size: var(--fs-xs); font-family: monospace; background: var(--color-surface); padding: var(--space-1); border-radius: var(--radius-sm); max-height: 120px; overflow-y: auto; white-space: pre-wrap; word-break: break-all; }
  .edit-form { display: flex; flex-direction: column; gap: var(--space-1); }
  .edit-form label { font-size: var(--fs-xs); font-weight: 600; color: var(--color-text-muted); }
  .edit-form textarea { font-size: var(--fs-sm); padding: var(--space-1); border: var(--border-width) solid var(--color-border); border-radius: var(--radius-sm); background: var(--color-bg); color: var(--color-text); resize: vertical; font-family: inherit; }
  .edit-actions { display: flex; gap: var(--space-1); }
  .connections-section h3 { font-size: var(--fs-sm); font-weight: 600; margin: 0 0 var(--space-1); }
  .empty-small { font-size: var(--fs-xs); color: var(--color-text-muted); }
  .conn-list { display: flex; flex-direction: column; gap: 2px; }
  .conn-item { display: flex; align-items: center; gap: var(--space-1); padding: var(--space-1); border-radius: var(--radius-sm); background: var(--color-surface); font-size: var(--fs-xs); }
  .conn-node { font-family: monospace; color: var(--color-primary); background: none; border: none; cursor: pointer; padding: 0; font-size: var(--fs-xs); }
  .conn-node:hover { text-decoration: underline; }
  .conn-arrow { color: var(--color-text-muted); }
  .conn-weight { font-weight: 600; color: var(--color-text-muted); min-width: 40px; text-align: center; }
  .network-btn { align-self: flex-start; }

  .btn { font-size: var(--fs-xs); padding: 2px 10px; border: var(--border-width) solid var(--color-border); border-radius: var(--radius-sm); background: transparent; color: var(--color-text); cursor: pointer; }
  .btn-primary { background: var(--color-primary); color: var(--color-on-primary); border-color: var(--color-primary); }
  .btn:disabled { opacity: 0.4; cursor: default; }
</style>
