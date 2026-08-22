<script lang="ts">
  // GitPanel：sidebar「git」视图（单实例，VSCode SCM 语义）。
  // - 数据源：dataStore.state.git（单一权威；写操作后依赖 StateChange::Git 事件自动刷新）
  // - 写操作确认（commit/push/pull/checkout/stash 等）走后端确认服务 → GitConfirmHost 全局弹窗
  // - 危险写开关（reset --hard / checkout 覆盖未提交改动）默认关闭
  import { dataStore } from "$lib/stores/dataStore.svelte";
  import { fileEditorStore, fileKey } from "$lib/stores/fileEditorStore.svelte";
  import { layoutStore } from "$lib/layout/LayoutStore.svelte";
  import { t, tMap } from "$lib/i18n";
  import { formatInvokeError } from "$lib/utils/formatInvokeError";
  import Select from "./Select.svelte";
  import ContextMenu, { type ContextMenuItem } from "./ContextMenu.svelte";
  import Tooltip from "./Tooltip.svelte";
  import type { GitStatusEntry, GitShowFile, GitFileDiff } from "$lib/types";

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
    log: false,
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
    if (repoId && repoId !== git?.activeRepoId) {
      resetLogExpansion();
      void run(() => dataStore.setActiveGitRepo(repoId));
    }
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

  /** 条目点击：未跟踪文件无 diff 直接打开编辑器；其余打开 git-diff 面板（range 按来源分组：暂存→staged / 工作区→unstaged / 冲突→both）。 */
  function openEntry(e: GitStatusEntry, range: "staged" | "unstaged" | "both") {
    if (!activeRepo) return;
    if (e.status.trim() === "??") {
      if (activeWs) {
        const key = fileKey(activeWs.id, e.path);
        fileEditorStore.open(key, activeWs.id, e.path, null);
        layoutStore.insertPanel("file-editor", undefined, key);
      }
      return;
    }
    dataStore.openGitDiff(activeRepo.id, e.path, range);
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

  // ── 提交记录（git log 区段，点击提交懒加载文件列表与 diff）──
  let openCommit = $state<string | null>(null);
  let commitFiles = $state<GitShowFile[] | null>(null);
  let openFile = $state<string | null>(null);
  let commitDiff = $state<GitFileDiff | null>(null);

  function resetLogExpansion() {
    openCommit = null;
    commitFiles = null;
    openFile = null;
    commitDiff = null;
  }

  function shortDate(iso: string): string {
    return iso.slice(0, 10);
  }

  /** 拆出文件名与目录前缀（目录弱化跟随在文件名右侧）；兼容目录尾斜杠。 */
  function splitPath(p: string): { name: string; dir: string } {
    const t = p.endsWith("/") ? p.slice(0, -1) : p;
    const i = t.lastIndexOf("/");
    return i < 0 ? { name: t, dir: "" } : { name: t.slice(i + 1), dir: t.slice(0, i + 1) };
  }

  async function toggleCommit(hash: string) {
    if (openCommit === hash) {
      resetLogExpansion();
      return;
    }
    openCommit = hash;
    openFile = null;
    commitDiff = null;
    commitFiles = null;
    await run(async () => {
      commitFiles = await dataStore.gitShowFiles(hash);
    });
  }

  async function toggleFile(f: GitShowFile) {
    if (openFile === f.path) {
      openFile = null;
      commitDiff = null;
      return;
    }
    openFile = f.path;
    commitDiff = null;
    if (f.is_binary || !openCommit) return;
    await run(async () => {
      commitDiff = await dataStore.gitShowDiff(openCommit!, f.path);
    });
  }

  /** 追加加载更早的提交历史（分页）。 */
  async function loadMoreLog() {
    await run(() => dataStore.loadMoreGitLog());
  }

  /** 把某提交中该文件的 diff 作为新面板打开到主区域。 */
  function openCommitDiffPanel(f: GitShowFile) {
    if (!activeRepo || !openCommit) return;
    dataStore.openCommitDiff(activeRepo.id, openCommit, f.path);
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

  /**
   * git 状态码说明（徽标 hover 提示）。
   * 优先精炼映射（trim 后匹配，如 MM/??）；未命中时用 states 拆解双字符模板兜底，
   * 保证任意状态码都有提示。
   */
  function statusHint(code: string): string {
    const c = code.trim();
    const exact = tMap("git.status", c);
    if (!exact.startsWith("git.status.")) return exact;
    if (c.length >= 2) {
      return t("git.statusTemplate", {
        x: tMap("git.states", c[0]),
        y: tMap("git.states", c[1]),
      });
    }
    return tMap("git.states", c);
  }
</script>

<svelte:window />

{#snippet groupIcon(kind: string)}
  {#if kind === "conflicted"}
    <path d="M8 3 2.5 13h11L8 3z" />
    <path d="M8 8v3" />
    <path d="M8 12.5h.01" />
  {:else if kind === "staged"}
    <rect x="3" y="3" width="10" height="10" rx="2" />
    <path d="M8 6v4M6 8h4" />
  {:else if kind === "changes"}
    <path d="M11.2 2.8l2 2L4.5 13.5h-2v-2L11.2 2.8z" />
  {:else if kind === "log"}
    <path d="M4 4.5a5 5 0 1 1 0 7" />
    <path d="M4 2.5v3h3" />
    <path d="M8 6v2.5l1.8 1.1" />
  {:else if kind === "branches"}
    <circle cx="4.5" cy="3.5" r="1.5" />
    <circle cx="4.5" cy="12.5" r="1.5" />
    <circle cx="11.5" cy="12.5" r="1.5" />
    <path d="M4.5 5v5" />
    <path d="M11.5 11c0-2-1.5-3.5-4-3.5" />
  {:else if kind === "stash"}
    <path d="M3 4.5h10v9H3z" />
    <path d="M6.5 8h3" />
  {/if}
{/snippet}

{#snippet badge(code: string, tone: string)}
  <Tooltip label={statusHint(code)} position="bottom">
    <!-- title="" 阻止继承父级 .item 的文件路径提示 -->
    <span class="badge {tone}" title="">{code}</span>
  </Tooltip>
{/snippet}

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
        <svg class="chevron" class:open={coll.conflicted} viewBox="0 0 16 16" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="6 4 10 8 6 12" /></svg>
        <svg class="group-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{@render groupIcon("conflicted")}</svg>
        {t("git.groupConflicted")} ({conflicted.length})
      </button>
      {#if coll.conflicted}
        {#each conflicted as e (e.path)}
          {@const { name, dir } = splitPath(e.path)}
          <div class="item" class:error title={e.path}>
            {@render badge(e.status, "error")}
            <button class="name" onclick={() => openEntry(e, "both")}>
              <span class="name-basename">{e.is_dir ? `${name}/` : name}</span>
              {#if dir}<span class="name-dir">{dir}</span>{/if}
            </button>
            <button class="op" title={t("git.stage")} onclick={() => stagePath(e)}>＋</button>
          </div>
        {/each}
      {/if}
    {/if}

    <!-- 暂存区 -->
    <div class="group-row">
      <button class="group-head" onclick={() => toggleColl("staged")}>
        <svg class="chevron" class:open={coll.staged} viewBox="0 0 16 16" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="6 4 10 8 6 12" /></svg>
        <svg class="group-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{@render groupIcon("staged")}</svg>
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
        {@const { name, dir } = splitPath(e.path)}
        <div class="item" class:staged title={e.path}>
          {@render badge(e.status, "staged")}
          <button class="name" onclick={() => openEntry(e, "staged")}>
            <span class="name-basename">{e.is_dir ? `${name}/` : name}</span>
            {#if dir}<span class="name-dir">{dir}</span>{/if}
          </button>
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
        <svg class="chevron" class:open={coll.changes} viewBox="0 0 16 16" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="6 4 10 8 6 12" /></svg>
        <svg class="group-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{@render groupIcon("changes")}</svg>
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
        {@const { name, dir } = splitPath(e.path)}
        <div class="item" title={e.path}>
          {@render badge(e.status, statusTone(e.status.includes("U") ? "conflict" : "changes"))}
          <button class="name" onclick={() => openEntry(e, "unstaged")}>
            <span class="name-basename">{e.is_dir ? `${name}/` : name}</span>
            {#if dir}<span class="name-dir">{dir}</span>{/if}
          </button>
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

    <!-- 提交记录 -->
    <button class="group-head" onclick={() => toggleColl("log")}>
      <svg class="chevron" class:open={coll.log} viewBox="0 0 16 16" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="6 4 10 8 6 12" /></svg>
      <svg class="group-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{@render groupIcon("log")}</svg>
      {t("git.groupLog")} ({git?.log.length ?? 0})
    </button>
    {#if coll.log}
      {#each git?.log ?? [] as c (c.hash)}
        <button
          class="commit-item"
          class:open={openCommit === c.hash}
          onclick={() => void toggleCommit(c.hash)}
          title={c.subject}
        >
          <span class="commit-short">{c.short}</span>
          <span class="commit-subject">{c.subject}</span>
          <span class="commit-meta">{c.author} · {shortDate(c.date)}</span>
        </button>
        {#if openCommit === c.hash}
          {#if commitFiles === null}
            <p class="hint-muted">{t("git.logLoading")}</p>
          {:else if commitFiles.length === 0}
            <p class="hint-muted">{t("git.logEmpty")}</p>
          {:else}
            {#each commitFiles as f (f.path)}
              {@const { name, dir } = splitPath(f.path)}
              <div
                class="commit-file"
                class:open={openFile === f.path}
                role="button"
                tabindex="0"
                onclick={() => void toggleFile(f)}
                onkeydown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    void toggleFile(f);
                  }
                }}
                title={f.path}
              >
                <span class="cf-name">{name}</span>
                {#if dir}<span class="cf-dir">{dir}</span>{/if}
                {#if f.is_binary}
                  <span class="cf-stat binary">{t("git.binaryDiff")}</span>
                {:else}
                  <span class="cf-stat"><i class="add">+{f.additions}</i> <i class="del">-{f.deletions}</i></span>
                {/if}
                <button
                  class="cf-open"
                  disabled={f.is_binary}
                  title={t("git.openCommitDiff")}
                  onclick={(e) => {
                    e.stopPropagation();
                    void openCommitDiffPanel(f);
                  }}
                >
                  <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 2h8v8" /><path d="M2 6v8h8" /><path d="m2 14 12-12" /></svg>
                </button>
              </div>
              {#if openFile === f.path}
                {#if f.is_binary}
                  <p class="hint-muted">{t("git.binaryDiff")}</p>
                {:else if commitDiff}
                  <div class="commit-diff">
                    {#each commitDiff.hunks as h (h.header)}
                      <div class="hunk-header">{h.header}</div>
                      {#each h.lines as l, i (i)}
                        <div class="dline {l.kind}">
                          <span class="ln">{l.old_no ?? ""}</span>
                          <span class="ln">{l.new_no ?? ""}</span>
                          <span class="dt">{l.text}</span>
                        </div>
                      {/each}
                    {/each}
                  </div>
                {/if}
              {/if}
            {/each}
          {/if}
        {/if}
      {/each}
      {#if (git?.log?.length ?? 0) === 0}
        <p class="hint-muted">{t("git.logEmpty")}</p>
      {/if}
      {#if git?.logHasMore}
        <button class="log-more" onclick={() => void loadMoreLog()}>{t("git.logMore")}</button>
      {/if}
    {/if}

    <!-- 分支区段 -->
    <button class="group-head" onclick={() => toggleColl("branches")}>
      <svg class="chevron" class:open={coll.branches} viewBox="0 0 16 16" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="6 4 10 8 6 12" /></svg>
      <svg class="group-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{@render groupIcon("branches")}</svg>
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
      <svg class="chevron" class:open={coll.stash} viewBox="0 0 16 16" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="6 4 10 8 6 12" /></svg>
      <svg class="group-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{@render groupIcon("stash")}</svg>
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
    font-size: var(--fs-sm);
    font-weight: 600;
    text-align: left;
    cursor: pointer;
  }
  .group-head:hover {
    color: var(--color-text);
  }
  .chevron {
    flex-shrink: 0;
    transition: transform var(--duration-fast) var(--ease-out);
    transform-origin: center;
  }
  .chevron.open {
    transform: rotate(90deg);
  }
  .group-icon {
    flex-shrink: 0;
    width: 14px;
    height: 14px;
    color: var(--color-text-muted);
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
  .badge {
    flex-shrink: 0;
    min-width: 18px;
    padding: 0 var(--space-1);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    text-align: center;
    cursor: default;
  }
  .badge.staged { color: var(--color-primary); background: color-mix(in oklch, var(--color-primary) 12%, transparent); }
  .badge.warning { color: var(--color-warning); background: color-mix(in oklch, var(--color-warning) 12%, transparent); }
  .badge.error { color: var(--color-error); background: var(--color-error-bg); }
  .item.error .name { color: var(--color-error); }

  .name {
    display: flex;
    align-items: baseline;
    gap: 2px;
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
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
  .name-basename {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name-dir {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }
  .name:hover .name-dir {
    color: var(--color-text-muted);
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

  /* 提交记录区段 */
  .commit-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    height: 28px;
    padding: 0 var(--space-3);
    border: none;
    background: transparent;
    text-align: left;
    cursor: pointer;
  }
  .commit-item:hover { background: var(--color-hover); }
  .commit-item.open { background: var(--color-hover); }
  .commit-short {
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--color-primary);
  }
  .commit-subject {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-sm);
    color: var(--color-text);
  }
  .commit-meta {
    flex-shrink: 1;
    min-width: 0;
    max-width: 45%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }

  .commit-file {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    width: 100%;
    height: 26px;
    padding: 0 var(--space-3) 0 var(--space-5);
    border: none;
    background: transparent;
    text-align: left;
    cursor: pointer;
  }
  .commit-file:hover { background: var(--color-hover); }
  .commit-file.open { background: var(--color-hover); }
  .cf-name {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-sm);
    color: var(--color-text);
  }
  .cf-dir {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }
  .cf-stat {
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }
  .cf-stat i { font-style: normal; }
  .cf-stat .add { color: var(--color-success); }
  .cf-stat .del { color: var(--color-error); }
  .cf-stat.binary { font-size: var(--fs-xs); }

  .commit-diff {
    margin: 0 var(--space-2) var(--space-1) var(--space-5);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    overflow-x: auto;
    /* 固定高度：详情区恒定高度，内容过长内部滚动，不受文件大小/布局影响 */
    height: 320px;
    overflow-y: auto;
    /* .git-panel 是 flex 列布局；不加会因默认 flex-shrink:1 被压缩（overflow:auto 使 min-height 归零） */
    flex-shrink: 0;
  }
  /* commit-file 行右侧「在主区域打开」icon 按钮 */
  .cf-open {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    opacity: 0.6;
  }
  .cf-open:hover:not(:disabled) {
    opacity: 1;
    color: var(--color-primary);
    background: var(--color-hover);
  }
  .cf-open:disabled { opacity: 0.3; cursor: default; }
  /* 提交记录「加载更多」 */
  .log-more {
    display: block;
    width: calc(100% - var(--space-8));
    margin: var(--space-1) var(--space-4);
    padding: var(--space-1);
    border: var(--border-width) dashed var(--color-border);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
    cursor: pointer;
  }
  .log-more:hover {
    color: var(--color-primary);
    border-color: var(--color-primary);
  }
  .hunk-header {
    padding: 2px var(--space-2);
    background: var(--color-elevated);
    color: var(--color-text-muted);
    white-space: nowrap;
  }
  .dline {
    display: flex;
    white-space: pre;
    line-height: 1.5;
  }
  .dline .ln {
    flex-shrink: 0;
    width: 3em;
    padding-right: var(--space-1);
    text-align: right;
    color: var(--color-text-muted);
    user-select: none;
  }
  .dline .dt { overflow: hidden; }
  .dline.add { background: color-mix(in oklch, var(--color-success) 12%, transparent); }
  .dline.del { background: color-mix(in oklch, var(--color-error) 12%, transparent); }

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
