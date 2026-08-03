<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Topic, TopicStatus } from "$lib/types";
  import { t } from "$lib/i18n";
  import { errorMessage } from "$lib/errorMessage";

  let {
    topics = $bindable([]),
  }: { topics: Topic[] } = $props();

  // ── State ──
  let filterStatus: TopicStatus | "" = $state("");
  let expandedId = $state<string | null>(null);
  let showCreateForm = $state(false);
  let createName = $state("");
  let createDesc = $state("");
  let creating = $state(false);
  let deleteConfirmId = $state<string | null>(null);
  let errorMsg = $state("");

  // ── New scope item form ──
  let newScopeGoal = $state("");
  let newScopeContract = $state("");
  let addingScope = $state(false);

  // ── Derived ──
  let filteredTopics = $derived(
    filterStatus
      ? topics.filter((t) => t.status === filterStatus)
      : topics
  );

  const statusFilters: { value: TopicStatus | ""; label: string }[] = [
    { value: "", label: t("topicPanel.all") },
    { value: "todo", label: "Todo" },
    { value: "in_progress", label: "In Progress" },
    { value: "paused", label: "Paused" },
    { value: "done", label: "Done" },
    { value: "cancelled", label: "Cancelled" },
  ];

  const statusColors: Record<TopicStatus, string> = {
    todo: "var(--color-text-muted)",
    in_progress: "var(--color-primary)",
    paused: "var(--color-warning, #f59e0b)",
    done: "var(--color-success, #22c55e)",
    cancelled: "var(--color-danger, #ef4444)",
  };

  const statusLabels: Record<TopicStatus, string> = {
    todo: "Todo",
    in_progress: "In Progress",
    paused: "Paused",
    done: "Done",
    cancelled: "Cancelled",
  };

  function formatTime(ts: number): string {
    const d = new Date(ts);
    return d.toLocaleString();
  }

  // ── Actions ──

  async function handleCreate() {
    if (!createName.trim()) return;
    creating = true;
    errorMsg = "";
    try {
      const topic = await invoke<Topic>("create_topic", {
        name: createName.trim(),
        description: createDesc.trim(),
      });
      topics = [...topics, topic];
      createName = "";
      createDesc = "";
      showCreateForm = false;
      expandedId = topic.id;
    } catch (e) {
      errorMsg = `Create failed: ${errorMessage(e)}`;
    } finally {
      creating = false;
    }
  }

  async function handlePause(id: string) {
    errorMsg = "";
    try {
      const updated = await invoke<Topic>("pause_topic", { id });
      topics = topics.map((t) => (t.id === id ? updated : t));
    } catch (e) {
      errorMsg = `Pause failed: ${errorMessage(e)}`;
    }
  }

  async function handleResume(id: string) {
    errorMsg = "";
    try {
      const updated = await invoke<Topic>("resume_topic", { id });
      topics = topics.map((t) => (t.id === id ? updated : t));
    } catch (e) {
      errorMsg = `Resume failed: ${errorMessage(e)}`;
    }
  }

  async function handleDelete(id: string) {
    errorMsg = "";
    deleteConfirmId = null;
    try {
      await invoke<boolean>("delete_topic", { id });
      topics = topics.filter((t) => t.id !== id);
      if (expandedId === id) expandedId = null;
    } catch (e) {
      errorMsg = `Delete failed: ${errorMessage(e)}`;
    }
  }

  async function handleAddScopeItem(topicId: string) {
    if (!newScopeGoal.trim() || !newScopeContract.trim()) return;
    addingScope = true;
    errorMsg = "";
    try {
      const updated = await invoke<Topic>("add_topic_scope_item", {
        topicId,
        goal: newScopeGoal.trim(),
        doneContract: newScopeContract.trim(),
      });
      topics = topics.map((t) => (t.id === topicId ? updated : t));
      newScopeGoal = "";
      newScopeContract = "";
    } catch (e) {
      errorMsg = `Add scope item failed: ${errorMessage(e)}`;
    } finally {
      addingScope = false;
    }
  }

  async function handleCompleteScopeItem(topicId: string, itemId: string) {
    errorMsg = "";
    try {
      const updated = await invoke<Topic>("complete_topic_scope_item", {
        topicId,
        itemId,
      });
      topics = topics.map((t) => (t.id === topicId ? updated : t));
    } catch (e) {
      errorMsg = `Complete scope item failed: ${errorMessage(e)}`;
    }
  }

  async function handleDeleteScopeItem(topicId: string, itemId: string) {
    errorMsg = "";
    try {
      const updated = await invoke<Topic>("delete_topic_scope_item", {
        topicId,
        itemId,
      });
      topics = topics.map((t) => (t.id === topicId ? updated : t));
    } catch (e) {
      errorMsg = `Delete scope item failed: ${errorMessage(e)}`;
    }
  }
</script>

<div class="topic-panel">
  {#if errorMsg}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="error-banner" onclick={() => (errorMsg = "")}>{errorMsg}</div>
  {/if}

  <!-- Status filters -->
  <div class="filter-bar">
    {#each statusFilters as sf}
      <button
        class="filter-btn"
        class:active={filterStatus === sf.value}
        onclick={() => (filterStatus = sf.value)}
      >
        {sf.label}
      </button>
    {/each}
  </div>

  <!-- Create button -->
  <div class="toolbar">
    <button class="btn btn-primary" onclick={() => (showCreateForm = !showCreateForm)}>
      + {t("topicPanel.create")}
    </button>
  </div>

  <!-- Create form -->
  {#if showCreateForm}
    <div class="create-form">
      <input
        type="text"
        placeholder={t("topicPanel.createName")}
        bind:value={createName}
        disabled={creating}
      />
      <input
        type="text"
        placeholder={t("topicPanel.createDesc")}
        bind:value={createDesc}
        disabled={creating}
      />
      <button class="btn btn-primary" onclick={handleCreate} disabled={creating || !createName.trim()}>
        {creating ? t("topicPanel.creating") : t("topicPanel.createSubmit")}
      </button>
    </div>
  {/if}

  <!-- Topic list -->
  {#if filteredTopics.length === 0}
    <p class="empty">{t("topicPanel.noTopics")}</p>
  {:else}
    <div class="topic-list">
      {#each filteredTopics as topic (topic.id)}
        <div class="topic-card" class:expanded={expandedId === topic.id}>
          <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
          <div class="topic-summary" onclick={() => (expandedId = expandedId === topic.id ? null : topic.id)}>
            <div class="topic-header">
              <span class="topic-name">{topic.name}</span>
              <span class="status-badge" style="background: {statusColors[topic.status]}">
                {statusLabels[topic.status]}
              </span>
            </div>
            <div class="progress-row">
              <div class="progress-bar-bg">
                <div class="progress-bar-fill" style="width: {Math.round(topic.progress)}%"></div>
              </div>
              <span class="progress-text">{Math.round(topic.progress)}%</span>
            </div>
            <div class="topic-meta">
              {t("topicPanel.updated")}: {formatTime(topic.updated_at)}
            </div>
          </div>

          {#if expandedId === topic.id}
            <div class="topic-detail">
              {#if topic.description}
                <div class="detail-row">
                  <span class="detail-label">{t("topicPanel.description")}</span>
                  <span>{topic.description}</span>
                </div>
              {/if}

              {#if topic.session_id}
                <div class="detail-row">
                  <span class="detail-label">{t("topicPanel.sessionId")}</span>
                  <span class="mono">{topic.session_id.slice(0, 12)}...</span>
                </div>
              {/if}

              <!-- Scope Items -->
              <div class="scope-section">
                <div class="scope-header">
                  <span class="detail-label">{t("topicPanel.scopeItems")}</span>
                  <span>({topic.scope_in.filter((s) => s.status === "completed").length}/{topic.scope_in.length})</span>
                </div>

                {#if topic.status !== "paused"}
                  <div class="scope-add-form">
                    <input
                      type="text"
                      placeholder={t("topicPanel.scopeGoal")}
                      bind:value={newScopeGoal}
                      disabled={addingScope}
                    />
                    <input
                      type="text"
                      placeholder={t("topicPanel.scopeContract")}
                      bind:value={newScopeContract}
                      disabled={addingScope}
                    />
                    <button
                      class="btn btn-sm"
                      onclick={() => handleAddScopeItem(topic.id)}
                      disabled={addingScope || !newScopeGoal.trim() || !newScopeContract.trim()}
                    >
                      {t("topicPanel.scopeAdd")}
                    </button>
                  </div>
                {/if}

                <div class="scope-list">
                  {#each topic.scope_in as item (item.id)}
                    <div class="scope-item" class:done={item.status === "completed"}>
                      <div class="scope-item-text">
                        <div class="scope-goal">{item.goal}</div>
                        <div class="scope-contract">{item.done_contract}</div>
                      </div>
                      <div class="scope-item-actions">
                        {#if item.status !== "completed"}
                          <button
                            class="btn btn-sm btn-done"
                            onclick={() => handleCompleteScopeItem(topic.id, item.id)}
                            disabled={topic.status === "paused"}
                            title={t("topicPanel.scopeStatusDone")}
                          >
                            ✓
                          </button>
                        {:else}
                          <span class="scope-done-badge">{t("topicPanel.scopeStatusDone")}</span>
                        {/if}
                        <button
                          class="btn btn-sm btn-danger"
                          onclick={() => handleDeleteScopeItem(topic.id, item.id)}
                          title={t("topicPanel.confirm")}
                        >
                          ×
                        </button>
                      </div>
                    </div>
                  {/each}
                </div>
              </div>

              <!-- Actions -->
              <div class="topic-actions">
                {#if topic.status === "paused"}
                  <button class="btn btn-sm" onclick={() => handleResume(topic.id)}>
                    {t("topicPanel.resume")}
                  </button>
                {:else if topic.status === "todo" || topic.status === "in_progress"}
                  <button class="btn btn-sm" onclick={() => handlePause(topic.id)}>
                    {t("topicPanel.pause")}
                  </button>
                {/if}
                {#if deleteConfirmId === topic.id}
                  <span class="delete-confirm">
                    {t("topicPanel.deleteConfirm")}
                    <button class="btn btn-sm btn-danger" onclick={() => handleDelete(topic.id)}>
                      {t("topicPanel.confirm")}
                    </button>
                    <button class="btn btn-sm" onclick={() => (deleteConfirmId = null)}>
                      {t("topicPanel.cancel")}
                    </button>
                  </span>
                {:else}
                  <button class="btn btn-sm btn-danger" onclick={() => (deleteConfirmId = topic.id)}>
                    {t("topicPanel.confirm")}
                  </button>
                {/if}
              </div>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .topic-panel { display: flex; flex-direction: column; gap: var(--space-2); height: 100%; overflow-y: auto; }
  .error-banner { background: var(--color-danger, #ef4444); color: #fff; padding: var(--space-1) var(--space-2); border-radius: var(--radius-md); font-size: var(--fs-xs); cursor: pointer; }
  .filter-bar { display: flex; flex-wrap: wrap; gap: var(--space-1); }
  .filter-btn { font-size: var(--fs-xs); padding: 2px 8px; border: var(--border-width) solid var(--color-border); border-radius: var(--radius-sm); background: transparent; color: var(--color-text-muted); cursor: pointer; }
  .filter-btn.active { background: var(--color-primary); color: var(--color-on-primary); border-color: var(--color-primary); }
  .toolbar { display: flex; gap: var(--space-1); }
  .create-form { display: flex; flex-direction: column; gap: var(--space-1); padding: var(--space-2); background: var(--color-surface); border: var(--border-width) solid var(--color-border); border-radius: var(--radius-md); }
  .create-form input { font-size: var(--fs-sm); padding: var(--space-1) var(--space-2); border: var(--border-width) solid var(--color-border); border-radius: var(--radius-sm); background: var(--color-bg); color: var(--color-text); }
  .empty { text-align: center; color: var(--color-text-muted); font-size: var(--fs-sm); padding: var(--space-4); }
  .topic-list { display: flex; flex-direction: column; gap: var(--space-1); }
  .topic-card { border: var(--border-width) solid var(--color-border); border-radius: var(--radius-md); background: var(--color-bg); overflow: hidden; }
  .topic-card.expanded { border-color: var(--color-primary); }
  .topic-summary { padding: var(--space-2); cursor: pointer; }
  .topic-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: var(--space-1); }
  .topic-name { font-size: var(--fs-sm); font-weight: 600; }
  .status-badge { font-size: 10px; font-weight: 600; padding: 1px 8px; border-radius: var(--radius-sm); color: #fff; }
  .progress-row { display: flex; align-items: center; gap: var(--space-2); margin-bottom: var(--space-1); }
  .progress-bar-bg { flex: 1; height: 4px; background: var(--color-border); border-radius: 2px; overflow: hidden; }
  .progress-bar-fill { height: 100%; background: var(--color-primary); border-radius: 2px; transition: width var(--duration-normal) var(--ease-out); }
  .progress-text { font-size: var(--fs-xs); color: var(--color-text-muted); min-width: 32px; text-align: right; }
  .topic-meta { font-size: var(--fs-xs); color: var(--color-text-muted); }
  .topic-detail { padding: 0 var(--space-2) var(--space-2); border-top: var(--border-width) solid var(--color-border); }
  .detail-row { display: flex; gap: var(--space-2); font-size: var(--fs-xs); padding: var(--space-1) 0; }
  .detail-label { font-weight: 600; color: var(--color-text-muted); min-width: 60px; }
  .mono { font-family: monospace; font-size: var(--fs-xs); }
  .scope-section { margin-top: var(--space-1); }
  .scope-header { display: flex; gap: var(--space-1); font-size: var(--fs-xs); padding: var(--space-1) 0; }
  .scope-add-form { display: flex; gap: var(--space-1); margin-bottom: var(--space-1); }
  .scope-add-form input { flex: 1; font-size: var(--fs-xs); padding: 2px 6px; border: var(--border-width) solid var(--color-border); border-radius: var(--radius-sm); background: var(--color-bg); color: var(--color-text); }
  .scope-list { display: flex; flex-direction: column; gap: 2px; }
  .scope-item { display: flex; align-items: center; gap: var(--space-1); padding: var(--space-1); border-radius: var(--radius-sm); background: var(--color-surface); font-size: var(--fs-xs); }
  .scope-item.done { opacity: 0.6; }
  .scope-item-text { flex: 1; min-width: 0; }
  .scope-goal { font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .scope-contract { color: var(--color-text-muted); font-size: 10px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .scope-item-actions { display: flex; gap: 2px; align-items: center; }
  .scope-done-badge { font-size: 10px; color: var(--color-success, #22c55e); font-weight: 600; }
  .topic-actions { display: flex; gap: var(--space-1); margin-top: var(--space-2); padding-top: var(--space-1); border-top: var(--border-width) solid var(--color-border); }
  .delete-confirm { display: flex; gap: var(--space-1); align-items: center; font-size: var(--fs-xs); color: var(--color-danger, #ef4444); }

  /* ── Shared button styles (match SidePanel pattern) ── */
  .btn { font-size: var(--fs-xs); padding: 2px 10px; border: var(--border-width) solid var(--color-border); border-radius: var(--radius-sm); background: transparent; color: var(--color-text); cursor: pointer; white-space: nowrap; }
  .btn-primary { background: var(--color-primary); color: var(--color-on-primary); border-color: var(--color-primary); }
  .btn-sm { font-size: 10px; padding: 1px 8px; }
  .btn-danger { border-color: var(--color-danger, #ef4444); color: var(--color-danger, #ef4444); }
  .btn-done { border-color: var(--color-success, #22c55e); color: var(--color-success, #22c55e); }
  .btn:disabled { opacity: 0.4; cursor: default; }
</style>
