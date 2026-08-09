<script lang="ts">
  import { t } from "$lib/i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { useViewContext } from "$lib/layout/viewContext";
  import { errorMessage } from "$lib/errorMessage";
  import type {
    Neuron,
    NeighborhoodPoolPolicy,
    SelectionPolicy,
    SessionBehavior,
    SystemPromptStatus,
    ToolPolicy,
  } from "$lib/types";

  const ctx = useViewContext();
  const data = ctx.stores.data;

  // 邻域池默认配额（对齐后端 NeighborhoodPoolPolicy::default）。
  const DEFAULT_POLICY: NeighborhoodPoolPolicy = {
    existing_downstream: 4,
    new_downstream: 2,
    fill_downstream_shortage: true,
    siblings: 2,
    upstream_depth: 3,
    global_top_weight: 5,
  };

  let specs = $derived(data.state.sessionSpecs);
  let errorMsg = $state("");

  // ── 新建表单 ──
  let showCreate = $state(false);
  let newSystemType = $state("");
  let newContent = $state("");
  let newForm = $state(emptyBehaviorForm());

  // ── 编辑表单 ──
  let editingId = $state<string | null>(null);
  let editContent = $state("");
  let editForm = $state(emptyBehaviorForm());

  // content 摘要懒加载（已绑定规格按 neuron_id 拉取，失败静默）。
  let contents = $state<Record<string, string>>({});
  const requestedIds = new Set<string>();
  $effect(() => {
    for (const s of specs) {
      const id = s.neuron_id;
      if (id && contents[id] == null && !requestedIds.has(id)) {
        requestedIds.add(id);
        void invoke<Neuron>("get_neuron", { id })
          .then((n) => {
            contents[id] = n.content;
          })
          .catch(() => {
            /* ignore */
          });
      }
    }
  });

  type BehaviorFormState = {
    selection: "none" | "fixed" | "neighborhood" | "global";
    globalLimit: number;
    tools: "none" | "from_neuron" | "allowlist";
    allowlistText: string;
    insertId: string;
  };

  function emptyBehaviorForm(): BehaviorFormState {
    return {
      selection: "none",
      globalLimit: 7,
      tools: "none",
      allowlistText: "",
      insertId: "",
    };
  }

  function behaviorFromForm(f: BehaviorFormState): SessionBehavior {
    const selection: SelectionPolicy =
      f.selection === "fixed"
        ? "Fixed"
        : f.selection === "neighborhood"
          ? { Neighborhood: { policy: DEFAULT_POLICY } }
          : f.selection === "global"
            ? { Global: { limit: f.globalLimit || 7 } }
            : "None";
    const tools: ToolPolicy =
      f.tools === "from_neuron"
        ? "FromNeuron"
        : f.tools === "allowlist"
          ? {
              Allowlist: f.allowlistText
                .split(",")
                .map((s) => s.trim())
                .filter(Boolean),
            }
          : "None";
    const behavior: SessionBehavior = { selection, tools };
    if (f.insertId.trim()) behavior.insert_id = f.insertId.trim();
    return behavior;
  }

  function formFromBehavior(b?: SessionBehavior | null): BehaviorFormState {
    const f = emptyBehaviorForm();
    if (!b) return f;
    const sel = b.selection;
    if (sel === "Fixed") f.selection = "fixed";
    else if (sel !== "None" && typeof sel === "object" && "Global" in sel) {
      f.selection = "global";
      f.globalLimit = sel.Global.limit;
    } else if (sel !== "None" && typeof sel === "object" && "Neighborhood" in sel) {
      f.selection = "neighborhood";
    } else {
      f.selection = "none";
    }
    const tools = b.tools;
    if (tools === "FromNeuron") f.tools = "from_neuron";
    else if (tools !== "None" && typeof tools === "object" && "Allowlist" in tools) {
      f.tools = "allowlist";
      f.allowlistText = tools.Allowlist.join(", ");
    } else {
      f.tools = "none";
    }
    f.insertId = b.insert_id ?? "";
    return f;
  }

  function selectionLabel(sel: SelectionPolicy): string {
    if (sel === "Fixed") return t("sessionSpecsPanel.fixed");
    if (sel === "None") return t("sessionSpecsPanel.none");
    if (typeof sel === "object") {
      if ("Global" in sel) return `${t("sessionSpecsPanel.global")}(${sel.Global.limit})`;
      if ("Neighborhood" in sel) return t("sessionSpecsPanel.neighborhood");
    }
    return String(sel);
  }

  function toolsLabel(tp: ToolPolicy): string {
    if (tp === "FromNeuron") return t("sessionSpecsPanel.toolFromNeuron");
    if (tp === "None") return t("sessionSpecsPanel.toolNone");
    if (typeof tp === "object" && "Allowlist" in tp) {
      return tp.Allowlist.join(", ") || t("sessionSpecsPanel.toolAllowlist");
    }
    return String(tp);
  }

  function contentPreview(spec: SystemPromptStatus): string {
    const id = spec.neuron_id;
    if (!id) return t("sessionSpecsPanel.unbound");
    const content = contents[id];
    if (content == null) return "…";
    const trimmed = content.trim();
    if (!trimmed) return t("sessionSpecsPanel.unbound");
    return trimmed.length > 80 ? `${trimmed.slice(0, 80)}…` : trimmed;
  }

  async function handleCreate() {
    errorMsg = "";
    if (!newSystemType.trim().startsWith("session.")) {
      errorMsg = t("sessionSpecsPanel.systemTypeHint");
      return;
    }
    if (newForm.selection === "fixed" && !newContent.trim()) {
      errorMsg = t("sessionSpecsPanel.contentRequired");
      return;
    }
    try {
      await data.createSessionSpec(
        newSystemType.trim(),
        newContent.trim() || null,
        behaviorFromForm(newForm),
      );
      showCreate = false;
      newSystemType = "";
      newContent = "";
      newForm = emptyBehaviorForm();
    } catch (e) {
      errorMsg = `${t("sessionSpecsPanel.operationFailed")}: ${errorMessage(e)}`;
    }
  }

  function startEdit(spec: SystemPromptStatus) {
    errorMsg = "";
    editingId = spec.neuron_id ?? null;
    editContent = spec.neuron_id ? (contents[spec.neuron_id] ?? "") : "";
    editForm = formFromBehavior(spec.behavior);
  }

  async function handleSaveEdit(spec: SystemPromptStatus) {
    if (!spec.neuron_id) return;
    errorMsg = "";
    try {
      const id = spec.neuron_id;
      if (editContent !== (contents[id] ?? "")) {
        const updated = await invoke<Neuron>("update_neuron", {
          id,
          desc: null,
          content: editContent,
        });
        contents[id] = updated.content;
      }
      await data.updateSessionSpecBehavior(id, behaviorFromForm(editForm));
      editingId = null;
    } catch (e) {
      errorMsg = `${t("sessionSpecsPanel.operationFailed")}: ${errorMessage(e)}`;
    }
  }

  async function handleLaunch(spec: SystemPromptStatus) {
    if (!spec.neuron_id) return;
    errorMsg = "";
    try {
      await data.openSession(spec.neuron_id, "assistant");
      ctx.stores.layout.insertPanel("chat");
    } catch (e) {
      errorMsg = `${t("sessionSpecsPanel.operationFailed")}: ${errorMessage(e)}`;
    }
  }
</script>

<div class="specs-panel">
  <div class="panel-header">
    <h2 class="panel-title">{t("sessionSpecsPanel.title")}</h2>
    <button class="btn btn-primary" onclick={() => (showCreate = !showCreate)}>
      {t("sessionSpecsPanel.newButton")}
    </button>
  </div>

  {#if errorMsg}
    <p class="error">{errorMsg}</p>
  {/if}

  {#if showCreate}
    <div class="card form-card">
      <label class="field">
        <span>{t("sessionSpecsPanel.systemType")}</span>
        <input
          bind:value={newSystemType}
          placeholder={t("sessionSpecsPanel.systemTypeHint")}
        />
      </label>
      <label class="field">
        <span>{t("sessionSpecsPanel.content")}</span>
        <textarea bind:value={newContent} rows="4" placeholder={t("sessionSpecsPanel.contentHint")} />
      </label>
      {@render BehaviorFields(newForm)}
      <div class="form-actions">
        <button class="btn btn-primary" onclick={handleCreate}>{t("sessionSpecsPanel.create")}</button>
        <button class="btn" onclick={() => (showCreate = false)}>{t("sessionSpecsPanel.cancel")}</button>
      </div>
    </div>
  {/if}

  {#if specs.length === 0}
    <p class="empty">{t("sessionSpecsPanel.empty")}</p>
  {:else}
    <div class="list">
      {#each specs as spec (spec.system_type)}
        <div class="item">
          <div class="item-title">
            <code>{spec.system_type}</code>
            <span class="tag" class:unbound={!spec.neuron_id}>
              {spec.neuron_id ? t("sessionSpecsPanel.bound") : t("sessionSpecsPanel.unbound")}
            </span>
          </div>
          <div class="item-detail">{contentPreview(spec)}</div>
          <div class="item-detail">
            {t("sessionSpecsPanel.selection")}: {selectionLabel(spec.behavior?.selection ?? "None")}
            &middot; {t("sessionSpecsPanel.tools")}: {toolsLabel(spec.behavior?.tools ?? "None")}
            {#if spec.behavior?.insert_id}
              &middot; {spec.behavior.insert_id}
            {/if}
          </div>
          <div class="item-actions">
            <button class="btn" onclick={() => startEdit(spec)}>{t("sessionSpecsPanel.edit")}</button>
            <button
              class="btn btn-primary"
              disabled={!spec.neuron_id}
              title={t("sessionSpecsPanel.launchHint")}
              onclick={() => handleLaunch(spec)}
            >
              {t("sessionSpecsPanel.launch")}
            </button>
          </div>
          {#if editingId === spec.neuron_id}
            <div class="card edit-card">
              <label class="field">
                <span>{t("sessionSpecsPanel.content")}</span>
                <textarea bind:value={editContent} rows="4" placeholder={t("sessionSpecsPanel.contentHint")} />
              </label>
              {@render BehaviorFields(editForm)}
              <div class="form-actions">
                <button class="btn btn-primary" onclick={() => handleSaveEdit(spec)}>
                  {t("sessionSpecsPanel.save")}
                </button>
                <button class="btn" onclick={() => (editingId = null)}>
                  {t("sessionSpecsPanel.cancel")}
                </button>
              </div>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<!-- 选型策略 + 工具策略 + 契约段 id 表单化控件（新建/编辑共用）。 -->
{#snippet BehaviorFields(form: BehaviorFormState)}
  <div class="form-grid">
    <label class="field">
      <span>{t("sessionSpecsPanel.selection")}</span>
      <select bind:value={form.selection}>
        <option value="none">{t("sessionSpecsPanel.none")}</option>
        <option value="fixed">{t("sessionSpecsPanel.fixed")}</option>
        <option value="neighborhood">{t("sessionSpecsPanel.neighborhood")}</option>
        <option value="global">{t("sessionSpecsPanel.global")}</option>
      </select>
    </label>
    {#if form.selection === "global"}
      <label class="field">
        <span>{t("sessionSpecsPanel.globalLimit")}</span>
        <input type="number" min="1" max="20" bind:value={form.globalLimit} />
      </label>
    {/if}
    <label class="field">
      <span>{t("sessionSpecsPanel.tools")}</span>
      <select bind:value={form.tools}>
        <option value="none">{t("sessionSpecsPanel.toolNone")}</option>
        <option value="from_neuron">{t("sessionSpecsPanel.toolFromNeuron")}</option>
        <option value="allowlist">{t("sessionSpecsPanel.toolAllowlist")}</option>
      </select>
    </label>
    {#if form.tools === "allowlist"}
      <label class="field">
        <span>{t("sessionSpecsPanel.allowlistHint")}</span>
        <input bind:value={form.allowlistText} placeholder={t("sessionSpecsPanel.allowlistHint")} />
      </label>
    {/if}
    <label class="field">
      <span>{t("sessionSpecsPanel.insertId")}</span>
      <input bind:value={form.insertId} placeholder="assistant.match_topic" />
    </label>
  </div>
{/snippet}

<style>
  .specs-panel {
    height: 100%;
    overflow-y: auto;
    padding: var(--space-2);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .panel-title {
    font-size: var(--fs-md);
    font-weight: 600;
    margin: 0;
  }
  .empty {
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--fs-sm);
    padding: var(--space-6) var(--space-2);
  }
  .error {
    font-size: var(--fs-sm);
    color: var(--color-danger, #e5484d);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .item {
    padding: var(--space-2);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    border: var(--border-width) solid var(--color-border);
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .item-title {
    font-size: var(--fs-sm);
    font-weight: 600;
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
  .tag {
    font-size: 10px;
    font-weight: 600;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    background: var(--color-primary);
    color: var(--color-on-primary);
    opacity: 0.85;
  }
  .tag.unbound {
    background: var(--color-text-muted);
  }
  .item-detail {
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    line-height: 1.5;
    word-break: break-all;
  }
  .item-actions {
    display: flex;
    gap: var(--space-1);
  }
  .form-card,
  .edit-card {
    padding: var(--space-2);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    border: var(--border-width) solid var(--color-border);
  }
  .form-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: var(--space-2);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-size: var(--fs-sm);
  }
  .field input,
  .field select,
  .field textarea {
    padding: var(--space-1);
    border-radius: var(--radius-sm);
    border: var(--border-width) solid var(--color-border);
    background: var(--color-bg);
    color: var(--color-text);
    font-size: var(--fs-sm);
  }
  .form-actions {
    display: flex;
    gap: var(--space-1);
    margin-top: var(--space-2);
  }
</style>
