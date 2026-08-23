<script lang="ts">
  import { onMount } from "svelte";
  import { api, c } from "$lib/api";
  import type { Connection, Neuron, SessionBehavior } from "$lib/types";
  import { t } from "$lib/i18n";
  import { formatInvokeError } from "$lib/utils/formatInvokeError";
  import { CopyToClipboard } from "$lib/utils";
  import BehaviorFields from "./BehaviorFields.svelte";
  import { systemTypeColor } from "$lib/features/neuron/systemTypeColor";

  export let neuron: Neuron | null = null;
  export let connections: Connection[] = [];
  export let onClose: () => void = () => {};
  export let onJumpTo: (id: string) => void = () => {};
  export let onChanged: () => void = () => {};
  export let onRequestCreateDownstream: (sourceId: string) => void = () => {};

  // ── 系统类型绑定 + 行为管理 ──
  let bindMode = false;
  let bindTypeInput = "";
  let behaviorDraft: SessionBehavior | null = null;
  let behaviorSaving = false;
  let actionBusy = false;
  type ConfirmAction = { kind: "bind" | "unbind"; type?: string } | null;
  let confirmAction: ConfirmAction = null;

  // 抽屉位置：右侧 / 底部，偏好持久化到 localStorage
  type DrawerPosition = "right" | "bottom";
  const DRAWER_POS_KEY = "neuron-drawer-position";

  function readDrawerPosition(): DrawerPosition {
    try {
      const v = localStorage.getItem(DRAWER_POS_KEY);
      return v === "right" || v === "bottom" ? v : "bottom";
    } catch {
      return "bottom";
    }
  }

  let position: DrawerPosition = readDrawerPosition();

  function togglePosition() {
    position = position === "right" ? "bottom" : "right";
    try {
      localStorage.setItem(DRAWER_POS_KEY, position);
    } catch {
      // 忽略持久化失败
    }
  }

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
    const ok = await CopyToClipboard.copyText(neuron.id);
    if (ok) {
      copied = true;
      clearTimeout(copyTimer);
      copyTimer = setTimeout(() => (copied = false), 1500);
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
    bindMode = false;
    bindTypeInput = "";
    confirmAction = null;
    behaviorDraft = neuron.behavior ?? null;
  }

  // 关闭抽屉时清空记忆，下次打开任意神经元都会重新初始化
  $: if (!neuron) {
    lastNeuronId = null;
    editing = false;
    bindMode = false;
    confirmAction = null;
  }

  // ── 系统类型绑定（换绑/取消需二次确认） ──
  function askConfirm(action: ConfirmAction) {
    confirmAction = action;
  }

  async function runConfirmAction() {
    if (!neuron || !confirmAction) return;
    actionBusy = true;
    try {
      const { kind, type } = confirmAction;
      if (kind === "bind") {
        const updated = (await api.call(c.setNeuronSystemType, {
          id: neuron.id,
          systemType: type ?? null,
        })) as Neuron;
        neuron = { ...neuron, system_type: updated.system_type, behavior: updated.behavior };
        bindMode = false;
        bindTypeInput = "";
      } else {
        const updated = (await api.call(c.setNeuronSystemType, {
          id: neuron.id,
          systemType: null,
        })) as Neuron;
        neuron = { ...neuron, system_type: updated.system_type, behavior: updated.behavior };
      }
      confirmAction = null;
      onChanged();
    } catch (e) {
      console.error(e);
      saveError = formatInvokeError(e);
    } finally {
      actionBusy = false;
    }
  }

  async function saveBehavior() {
    if (!neuron || !behaviorDraft) return;
    behaviorSaving = true;
    saveError = null;
    try {
      const updated = (await api.call(c.updateNeuronBehavior, {
        id: neuron.id,
        behavior: behaviorDraft,
      })) as Neuron;
      neuron = { ...neuron, behavior: updated.behavior };
      onChanged();
    } catch (e) {
      console.error(e);
      saveError = formatInvokeError(e);
    } finally {
      behaviorSaving = false;
    }
  }

  onMount(() => {
    api.call(c.listSkills, undefined)
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
      await api.call(c.updateNeuron, {
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
      const updated = (await api.call(c.updateNeuron, {
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
      const updated = (await api.call(c.adjustNeuronWeight, {
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

  async function adjustEdge(conn: Connection, delta: number) {
    if (weightBusy) return;
    weightBusy = true;
    try {
      const updated = (await api.call(c.adjustEdgeWeight, {
        source: conn.source,
        target: conn.target,
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

<div class="drawer" class:open={!!neuron} class:right={position === "right"} class:bottom={position === "bottom"}>
  {#if neuron}
    <div class="drawer-head">
      <span class="type-bar" style:background={systemTypeColor(neuron.system_type)}></span>
      <span class="title">{t("neuronPanel.drawerTitle")}</span>
      <div class="head-actions" class:editing>
        {#if editing}
          <button
            class="head-btn"
            onclick={handleSave}
            disabled={saving}
            title={saving ? t("neuronPanel.saving") : t("neuronPanel.save")}
            aria-label={t("neuronPanel.save")}
          >
            <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
              <path d="M9 16.17 4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" fill="currentColor"/>
            </svg>
          </button>
          <button
            class="head-btn"
            onclick={handleCancel}
            disabled={saving}
            title={t("neuronPanel.cancel")}
            aria-label={t("neuronPanel.cancel")}
          >
            <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
              <path d="M19 6.41 17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" fill="currentColor"/>
            </svg>
          </button>
        {:else}
          <button
            class="head-btn"
            onclick={() => (editing = true)}
            title={t("neuronPanel.edit")}
            aria-label={t("neuronPanel.edit")}
          >
            <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
              <path d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04c.39-.39.39-1.02 0-1.41l-2.34-2.34a.9959.9959 0 0 0-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z" fill="currentColor"/>
            </svg>
          </button>
          <button
            class="head-btn"
            onclick={() => onRequestCreateDownstream(neuron!.id)}
            title={t("neuronPanel.createDownstreamFromHere")}
            aria-label={t("neuronPanel.createDownstreamFromHere")}
          >
            <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">
              <path d="M14 4l2.29 2.29-2.88 2.88 1.42 1.42 2.88-2.88L20 10V4h-6zm-4 0H4v6l2.29-2.29 4.71 4.7V20h2v-8.41l-5.29-5.3z" fill="currentColor"/>
            </svg>
          </button>
        {/if}
      </div>
      <button
        class="pos-btn"
        onclick={togglePosition}
        title={position === "bottom" ? t("neuronPanel.posRight") : t("neuronPanel.posBottom")}
        aria-label={position === "bottom" ? t("neuronPanel.posRight") : t("neuronPanel.posBottom")}
      >
        {#if position === "bottom"}
          <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
            <rect x="1.5" y="1.5" width="13" height="13" rx="2" fill="none" stroke="currentColor" stroke-width="1.5"/>
            <line x1="11" y1="1.5" x2="11" y2="14.5" stroke="currentColor" stroke-width="1.5"/>
          </svg>
        {:else}
          <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
            <rect x="1.5" y="1.5" width="13" height="13" rx="2" fill="none" stroke="currentColor" stroke-width="1.5"/>
            <line x1="1.5" y1="11" x2="14.5" y2="11" stroke="currentColor" stroke-width="1.5"/>
          </svg>
        {/if}
      </button>
      <button
        class="close"
        onclick={onClose}
        title={t("neuronPanel.close")}
        aria-label={t("neuronPanel.close")}
      >
        <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
          <line x1="4" y1="4" x2="12" y2="12" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>
          <line x1="12" y1="4" x2="4" y2="12" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>
        </svg>
      </button>
    </div>

    <div class="drawer-body">
      <div class="field">
        <span class="field-label">{t("neuronEditor.systemType")}</span>
        {#if neuron.system_type}
          <div class="system-type-row">
            <span class="type-badge" style:background={systemTypeColor(neuron.system_type)}>
              {neuron.system_type}
            </span>
            <button class="btn small" onclick={() => (bindMode = true)}>
              {t("neuronEditor.rebind")}
            </button>
            <button
              class="btn small danger"
              onclick={() => askConfirm({ kind: "unbind" })}
            >
              {t("neuronEditor.unbind")}
            </button>
          </div>
        {:else}
          <div class="system-type-row">
            <span class="value muted">{t("neuronEditor.systemTypeUnbound")}</span>
            <button class="btn small" onclick={() => (bindMode = true)}>
              {t("neuronEditor.bind")}
            </button>
          </div>
        {/if}
        {#if bindMode}
          <div class="bind-row">
            <input
              bind:value={bindTypeInput}
              placeholder={t("neuronEditor.bindPlaceholder")}
              onkeydown={(e) => {
                if (e.key === "Enter" && bindTypeInput.trim() && !actionBusy)
                  askConfirm({ kind: "bind", type: bindTypeInput.trim() });
              }}
            />
            <button
              class="btn small primary"
              disabled={actionBusy || !bindTypeInput.trim()}
              onclick={() => askConfirm({ kind: "bind", type: bindTypeInput.trim() })}
            >
              {t("neuronEditor.confirm")}
            </button>
            <button
              class="btn small"
              onclick={() => {
                bindMode = false;
                bindTypeInput = "";
              }}
            >
              {t("neuronEditor.cancel")}
            </button>
          </div>
        {/if}
      </div>
      <div class="field">
        <span class="field-label">{t("neuronPanel.id")}</span>
        <div class="id-row">
          <span class="value mono id-text" title={neuron.id}>{neuron.id}</span>
          <button class="copy-btn" onclick={copyId} title={t("neuronPanel.copy")}>
            {copied ? t("neuronPanel.copied") : t("neuronPanel.copy")}
          </button>
        </div>
      </div>
      <div class="field">
        <span class="field-label">{t("neuronPanel.weight")}</span>
        <div class="stepper">
          <button class="step-btn" onclick={() => adjustNeuron(-WEIGHT_STEP)} disabled={weightBusy}>−</button>
          <span class="value mono step-val">{neuron.weight.toFixed(4)}</span>
          <button class="step-btn" onclick={() => adjustNeuron(WEIGHT_STEP)} disabled={weightBusy}>＋</button>
        </div>
        <div class="delta-row">
          <label class="delta-label" for="neuron-delta-input">{t("neuronPanel.delta")}</label>
          <input
            id="neuron-delta-input"
            class="delta-input"
            type="number"
            step="0.05"
            bind:value={weightDelta}
            onkeydown={(e) => { if (e.key === "Enter") adjustNeuron(weightDelta); }}
          />
          <button class="btn small primary" onclick={() => adjustNeuron(weightDelta)} disabled={weightBusy}>
            {t("neuronPanel.apply")}
          </button>
        </div>
      </div>

      <div class="field">
        <label for="neuron-desc">{t("neuronPanel.description")}</label>
        {#if editing}
          <textarea id="neuron-desc" bind:value={desc} rows="2"></textarea>
        {:else}
          <span class="value">{neuron.desc || "—"}</span>
        {/if}
      </div>

      <div class="field">
        <label for="neuron-content">{t("neuronPanel.content")}</label>
        {#if editing}
          <textarea id="neuron-content" bind:value={content} rows="6"></textarea>
        {:else}
          <span class="value pre">{neuron.content || "—"}</span>
        {/if}
      </div>

      <div class="field">
        <span class="field-label">{t("neuronPanel.toolIds")}</span>
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
                    onchange={() => toggleTool(tool.name)}
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
        <span class="field-label">{t("neuronPanel.createdAt")}</span>
        <span class="value">{fmtTime(neuron.created_at)}</span>
      </div>
      <div class="field">
        <span class="field-label">{t("neuronPanel.updatedAt")}</span>
        <span class="value">{fmtTime(neuron.updated_at)}</span>
      </div>

      {#if neuron.system_type}
        <div class="field col behavior-block">
          <span class="field-label">{t("neuronEditor.behavior")}</span>
          <BehaviorFields
            value={neuron.behavior ?? null}
            onChange={(b) => (behaviorDraft = b)}
          />
          <button
            class="btn small primary behavior-save"
            disabled={behaviorSaving}
            onclick={() => void saveBehavior()}
          >
            {behaviorSaving ? t("neuronPanel.saving") : t("neuronEditor.saveBehavior")}
          </button>
        </div>
      {/if}

      <div class="field col">
        <span class="field-label">{t("neuronPanel.connections")} ({connections.length})</span>
        {#if connections.length === 0}
          <span class="value muted">—</span>
        {:else}
          <ul class="conns">
            {#each connections as c (c.source + "->" + c.target)}
              {@const selfId = neuron!.id}
              <li>
                <button class="conn-link" onclick={() => onJumpTo(c.source === selfId ? c.target : c.source)}>
                  {c.source === selfId ? c.target : c.source}
                </button>
                <div class="stepper small">
                  <button class="step-btn" onclick={() => adjustEdge(c, -WEIGHT_STEP)} disabled={weightBusy}>−</button>
                  <span class="conn-w">w{c.weight.toFixed(2)}</span>
                  <button class="step-btn" onclick={() => adjustEdge(c, WEIGHT_STEP)} disabled={weightBusy}>＋</button>
                  <input
                    class="delta-input small"
                    type="number"
                    step="0.05"
                    bind:value={edgeDelta}
                    onkeydown={(e) => { if (e.key === "Enter") adjustEdge(c, edgeDelta); }}
                  />
                  <button class="btn small primary" onclick={() => adjustEdge(c, edgeDelta)} disabled={weightBusy}>
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

    {#if confirmAction}
      <div
        class="confirm-mask"
        role="presentation"
        onclick={(e) => {
          if (e.target === e.currentTarget) confirmAction = null;
        }}
      >
        <div class="confirm-box">
          <div class="confirm-title">
            {confirmAction.kind === "bind"
              ? t("neuronEditor.bindConfirmTitle")
              : t("neuronEditor.unbindConfirmTitle")}
          </div>
          <div class="confirm-body">
            {confirmAction.kind === "bind"
              ? t("neuronEditor.bindConfirmBody", { type: confirmAction.type ?? "" })
              : t("neuronEditor.unbindConfirmBody")}
          </div>
          <div class="confirm-actions">
            <button class="btn" onclick={() => (confirmAction = null)} disabled={actionBusy}>
              {t("neuronEditor.cancel")}
            </button>
            <button
              class="btn primary"
              disabled={actionBusy}
              onclick={() => void runConfirmAction()}
            >
              {actionBusy ? t("neuronPanel.saving") : t("neuronEditor.confirm")}
            </button>
          </div>
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .drawer {
    position: absolute;
    background: var(--color-surface);
    display: flex;
    flex-direction: column;
    opacity: 0;
    pointer-events: none;
    transition:
      transform 0.22s ease,
      opacity 0.22s ease;
    z-index: 20;
  }
  .drawer.open {
    opacity: 1;
    pointer-events: auto;
  }
  .drawer.bottom {
    bottom: 0;
    left: 0;
    width: 100%;
    height: 50%;
    border-top: 1px solid var(--color-border);
    box-shadow: 0 -8px 24px rgba(0, 0, 0, 0.12);
    transform: translateY(100%);
  }
  .drawer.bottom.open {
    transform: translateY(0);
  }
  .drawer.right {
    top: 0;
    right: 0;
    width: 320px;
    height: 100%;
    border-left: 1px solid var(--color-border);
    box-shadow: -8px 0 24px rgba(0, 0, 0, 0.12);
    transform: translateX(100%);
  }
  .drawer.right.open {
    transform: translateX(0);
  }

  .drawer-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--color-border);
  }
  .title {
    flex: 1;
    font-weight: 600;
    font-size: var(--fs-sm);
    color: var(--color-text);
  }
  .pos-btn {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    padding: 0;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    cursor: pointer;
    transition:
      color 0.15s ease,
      background 0.15s ease;
  }
  .head-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-left: auto;
  }
  .head-actions .head-btn {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    padding: 0;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    cursor: pointer;
    opacity: 0;
    transition:
      color 0.15s ease,
      background 0.15s ease,
      opacity 0.15s ease;
  }
  /* 编辑态操作常显；查看态编辑/发起仅在悬停标题栏或键盘聚焦时浮现 */
  .head-actions.editing .head-btn {
    opacity: 1;
  }
  .drawer-head:hover .head-actions .head-btn,
  .drawer-head:focus-within .head-actions .head-btn {
    opacity: 1;
  }
  .head-actions .head-btn:hover {
    color: var(--color-text);
    background: var(--color-hover);
  }
  .head-actions .head-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .head-actions .head-btn svg {
    display: block;
  }
  /* 触屏（hover: none）常显，保证可发现性 */
  @media (hover: none) {
    .head-actions .head-btn {
      opacity: 1;
    }
  }
  .pos-btn:hover {
    color: var(--color-text);
    background: var(--color-hover);
  }
  .pos-btn svg {
    display: block;
  }
  .close {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    padding: 0;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    cursor: pointer;
    transition:
      color 0.15s ease,
      background 0.15s ease;
  }
  .close:hover {
    color: var(--color-text);
    background: var(--color-hover);
  }
  .close svg {
    display: block;
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
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .field-label {
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .value {
    font-size: var(--fs-sm);
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
    font-size: var(--fs-xs);
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
    border: var(--border-width) solid var(--color-border);
    background: var(--color-bg);
    color: var(--color-text-muted);
    border-radius: var(--radius-sm);
    padding: 2px 8px;
    font-size: var(--fs-xs);
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
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    padding: 8px;
    background: var(--color-bg);
  }
  .tool-check {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    cursor: pointer;
    font-size: var(--fs-sm);
    line-height: 1.4;
  }
  .tool-check input {
    margin-top: 2px;
    accent-color: var(--color-primary);
  }
  .tool-name {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
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
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    padding: 6px 8px;
    font-size: var(--fs-sm);
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
    font-size: var(--fs-sm);
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
    font-size: var(--fs-xs);
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
    font-size: var(--fs-sm);
    color: var(--color-text-muted);
  }
  .delta-input {
    width: 72px;
    padding: 4px 6px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg);
    color: var(--color-text);
    font-size: var(--fs-sm);
  }
  .delta-input.small {
    width: 56px;
  }
  .btn.small {
    flex: 0 0 auto;
    height: 26px;
    padding: 0 10px;
    font-size: var(--fs-sm);
  }
  .btn.primary {
    border: 1px solid var(--color-primary);
    background: var(--color-primary);
    color: var(--color-on-primary);
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
    font-size: var(--fs-sm);
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
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }

  .drawer-error {
    margin: 0 14px;
    padding: 8px 10px;
    border: 1px solid var(--color-error, #e5484d);
    border-radius: 8px;
    background: color-mix(in srgb, var(--color-error, #e5484d) 10%, transparent);
    color: var(--color-error, #e5484d);
    font-size: var(--fs-xs);
    line-height: 1.4;
    word-break: break-all;
  }
  .btn {
    flex: 1;
    padding: 7px 10px;
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: var(--fs-sm);
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

  .system-type-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .type-badge {
    font-size: var(--fs-xs);
    font-weight: 600;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    color: var(--color-on-primary, #fff);
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .btn.danger {
    color: var(--color-error, #e5484d);
    border-color: var(--color-error, #e5484d);
    background: transparent;
  }
  .btn.danger:hover {
    background: color-mix(in srgb, var(--color-error, #e5484d) 10%, transparent);
  }
  .bind-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .bind-row input {
    flex: 1;
    min-width: 0;
    background: var(--color-bg);
    color: var(--color-text);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    padding: 4px 8px;
    font-size: var(--fs-sm);
    font-family: var(--font-mono);
  }
  .bind-row input:focus {
    outline: none;
    border-color: var(--color-primary);
  }
  .behavior-block {
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    padding: 8px 10px;
    background: var(--color-bg);
  }
  .behavior-save {
    align-self: flex-start;
  }
  .confirm-mask {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--color-bg) 55%, transparent);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 30;
  }
  .confirm-box {
    width: min(300px, 90%);
    background: var(--color-surface);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-lg);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.25);
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .confirm-title {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--color-text);
  }
  .confirm-body {
    font-size: var(--fs-sm);
    line-height: 1.5;
    color: var(--color-text-muted);
    word-break: break-word;
  }
  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .confirm-actions .btn {
    flex: 0 0 auto;
    min-width: 72px;
  }
</style>
