<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { Connection, Neuron } from "$lib/types";
  import { t } from "$lib/i18n";
  import { formatInvokeError } from "$lib/utils/formatInvokeError";

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
  let toolIds: string[] = [];
  let availableTools: { name: string; description: string }[] = [];

  let lastNeuronId: string | null = null;
  let copied = false;
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
  let saveError: string | null = null;

  async function copyId() {
    if (!neuron) return;
    try {
      await navigator.clipboard.writeText(neuron.id);
      copied = true;
      clearTimeout(copyTimer);
      copyTimer = setTimeout(() => (copied = false), 1500);
    } catch {
      // 忽略复制失败
    }
  }

  // 打开抽屉 / 切换神经元时：重置编辑态并加载该神经元的最新数据
  $: if (neuron && neuron.id !== lastNeuronId) {
    lastNeuronId = neuron.id;
    editing = false;
    saveError = null;
    desc = neuron.desc;
    content = neuron.content;
    toolIds = neuron.tool_ids ? [...neuron.tool_ids] : [];
  }

  // 关闭抽屉时清空记忆，下次打开任意神经元都会重新初始化
  $: if (!neuron) {
    lastNeuronId = null;
    editing = false;
  }

  onMount(() => {
    invoke("list_skills")
      .then((skills) => {
        availableTools = skills as { name: string; description: string }[];
      })
      .catch(() => {
        availableTools = [];
      });
  });

  async function handleSave() {
    if (!neuron) return;
    saving = true;
    saveError = null;
    try {
      await invoke("update_neuron", {
        id: neuron.id,
        desc,
        content,
        toolIds,
      });
      neuron = { ...neuron, desc, content, tool_ids: toolIds };
      editing = false;
      onChanged();
    } catch (e) {
      console.error(e);
      saveError = formatInvokeError(e);
    } finally {
      saving = false;
    }
  }

  function handleCancel() {
    if (!neuron) return;
    desc = neuron.desc;
    content = neuron.content;
    toolIds = neuron.tool_ids ? [...neuron.tool_ids] : [];
    editing = false;
  }

  // 勾选 / 取消工具即时保存（与权重步进器一致），不依赖「保存」按钮。
  // 乐观更新：不阻塞勾选交互；失败时回滚并展示错误。
  async function toggleTool(name: string) {
    if (!neuron) return;
    const next = toolIds.includes(name)
      ? toolIds.filter((x) => x !== name)
      : [...toolIds, name];
    toolIds = next;
    saveError = null;
    try {
      const updated = (await invoke("update_neuron", {
        id: neuron.id,
        toolIds: next,
      })) as Neuron;
      neuron = updated;
      onChanged();
    } catch (e) {
      console.error(e);
      // 保存失败回滚为已保存的工具列表，并在抽屉内展示错误
      saveError = formatInvokeError(e);
      toolIds = neuron.tool_ids ? [...neuron.tool_ids] : [];
    }
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
        <label>{t("neuronPanel.id")}</label>
        <div class="id-row">
          <span class="value mono id-text" title={neuron.id}>{neuron.id}</span>
          <button class="copy-btn" on:click={copyId} title={t("neuronPanel.copy")}>
            {copied ? t("neuronPanel.copied") : t("neuronPanel.copy")}
          </button>
        </div>
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

      <div class="field">
        <label>{t("neuronPanel.toolIds")}</label>
        {#if editing}
          {#if availableTools.length === 0}
            <span class="value muted">{t("neuronPanel.noToolsAvailable")}</span>
          {:else}
            <div class="tool-checks">
              {#each availableTools as tool (tool.name)}
                <label class="tool-check">
                  <input
                    type="checkbox"
                    checked={toolIds.includes(tool.name)}
                    on:change={() => toggleTool(tool.name)}
                  />
                  <span class="tool-name">{tool.name}</span>
                  <span class="tool-desc">{tool.description}</span>
                </label>
              {/each}
            </div>
          {/if}
        {:else}
          {#if neuron.tool_ids && neuron.tool_ids.length}
            <span class="value mono">{neuron.tool_ids.join(", ")}</span>
          {:else}
            <span class="value muted">—</span>
          {/if}
        {/if}
      </div>

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

    {#if saveError}
      <div class="drawer-error">{saveError}</div>
    {/if}

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
  .id-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .id-text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .copy-btn {
    flex-shrink: 0;
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    color: var(--color-text-muted);
    border-radius: 6px;
    padding: 2px 8px;
    font-size: 11px;
    cursor: pointer;
    transition:
      color 0.15s ease,
      border-color 0.15s ease;
  }
  .copy-btn:hover {
    color: var(--color-primary);
    border-color: var(--color-primary);
  }
  .tool-checks {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 160px;
    overflow-y: auto;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 8px;
    background: var(--color-bg);
  }
  .tool-check {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    cursor: pointer;
    font-size: 12px;
    line-height: 1.4;
  }
  .tool-check input {
    margin-top: 2px;
    accent-color: var(--color-primary);
  }
  .tool-name {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--color-text);
    white-space: nowrap;
  }
  .tool-desc {
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
  .drawer-error {
    margin: 0 14px;
    padding: 8px 10px;
    border: 1px solid var(--color-error, #e5484d);
    border-radius: 8px;
    background: color-mix(in srgb, var(--color-error, #e5484d) 10%, transparent);
    color: var(--color-error, #e5484d);
    font-size: 11.5px;
    line-height: 1.4;
    word-break: break-all;
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
