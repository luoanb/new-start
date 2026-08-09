<script lang="ts">
  import type { TopicStatus } from "$lib/types";
  import { t, tMap } from "$lib/i18n";
  import { errorMessage } from "$lib/errorMessage";
  import { dataStore } from "$lib/stores/dataStore.svelte";

  // 统一从 dataStore 读取 topics，不再由父组件 bind 传入。
  let topics = $derived(dataStore.state.topics);

  // ── State ──
  type TopicFilter = "all" | "active" | "done";
  let filter = $state<TopicFilter>("active");
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

  // 三段式聚合：未完成 = todo+in_progress+paused；已完成 = done+cancelled。
  const ACTIVE: TopicStatus[] = ["todo", "in_progress", "paused"];
  const DONE: TopicStatus[] = ["done", "cancelled"];

  // ── Derived ──
  let filteredTopics = $derived(
    filter === "active"
      ? topics.filter((t) => ACTIVE.includes(t.status))
      : filter === "done"
        ? topics.filter((t) => DONE.includes(t.status))
        : topics
  );

  const topicFilters: TopicFilter[] = ["active", "all", "done"];

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
      const topic = await dataStore.createTopic(createName.trim(), createDesc.trim());
      createName = "";
      createDesc = "";
      showCreateForm = false;
      expandedId = topic.id;
    } catch (e) {
      errorMsg = t("topicPanel.createFailed", { error: errorMessage(e) });
    } finally {
      creating = false;
    }
  }

  async function handlePause(id: string) {
    errorMsg = "";
    try {
      await dataStore.pauseTopic(id);
    } catch (e) {
      errorMsg = t("topicPanel.pauseFailed", { error: errorMessage(e) });
    }
  }

  async function handleResume(id: string) {
    errorMsg = "";
    try {
      await dataStore.resumeTopic(id);
    } catch (e) {
      errorMsg = t("topicPanel.resumeFailed", { error: errorMessage(e) });
    }
  }

  async function handleDelete(id: string) {
    errorMsg = "";
    deleteConfirmId = null;
    try {
      await dataStore.deleteTopic(id);
      if (expandedId === id) expandedId = null;
    } catch (e) {
      errorMsg = t("topicPanel.deleteFailed", { error: errorMessage(e) });
    }
  }

  async function handleAddScopeItem(topicId: string) {
    if (!newScopeGoal.trim() || !newScopeContract.trim()) return;
    addingScope = true;
    errorMsg = "";
    try {
      await dataStore.addScopeItem(topicId, newScopeGoal.trim(), newScopeContract.trim());
      newScopeGoal = "";
      newScopeContract = "";
    } catch (e) {
      errorMsg = t("topicPanel.addScopeFailed", { error: errorMessage(e) });
    } finally {
      addingScope = false;
    }
  }

  async function handleCompleteScopeItem(topicId: string, itemId: string) {
    errorMsg = "";
    try {
      await dataStore.completeScopeItem(topicId, itemId);
    } catch (e) {
      errorMsg = t("topicPanel.completeScopeFailed", { error: errorMessage(e) });
    }
  }

  async function handleDeleteScopeItem(topicId: string, itemId: string) {
    errorMsg = "";
    try {
      await dataStore.deleteScopeItem(topicId, itemId);
    } catch (e) {
      errorMsg = t("topicPanel.deleteScopeFailed", { error: errorMessage(e) });
    }
  }
</script>

<div class="topic-panel">
  {#if errorMsg}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="error-banner" onclick={() => (errorMsg = "")}>{errorMsg}</div>
  {/if}

  <!-- 面板标题 + 新建入口（对齐 ToolPanel 的 panel-toolbar 词汇） -->
  <div class="panel-toolbar">
    <span class="panel-title">{t("topicPanel.topics")}</span>
    <div class="toolbar-actions">
      <button
        class="icon-btn"
        onclick={() => (showCreateForm = !showCreateForm)}
        title={t("topicPanel.create")}
        aria-label={t("topicPanel.create")}
      >
        <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
      </button>
    </div>
  </div>

  <!-- Status filters: 全部 / 未完成 / 已完成 -->
  <div class="filter-bar" role="group" aria-label={t("topicPanel.status")}>
    {#each topicFilters as f}
      <button
        class="filter-btn"
        class:active={filter === f}
        onclick={() => (filter = f)}
      >
        {f === "all"
          ? t("topicPanel.all")
          : f === "active"
            ? t("topicPanel.filterActive")
            : t("topicPanel.filterDone")}
      </button>
    {/each}
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
              <div class="topic-header-actions">
                <span class="status-badge {topic.status}">
                  {tMap("topicPanel.topicStatus", topic.status)}
                </span>
                {#if deleteConfirmId === topic.id}
                  <span class="delete-confirm" title={t("topicPanel.deleteConfirm")}>
                    <button class="btn btn-sm btn-danger" onclick={() => handleDelete(topic.id)}>
                      {t("topicPanel.confirm")}
                    </button>
                    <button class="btn btn-sm" onclick={() => (deleteConfirmId = null)}>
                      {t("topicPanel.cancel")}
                    </button>
                  </span>
                {:else}
                  <button
                    class="icon-btn danger"
                    onclick={() => (deleteConfirmId = topic.id)}
                    title={t("topicPanel.confirm")}
                    aria-label={t("topicPanel.confirm")}
                  >
                    <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
                  </button>
                {/if}
              </div>
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
                  {#if topic.status !== "paused"}
                    <button
                      class="icon-btn"
                      onclick={() => handleAddScopeItem(topic.id)}
                      disabled={addingScope || !newScopeGoal.trim() || !newScopeContract.trim()}
                      title={t("topicPanel.scopeAdd")}
                      aria-label={t("topicPanel.scopeAdd")}
                    >
                      <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
                    </button>
                  {/if}
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
                            class="icon-btn done"
                            onclick={() => handleCompleteScopeItem(topic.id, item.id)}
                            disabled={topic.status === "paused"}
                            title={t("topicPanel.scopeStatusDone")}
                            aria-label={t("topicPanel.scopeStatusDone")}
                          >
                            <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12" /></svg>
                          </button>
                        {:else}
                          <span class="status-badge done">{t("topicPanel.scopeStatusDone")}</span>
                        {/if}
                        <button
                          class="icon-btn danger"
                          onclick={() => handleDeleteScopeItem(topic.id, item.id)}
                          title={t("topicPanel.confirm")}
                          aria-label={t("topicPanel.confirm")}
                        >
                          <svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
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
              </div>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  /* 面板容器：对齐 ToolPanel 的 .tools-panel 间距（padding / gap / flex 约束） */
  .topic-panel {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    gap: var(--space-6);
    padding: var(--space-3) var(--space-4);
    overflow: auto;
  }
  .error-banner { background: var(--color-error); color: #fff; padding: var(--space-1) var(--space-2); border-radius: var(--radius-md); font-size: var(--fs-xs); cursor: pointer; }
  /* 面板标题栏：对齐 ToolPanel 的 panel-toolbar / panel-title / toolbar-actions 词汇 */
  .panel-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .panel-title {
    font-size: var(--fs-base);
    font-weight: 600;
    color: var(--color-text);
  }
  .toolbar-actions {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
  .filter-bar {
    display: flex;
    gap: 2px;
    padding: 2px;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }
  .filter-btn {
    flex: 1;
    font-size: var(--fs-xs);
    padding: 3px 10px;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    white-space: nowrap;
    transition: background var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out);
  }
  .filter-btn:hover { background: var(--color-hover); color: var(--color-text); }
  .filter-btn.active { background: var(--color-primary); color: var(--color-on-primary); }
  /* 与全项目 icon-btn 词汇一致：无边框方形 + hover tint */
  .icon-btn {
    flex-shrink: 0;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    width: 26px;
    height: 26px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-muted);
    transition: background var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out);
  }
  .icon-btn:hover { background: var(--color-hover); color: var(--color-text); }
  .icon-btn:disabled { opacity: 0.4; cursor: default; }
  .icon-btn.done { color: var(--color-success); }
  .icon-btn.danger { color: var(--color-error); }
  .icon-btn .icon { display: block; }
  .create-form { display: flex; flex-direction: column; gap: var(--space-1); padding: var(--space-2); background: var(--color-surface); border: var(--border-width) solid var(--color-border); border-radius: var(--radius-md); }
  .create-form input { font-size: var(--fs-sm); padding: var(--space-1) var(--space-2); border: var(--border-width) solid var(--color-border); border-radius: var(--radius-sm); background: var(--color-bg); color: var(--color-text); }
  .empty { text-align: center; color: var(--color-text-muted); font-size: var(--fs-sm); padding: var(--space-4); }
  .topic-list { display: flex; flex-direction: column; gap: var(--space-1); }
  .topic-card { border: var(--border-width) solid var(--color-border); border-radius: var(--radius-md); background: var(--color-bg); overflow: hidden; }
  .topic-card.expanded { border-color: var(--color-primary); }
  .topic-summary { padding: var(--space-2); cursor: pointer; }
  .topic-header { display: flex; justify-content: space-between; align-items: center; gap: var(--space-1); margin-bottom: var(--space-1); }
  .topic-header-actions { display: flex; align-items: center; gap: var(--space-1); }
  .topic-name { font-size: var(--fs-sm); font-weight: 600; }
  .status-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    font-weight: 600;
    padding: 1px 8px;
    border-radius: var(--radius-sm);
    background: var(--color-hover);
    color: var(--color-text-muted);
    white-space: nowrap;
  }
  .status-badge::before { content: ""; width: 6px; height: 6px; border-radius: 50%; background: currentColor; }
  .status-badge.todo { color: var(--color-text-muted); }
  .status-badge.in_progress { background: color-mix(in oklch, var(--color-primary) 12%, transparent); color: var(--color-primary); }
  .status-badge.paused { background: color-mix(in oklch, var(--color-warning) 12%, transparent); color: var(--color-warning); }
  .status-badge.done { background: color-mix(in oklch, var(--color-success) 12%, transparent); color: var(--color-success); }
  .status-badge.cancelled { background: color-mix(in oklch, var(--color-error) 12%, transparent); color: var(--color-error); }
  .progress-row { display: flex; align-items: center; gap: var(--space-2); margin-bottom: var(--space-1); }
  .progress-bar-bg { flex: 1; height: 4px; background: var(--color-border); border-radius: 2px; overflow: hidden; }
  .progress-bar-fill { height: 100%; background: var(--color-primary); border-radius: 2px; transition: width var(--duration-normal) var(--ease-out); }
  .progress-text { font-size: var(--fs-xs); color: var(--color-text-muted); min-width: 32px; text-align: right; }
  .topic-meta { font-size: var(--fs-xs); color: var(--color-text-muted); }
  .topic-detail { padding: 0 var(--space-2) var(--space-2); border-top: var(--border-width) solid var(--color-border); }
  .detail-row { display: flex; gap: var(--space-2); font-size: var(--fs-xs); padding: var(--space-1) 0; }
  .detail-label { flex-shrink: 0; font-weight: 600; color: var(--color-text-muted); }
  .detail-row > :not(.detail-label) { flex: 1; min-width: 0; }
  .mono { font-family: monospace; font-size: var(--fs-xs); }
  .scope-section { margin-top: var(--space-1); }
  .scope-header { display: flex; gap: var(--space-1); font-size: var(--fs-xs); padding: var(--space-1) 0; }
  .scope-header .icon-btn { margin-left: auto; }
  .scope-add-form { display: flex; flex-direction: column; gap: var(--space-1); margin-bottom: var(--space-1); }
  .scope-add-form input { flex: 1; font-size: var(--fs-xs); padding: 2px 6px; border: var(--border-width) solid var(--color-border); border-radius: var(--radius-sm); background: var(--color-bg); color: var(--color-text); }
  .scope-list { display: flex; flex-direction: column; gap: 2px; }
  .scope-item { display: flex; align-items: center; gap: var(--space-1); padding: var(--space-1); border-radius: var(--radius-sm); background: var(--color-surface); font-size: var(--fs-xs); }
  .scope-item.done { opacity: 0.6; }
  .scope-item-text { flex: 1; min-width: 0; }
  .scope-goal { font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .scope-contract { color: var(--color-text-muted); font-size: 10px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .scope-item-actions { display: flex; gap: 2px; align-items: center; }
  .topic-actions { display: flex; gap: var(--space-1); margin-top: var(--space-2); padding-top: var(--space-1); border-top: var(--border-width) solid var(--color-border); }
  .delete-confirm { display: flex; gap: var(--space-1); align-items: center; font-size: var(--fs-xs); color: var(--color-error); }
</style>
