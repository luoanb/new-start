<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Connection, Neuron } from "$lib/types";
  import { t } from "$lib/i18n";

  export let neuron: Neuron | null = null;
  export let connections: Connection[] = [];
  export let onClose: () => void = () => {};
  export let onJumpTo: (id: string) => void = () => {};
  export let onChanged: () => void = () => {};
  export let onRequestCreateDownstream: (sourceId: string) => void = () => {};

  let editing = false;
  let saving = false;
  let weightBusy = false;
  const WEIGHT_STEP = 0.05;
  let weightDelta = 1;
  let edgeDelta = 1;
  let desc = "";
  let content = "";

  // 打开抽屉时重置编辑态
  $: if (neuron && !editing) {
    desc = neuron.desc;
    content = neuron.content;
  }

  async function handleSave() {
    if (!neuron) return;
    saving = true;
    try {
      await invoke("update_neuron", {
        id: neuron.id,
        desc,
        content,
      });
      neuron = { ...neuron, desc, content };
      editing = false;
    } catch (e) {
      console.error(String(e));
    } finally {
      saving = false;
    }
  }

  function handleCancel() {
    if (!neuron) return;
    desc = neuron.desc;
    content = neuron.content;
    editing = false;
  }

  async function adjustNeuron(delta: number) {
    if (!neuron || weightBusy) return;
    weightBusy = true;
    try {
      const updated = (await invoke("adjust_neuron_weight", {
        id: neuron.id,
        delta,
      })) as Neuron;
      neuron = updated;
      onChanged();
    } catch (e) {
      console.error(String(e));
    } finally {
      weightBusy = false;
    }
  }

  async function adjustEdge(c: Connection, delta: number) {
    if (weightBusy) return;
    weightBusy = true;
    try {
      const updated = (await invoke("adjust_edge_weight", {
        source: c.source,
        target: c.target,
        delta,
      })) as Connection;
      connections = connections.map((x) =>
        x.source === updated.source && x.target === updated.target ? updated : x
      );
      onChanged();
    } catch (e) {
      console.error(String(e));
    } finally {
      weightBusy = false;
    }
  }

  function fmtTime(ts: number): string {
    if (!ts) return "—";
    return new Date(ts * 1000).toLocaleString();
  }
</script>

<div class="drawer" class:open={!!neuron}>
  {#if neuron}
    <div class="drawer-head">
      <span class="type-bar" style:background={`var(--color-system-${neuron.system_type || "default"}, var(--color-system-default))`}></span>
      <span class="title">{t("neuronPanel.drawerTitle")}</span>
      <button class="close" on:click={onClose} title={t("neuronPanel.close")}>×</button>
    </div>

    <div class="drawer-body">
      <div class="field">
        <label>{t("neuronPanel.systemType")}</label>
        <span class="value mono">{neuron.system_type || "—"}</span>
      </div>
      <div class="field">
        <label>{t("neuronPanel.weight")}</label>
        <div class="stepper">
          <button class="step-btn" on:click={() => adjustNeuron(-WEIGHT_STEP)} disabled={weightBusy}>−</button>
          <span class="value mono step-val">{neuron.weight.toFixed(4)}</span>
          <button class="step-btn" on:click={() => adjustNeuron(WEIGHT_STEP)} disabled={weightBusy}>＋</button>
        </div>
        <div class="delta-row">
          <label class="delta-label">{t("neuronPanel.delta")}</label>
          <input
            class="delta-input"
            type="number"
            step="0.05"
            bind:value={weightDelta}
            on:keydown={(e) => { if (e.key === "Enter") adjustNeuron(weightDelta); }}
          />
          <button class="btn small primary" on:click={() => adjustNeuron(weightDelta)} disabled={weightBusy}>
            {t("neuronPanel.apply")}
          </button>
        </div>
      </div>

      <div class="field">
        <label>{t("neuronPanel.description")}</label>
        {#if editing}
          <textarea bind:value={desc} rows="2"></textarea>
        {:else}
          <span class="value">{neuron.desc || "—"}</span>
        {/if}
      </div>

      <div class="field">
        <label>{t("neuronPanel.content")}</label>
        {#if editing}
          <textarea bind:value={content} rows="6"></textarea>
        {:else}
          <span class="value pre">{neuron.content || "—"}</span>
        {/if}
      </div>

      {#if neuron.tool_ids && neuron.tool_ids.length}
        <div class="field">
          <label>{t("neuronPanel.toolIds")}</label>
          <span class="value mono">{neuron.tool_ids.join(", ")}</span>
        </div>
      {/if}

      <div class="field">
        <label>{t("neuronPanel.createdAt")}</label>
        <span class="value">{fmtTime(neuron.created_at)}</span>
      </div>
      <div class="field">
        <label>{t("neuronPanel.updatedAt")}</label>
        <span class="value">{fmtTime(neuron.updated_at)}</span>
      </div>

      <div class="field col">
        <label>{t("neuronPanel.connections")} ({connections.length})</label>
        {#if connections.length === 0}
          <span class="value muted">—</span>
        {:else}
          <ul class="conns">
            {#each connections as c (c.source + "->" + c.target)}
              {@const selfId = neuron!.id}
              <li>
                <button class="conn-link" on:click={() => onJumpTo(c.source === selfId ? c.target : c.source)}>
                  {c.source === selfId ? c.target : c.source}
                </button>
                <div class="stepper small">
                  <button class="step-btn" on:click={() => adjustEdge(c, -WEIGHT_STEP)} disabled={weightBusy}>−</button>
                  <span class="conn-w">w{c.weight.toFixed(2)}</span>
                  <button class="step-btn" on:click={() => adjustEdge(c, WEIGHT_STEP)} disabled={weightBusy}>＋</button>
                  <input
                    class="delta-input small"
                    type="number"
                    step="0.05"
                    bind:value={edgeDelta}
                    on:keydown={(e) => { if (e.key === "Enter") adjustEdge(c, edgeDelta); }}
                  />
                  <button class="btn small primary" on:click={() => adjustEdge(c, edgeDelta)} disabled={weightBusy}>
                    {t("neuronPanel.apply")}
                  </button>
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </div>

    <div class="drawer-foot">
      <button class="btn primary" on:click={() => onRequestCreateDownstream(neuron!.id)}>
        {t("neuronPanel.createDownstreamFromHere")}
      </button>
      {#if editing}
        <button class="btn primary" on:click={handleSave} disabled={saving}>
          {saving ? t("neuronPanel.saving") : t("neuronPanel.save")}
        </button>
        <button class="btn" on:click={handleCancel}>{t("neuronPanel.cancel")}</button>
      {:else}
        <button class="btn" on:click={() => (editing = true)}>{t("neuronPanel.edit")}</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .drawer {
    position: absolute;
    top: 0;
    right: 0;
    height: 100%;
    width: 320px;
    background: var(--color-surface);
    border-left: 1px solid var(--color-border);
    box-shadow: -8px 0 24px rgba(0, 0, 0, 0.12);
    display: flex;
    flex-direction: column;
    transform: translateX(100%);
    opacity: 0;
    pointer-events: none;
    transition:
      transform 0.22s ease,
      opacity 0.22s ease;
    z-index: 20;
  }
  .drawer.open {
    transform: translateX(0);
    opacity: 1;
    pointer-events: auto;
  }

  .drawer-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--color-border);
  }
  .type-bar {
    width: 4px;
    height: 18px;
    border-radius: 2px;
  }
  .title {
    flex: 1;
    font-weight: 600;
    font-size: 13px;
    color: var(--color-text);
  }
  .close {
    background: none;
    border: none;
    color: var(--color-text-muted);
    font-size: 20px;
    line-height: 1;
    cursor: pointer;
    padding: 0 4px;
  }
  .close:hover {
    color: var(--color-text);
  }

  .drawer-body {
    flex: 1;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .field.col {
    gap: 6px;
  }
  label {
    font-size: 11px;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .value {
    font-size: 12.5px;
    color: var(--color-text);
    line-height: 1.5;
    word-break: break-word;
  }
  .value.pre {
    white-space: pre-wrap;
  }
  .value.mono,
  .mono {
    font-family: var(--font-mono);
    font-size: 11.5px;
  }
  .value.muted {
    color: var(--color-text-muted);
  }

  textarea {
    width: 100%;
    background: var(--color-bg);
    color: var(--color-text);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 6px 8px;
    font-size: 12px;
    font-family: inherit;
    resize: vertical;
  }

  .conns {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .conns li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .stepper {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .stepper.small {
    gap: 4px;
  }
  .step-btn {
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    color: var(--color-text);
    border-radius: 6px;
    width: 22px;
    height: 22px;
    font-size: 13px;
    line-height: 1;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition:
      background 0.15s ease,
      border-color 0.15s ease;
  }
  .stepper.small .step-btn {
    width: 18px;
    height: 18px;
    font-size: 11px;
  }
  .step-btn:hover:not(:disabled) {
    border-color: var(--color-primary);
    color: var(--color-primary);
  }
  .step-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .step-val {
    min-width: 56px;
    text-align: center;
  }
  .delta-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
  }
  .delta-label {
    font-size: 12px;
    color: var(--color-muted);
  }
  .delta-input {
    width: 72px;
    padding: 4px 6px;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-bg);
    color: var(--color-text);
    font-size: 12px;
  }
  .delta-input.small {
    width: 56px;
  }
  .btn.small {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
  }
  .btn.primary {
    border: 1px solid var(--color-primary);
    background: var(--color-primary);
    color: #fff;
    border-radius: 6px;
    cursor: pointer;
  }
  .btn.primary:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .conn-link {
    background: none;
    border: none;
    color: var(--color-primary);
    font-size: 12px;
    cursor: pointer;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }
  .conn-link:hover {
    text-decoration: underline;
  }
  .conn-w {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--color-text-muted);
  }

  .drawer-foot {
    display: flex;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--color-border);
  }
  .btn {
    flex: 1;
    padding: 7px 10px;
    border-radius: 8px;
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: 12px;
    cursor: pointer;
    transition: background 0.15s ease;
  }
  .btn:hover {
    background: var(--color-hover);
  }
  .btn.primary {
    background: var(--color-primary);
    border-color: var(--color-primary);
    color: var(--color-on-primary);
  }
  .btn.primary:disabled {
    opacity: 0.6;
    cursor: default;
  }
</style>
