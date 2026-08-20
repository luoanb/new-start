<script lang="ts">
  // GitPanel：sidebar「git」视图（单实例，VSCode SCM 语义）。
  // - 数据源：dataStore.state.git（单一权威；写操作后依赖 StateChange::Git 事件自动刷新）
  // - 写操作确认（commit/push/pull/checkout/stash 等）走后端确认服务 → GitConfirmHost 全局弹窗
  // - 危险写开关（reset --hard / checkout 覆盖未提交改动）默认关闭
  import { dataStore } from "$lib/stores/dataStore.svelte";
  import { fileEditorStore, fileKey } from "$lib/stores/fileEditorStore.svelte";
  import { layoutStore } from "$lib/layout/LayoutStore.svelte";
  import { t } from "$lib/i18n";
  import { formatInvokeError } from "$lib/utils/formatInvokeError";
  import Select from "./Select.svelte";
  import ContextMenu, { type ContextMenuItem } from "./ContextMenu.svelte";
  import type { GitStatusEntry } from "$lib/types";

  const git = $derived(dataStore.state.git);
  const activeRepo = $derived(git?.repos.find((r) => r.id === git?.activeRepoId) ?? null);
  const status = $derived(git?.status ?? null);
  const workspaces = $derived(dataStore.state.workspaces);
  const activeWs = $derived(
    workspaces?.workspaces.find((w) => w.id === workspaces?.active_id) ?? null,
  );

  const conflicted = $derived(status?.conflicted ?? []);
  const staged = $derived(status?.staged ?? []);
  const unstaged = $derived(status?.unstaged ?? []);
  const untracked = $derived(status?.untracked ?? []);
  const allChanges = $derived([...unstaged, ...untracked]);

  const currentBranch = $derived(status?.branch ?? null);
  const branchOptions = $derived(
    (git?.branches ?? []).map((b) => ({
      value: b.name,
      label: b.current ? `${b.name} ✓` : b.name,
    })),
  );
  const repoOptions = $derived(
    (git?.repos ?? []).map((r) => ({ value: r.id, label: r.name })),
  );

  // ── 作用域工具条 ⋯ 菜单 ──
  let menu = $state<{ items: ContextMenuItem[]; x: number; y: number } | null>(null);
  let menuBtnEl = $state<HTMLButtonElement | null>(null);

  function openMenu() {
    if (!menuBtnEl) return;
    const r = menuBtnEl.getBoundingClientRect();
    menu = {
      x: r.right - 200,
      y: r.bottom + 4,
      items: [
        { label: t("git.pull"), onSelect: () => void run(() => dataStore.gitPull()) },
        { label: t("git.push"), onSelect: () => void run(() => dataStore.gitPush()) },
        {
          label: t("git.discard"),
          danger: true,
          disabled: unstaged.length === 0,
          onSelect: discardAll,
        },
        {
          label: t("git.editIgnore"),
          disabled: !activeWs,
          onSelect: () => {
            if (activeWs) ignoreEdit = { ws: activeWs, text: activeWs.ignore.join("\n") };
          },
        },
      ],
    };
  }

  // ── 折叠区段 ──
  let coll = $state<Record<string, boolean>>({
    conflicted: true,
    staged: true,
    changes: true,
    branches: false,
    stash: false,
  });
  function toggleColl(key: string) {
    coll[key] = !coll[key];
  }

  // ── 交互 ──
  let error = $state("");
  async function run(fn: () => Promise<void>) {
    try {
      error = "";
      await fn();
    } catch (e) {
      error = formatInvokeError(e);
    }
  }

  function setRepo(repoId: string) {
    if (repoId && repoId !== git?.activeRepoId) void run(() => dataStore.setActiveGitRepo(repoId));
  }

  function checkout(target: string) {
    if (target === currentBranch) return;
    void run(() => dataStore.gitCheckout(target));
  }

  function stagePath(e: GitStatusEntry) {
    void run(() => dataStore.gitAdd([e.path]));
  }
  function unstagePath(e: GitStatusEntry) {
    void run(() => dataStore.gitUnstage([e.path]));
  }
  /** 批量：全部暂存（git add -A）／全部取消暂存（git restore --staged -- .）。 */
  function stageAllChanges() {
    if (allChanges.length === 0) return;
    void run(() => dataStore.gitAdd([], true));
  }
  function unstageAllChanges() {
    if (staged.length === 0) return;
    void run(() => dataStore.gitUnstage([]));
  }
  function discardAll() {
    if (unstaged.length === 0) return;
    void run(() => dataStore.gitRestore(unstaged.map((x) => x.path)));
  }

  /** 条目点击：未跟踪文件无 diff 直接打开编辑器；其余打开 git-diff 面板。 */
  function openEntry(e: GitStatusEntry) {
    if (!activeRepo) return;
    if (e.status === "??") {
      if (activeWs) {
        const key = fileKey(activeWs.id, e.path);
        fileEditorStore.open(key, activeWs.id, e.path, null);
        layoutStore.insertPanel("file-editor", undefined, key);
      }
      return;
    }
    dataStore.openGitDiff(activeRepo.id, e.path);
  }

  // ── 提交区 ──
  let commitMsg = $state("");
  let committing = $state(false);
  async function doCommit() {
    if (staged.length === 0 || commitMsg.trim() === "") return;
    committing = true;
    await run(async () => {
      await dataStore.gitCommit(commitMsg.trim());
      commitMsg = "";
    });
    committing = false;
  }

  // ── Stash ──
  let stashMsg = $state("");
  async function doStash(action: "push" | "apply" | "drop") {
    await run(() => dataStore.gitStash(action, action === "push" ? stashMsg || undefined : undefined));
    if (action === "push") stashMsg = "";
  }

  // ── 危险写开关 ──
  async function setDangerous(checked: boolean) {
    await run(() => dataStore.setDangerousWrites(checked));
  }

  // ── 编辑仓库忽略（复用 workspace ignore 规则编辑）──
  let ignoreEdit = $state<{ ws: { id: string; ignore: string[] }; text: string } | null>(null);
  async function saveIgnore() {
    if (!ignoreEdit) return;
    const lines = ignoreEdit.text
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean);
    await run(() => dataStore.updateWorkspaceIgnore(ignoreEdit!.ws.id, lines));
    ignoreEdit = null;
  }

  function statusTone(tone: "staged" | "changes" | "conflict"): string {
    return tone === "conflict" ? "error" : tone === "staged" ? "staged" : "warning";
  }
</script>

<svelte:window />

<div class="git-panel">
  {#if error}
    <p class="error-bar">{error}</p>
  {/if}

  {#if !git || git.repos.length === 0}
    <p class="hint-empty">{t("git.notRepo")}</p>
  {:else}
    <!-- 作用域工具条 -->
    <div class="toolbar">
      <Select
        value={activeRepo?.id ?? ""}
        options={repoOptions}
        placeholder={t("git.repoPlaceholder")}
        class="toolbar-select"
        onchange={(v) => setRepo(String(v))}
      />
      <Select
        value={currentBranch ?? ""}
        options={branchOptions}
        placeholder={t("git.branch")}
        disabled={branchOptions.length === 0}
        class="toolbar-select"
        onchange={(v) => checkout(String(v))}
      />
      <button
        bind:this={menuBtnEl}
        class="icon-btn"
        title={t("git.branches")}
        aria-label={t("git.branches")}
        onclick={openMenu}
      >⋯</button>
    </div>

    <!-- 变更汇总 -->
    <div class="summary">
      <span class="summary-text">
        {t("git.summary", { staged: staged.length, changes: allChanges.length })}
      </span>
      <button
        class="icon-btn"
        title={t("git.refresh")}
        aria-label={t("git.refresh")}
        onclick={() => void dataStore.refreshGit()}
      >↻</button>
    </div>

    <!-- 冲突分组 -->
    {#if conflicted.length > 0}
      <button class="group-head" onclick={() => toggleColl("conflicted")}>
        <span class="chevron" class:open={coll.conflicted}>▸</span>
        {t("git.groupConflicted")} ({conflicted.length})
      </button>
      {#if coll.conflicted}
        {#each conflicted as e (e.path)}
          {@const checked = true}
          <div class="item" class:error title={e.path}>
            <input type="checkbox" {checked} disabled />
            <span class="badge error">{e.status}</span>
            <button class="name" onclick={() => openEntry(e)}>{e.is_dir ? `${e.path}/` : e.path}</button>
            <button class="op" title={t("git.stage")} onclick={() => stagePath(e)}>＋</button>
          </div>
        {/each}
      {/if}
    {/if}

    <!-- 暂存区 -->
    <div class="group-row">
      <button class="group-head" onclick={() => toggleColl("staged")}>
        <span class="chevron" class:open={coll.staged}>▸</span>
        {t("git.groupStaged")} ({staged.length})
      </button>
      <button
        class="op group-act"
        title={t("git.unstageAll")}
        aria-label={t("git.unstageAll")}
        disabled={staged.length === 0}
        onclick={() => unstageAllChanges()}
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
          <path d="M3 8h4" />
          <path d="M9 8h4" />
        </svg>
      </button>
    </div>
    {#if coll.staged}
      {#each staged as e (e.path)}
        <div class="item" class:staged title={e.path}>
          <input type="checkbox" checked onchange={() => unstagePath(e)} />
          <span class="badge staged">{e.status}</span>
          <button class="name" onclick={() => openEntry(e)}>{e.is_dir ? `${e.path}/` : e.path}</button>
          <button class="op" title={t("git.unstage")} onclick={() => unstagePath(e)}>−</button>
        </div>
      {/each}
      {#if staged.length === 0}
        <p class="hint-muted">{t("git.clean")}</p>
      {/if}
    {/if}

    <!-- 更改（未暂存 + 未跟踪） -->
    <div class="group-row">
      <button class="group-head" onclick={() => toggleColl("changes")}>
        <span class="chevron" class:open={coll.changes}>▸</span>
        {t("git.groupChanges")} ({allChanges.length})
      </button>
      <button
        class="op group-act"
        title={t("git.stageAll")}
        aria-label={t("git.stageAll")}
        disabled={allChanges.length === 0}
        onclick={() => stageAllChanges()}
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
          <path d="M3 8h4M5 6v4" />
          <path d="M9 8h4M11 6v4" />
        </svg>
      </button>
    </div>
    {#if coll.changes}
      {#each allChanges as e (e.path)}
        <div class="item" title={e.path}>
          <input type="checkbox" onchange={() => stagePath(e)} />
          <span class="badge {statusTone(e.status.includes("U") ? "conflict" : "changes")}">{e.status}</span>
          <button class="name" onclick={() => openEntry(e)}>{e.is_dir ? `${e.path}/` : e.path}</button>
          <button class="op" title={t("git.stage")} onclick={() => stagePath(e)}>＋</button>
        </div>
      {/each}
      {#if allChanges.length === 0}
        <p class="hint-muted">{t("git.clean")}</p>
      {/if}
    {/if}

    <!-- 提交区 -->
    <div class="commit-area">
      <input
        class="commit-input"
        bind:value={commitMsg}
        placeholder={t("git.commitPlaceholder")}
        disabled={staged.length === 0}
        onkeydown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            void doCommit();
          }
        }}
      />
      <button
        class="btn btn-primary commit-btn"
        disabled={staged.length === 0 || commitMsg.trim() === "" || committing}
        title={staged.length === 0 ? t("git.nothingToCommit") : ""}
        onclick={() => void doCommit()}
      >{committing ? t("git.committing") : t("git.commit")}</button>
    </div>

    <!-- 分支区段 -->
    <button class="group-head" onclick={() => toggleColl("branches")}>
      <span class="chevron" class:open={coll.branches}>▸</span>
      {t("git.branches")} ({branchOptions.length})
    </button>
    {#if coll.branches}
      <div class="branch-list">
        {#each git?.branches ?? [] as b (b.name)}
          <button
            class="branch-item"
            class:current={b.current}
            onclick={() => checkout(b.name)}
            title={b.upstream ? `${b.name} → ${b.upstream}` : b.name}
          >
            <span class="branch-check">{b.current ? "✓" : ""}</span>
            <span class="branch-name">{b.name}</span>
            {#if b.upstream}<span class="branch-upstream">{b.upstream}</span>{/if}
          </button>
        {/each}
        {#if (git?.branches?.length ?? 0) === 0}
          <p class="hint-muted">{t("git.clean")}</p>
        {/if}
      </div>
    {/if}

    <!-- Stash 区段 -->
    <button class="group-head" onclick={() => toggleColl("stash")}>
      <span class="chevron" class:open={coll.stash}>▸</span>
      {t("git.stash")} ({git?.stash.length ?? 0})
    </button>
    {#if coll.stash}
      <div class="stash-list">
        <div class="stash-create">
          <input
            class="commit-input"
            bind:value={stashMsg}
            placeholder={t("git.stashCreate")}
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                void doStash("push");
              }
            }}
          />
          <button class="btn btn-sm" onclick={() => void doStash("push")}>+ {t("git.stashCreate")}</button>
        </div>
        {#each git?.stash ?? [] as s (s.index)}
          <div class="stash-item" title={`stash@{s.index}: ${s.message}`}>
            <span class="stash-label">stash@{s.index}: {s.message}</span>
            <button class="op" title={t("git.stashApply")} onclick={() => void doStash("apply")}>⎆</button>
            <button class="op danger" title={t("git.stashDrop")} onclick={() => void doStash("drop")}>✕</button>
          </div>
        {/each}
        {#if (git?.stash?.length ?? 0) === 0}
          <p class="hint-muted">{t("git.clean")}</p>
        {/if}
      </div>
    {/if}

    <!-- 危险写开关 -->
    <label class="danger-toggle">
      <input
        type="checkbox"
        checked={git?.confirmConfig.dangerous_writes ?? false}
        onchange={(e) => void setDangerous(e.currentTarget.checked)}
      />
      <span>{t("git.dangerousWrites")}</span>
    </label>
  {/if}
</div>

{#if menu}
  <ContextMenu items={menu.items} x={menu.x} y={menu.y} onClose={() => (menu = null)} />
{/if}

{#if ignoreEdit}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="overlay" role="presentation" onclick={() => (ignoreEdit = null)}>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="modal" onclick={(e) => e.stopPropagation()}>
      <div class="modal-header"><h2>{t("fileExplorer.ignoreTitle")}</h2></div>
      <div class="modal-body">
        <p class="modal-hint">{t("fileExplorer.ignoreHint")}</p>
        <textarea class="ignore-text" rows="8" bind:value={ignoreEdit.text}></textarea>
      </div>
      <div class="modal-footer">
        <button class="btn" onclick={() => (ignoreEdit = null)}>{t("fileExplorer.ignoreCancel")}</button>
        <button class="btn btn-primary" onclick={() => void saveIgnore()}>{t("fileExplorer.ignoreSave")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .git-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow-y: auto;
    background: var(--color-surface);
  }

  .error-bar {
    margin: 0;
    padding: var(--space-2) var(--space-3);
    font-size: var(--fs-xs);
    color: var(--color-error);
    background: var(--color-error-bg);
  }
  .hint-empty {
    padding: var(--space-4);
    font-size: var(--fs-sm);
    color: var(--color-text-muted);
    text-align: center;
  }
  .hint-muted {
    margin: 0;
    padding: var(--space-1) var(--space-3) var(--space-2);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }

  /* 作用域工具条 */
  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1) var(--space-2);
    border-bottom: 1px solid var(--color-border);
    min-height: 36px;
  }
  .toolbar-select {
    flex: 1 1 0;
    min-width: 0;
  }
  .toolbar-select :global(.select) {
    width: 100%;
  }

  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    flex-shrink: 0;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-muted);
    font-size: var(--fs-base);
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out);
  }
  .icon-btn:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  /* 汇总行 */
  .summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-1) var(--space-3);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }
  .summary-text :global(b) {
    color: var(--color-primary);
  }

  /* 分组头 */
  .group-row {
    display: flex;
    align-items: center;
  }
  .group-row .group-head {
    flex: 1 1 auto;
    width: auto;
  }
  .group-head {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    width: 100%;
    height: 28px;
    padding: 0 var(--space-3);
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
    font-weight: 600;
    text-align: left;
    cursor: pointer;
  }
  .group-head:hover {
    color: var(--color-text);
  }
  .chevron {
    display: inline-block;
    width: 10px;
    transition: transform var(--duration-fast) var(--ease-out);
    font-size: var(--fs-xs);
  }
  .chevron.open {
    transform: rotate(90deg);
  }

  /* 分组批量操作按钮（复用 .op 图标按钮样式，仅补间距） */
  .group-act {
    margin-right: var(--space-2);
  }

  /* 条目行 */
  .item {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    height: 28px;
    padding: 0 var(--space-2) 0 var(--space-3);
    font-size: var(--fs-sm);
  }
  .item:hover {
    background: var(--color-hover);
  }
  .item input[type="checkbox"] {
    accent-color: var(--color-primary);
    flex-shrink: 0;
    width: 14px;
    height: 14px;
    margin: 0;
    cursor: pointer;
  }
  .badge {
    flex-shrink: 0;
    min-width: 18px;
    padding: 0 var(--space-1);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    text-align: center;
  }
  .badge.staged { color: var(--color-primary); background: color-mix(in oklch, var(--color-primary) 12%, transparent); }
  .badge.warning { color: var(--color-warning); background: color-mix(in oklch, var(--color-warning) 12%, transparent); }
  .badge.error { color: var(--color-error); background: var(--color-error-bg); }
  .item.error .name { color: var(--color-error); }

  .name {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    border: none;
    background: transparent;
    color: var(--color-text);
    font-size: var(--fs-sm);
    text-align: left;
    cursor: pointer;
  }
  .name:hover {
    color: var(--color-primary);
  }

  .op {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    flex-shrink: 0;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-muted);
    font-size: var(--fs-base);
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out);
  }
  .op:hover { background: var(--color-hover); color: var(--color-text); }
  .op.danger:hover { color: var(--color-error); }

  /* 提交区 */
  .commit-area {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-3);
    border-top: 1px solid var(--color-border);
  }
  .commit-input {
    width: 100%;
    padding: var(--space-2);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: var(--fs-sm);
    outline: none;
    transition: border-color var(--duration-fast) var(--ease-out);
  }
  .commit-input:focus { border-color: var(--color-primary); }
  .commit-input:disabled { opacity: 0.5; }
  .commit-btn { width: 100%; }
  .commit-btn:disabled { opacity: 0.45; cursor: default; }

  /* 分支 / Stash */
  .branch-list, .stash-list { padding: var(--space-1) 0; }
  .branch-item {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    width: 100%;
    height: 28px;
    padding: 0 var(--space-3);
    border: none;
    background: transparent;
    color: var(--color-text);
    font-size: var(--fs-sm);
    text-align: left;
    cursor: pointer;
  }
  .branch-item:hover { background: var(--color-hover); }
  .branch-item.current { color: var(--color-primary); font-weight: 600; }
  .branch-check { width: 14px; flex-shrink: 0; font-size: var(--fs-xs); }
  .branch-name { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .branch-upstream {
    flex-shrink: 0;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }

  .stash-create {
    display: flex;
    gap: var(--space-1);
    padding: 0 var(--space-3) var(--space-1);
  }
  .stash-create .commit-input { flex: 1 1 auto; padding: var(--space-1) var(--space-2); }
  .stash-item {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    height: 28px;
    padding: 0 var(--space-3);
    font-size: var(--fs-sm);
  }
  .stash-item:hover { background: var(--color-hover); }
  .stash-label {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* 危险写开关 */
  .danger-toggle {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-top: 1px solid var(--color-border);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    cursor: pointer;
    user-select: none;
  }
  .danger-toggle input { accent-color: var(--color-warning); margin-top: 2px; }
  .danger-toggle span { line-height: 1.5; }

  /* ignore 编辑弹窗（对齐 FileExplorer 词汇） */
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .modal {
    background: var(--color-surface);
    border-radius: var(--radius-lg);
    width: 420px;
    max-width: 90vw;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
  }
  .modal-header { padding: 14px 20px; border-bottom: 1px solid var(--color-border); }
  .modal-header h2 { margin: 0; font-size: var(--fs-lg); font-weight: 600; }
  .modal-body { padding: 16px 20px; }
  .modal-hint { margin: 0 0 var(--space-2); font-size: var(--fs-xs); color: var(--color-text-muted); line-height: 1.5; }
  .ignore-text {
    width: 100%;
    box-sizing: border-box;
    padding: var(--space-2);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-elevated);
    color: var(--color-text);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    resize: vertical;
    outline: none;
  }
  .ignore-text:focus { border-color: var(--color-primary); }
  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    padding: 12px 20px;
    border-top: 1px solid var(--color-border);
  }
</style>
