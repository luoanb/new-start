<script lang="ts">
  // FileExplorer：sidebar「文件」视图（VSCode 资源管理器语义）。
  // - 工作区选择器：下拉切换 active + 添加（系统对话框 / 输入回退）+ 操作（编辑过滤、删除）
  // - 懒加载树：按需 fs_list，ignore 规则由后端按工作区应用；workspaces 事件自动刷新
  // - 编辑交互（用户确认）：右键菜单 + 快捷键（F2 重命名 / Delete 删除）+ 顶部工具条（新建文件/文件夹/刷新）
  // - 移动：HTML5 拖拽到目标目录；右键菜单「移动…」备选（目录选择器）
  // - pad/移动端：条目右侧 ⋮ 按钮触发同一右键菜单
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, isTauriEnv } from "$lib/api";
  import { t } from "$lib/i18n";
  import { formatInvokeError } from "$lib/utils/formatInvokeError";
  import { dataStore } from "$lib/stores/dataStore.svelte";
  import { layoutStore } from "$lib/layout/LayoutStore.svelte";
  import { fileEditorStore, fileKey } from "$lib/stores/fileEditorStore.svelte";
  import ContextMenu, { type ContextMenuItem } from "./ContextMenu.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Select from "./Select.svelte";
  import SuggestInput, { type SuggestItem } from "./SuggestInput.svelte";
  import type { FsEntry, WorkspaceEntry } from "$lib/types";

  // ── 派生：active 工作区 ──
  let wsView = $derived(dataStore.state.workspaces);
  let workspaces = $derived(wsView?.workspaces ?? []);
  let activeId = $derived(wsView?.active_id ?? null);
  let activeWs = $derived(workspaces.find((w) => w.id === activeId) ?? null);

  // ── 树状态：dirPath → 加载结果（"" = 根）──
  type DirState =
    | { status: "loading"; entries: FsEntry[] }
    | { status: "loaded"; entries: FsEntry[] }
    | { status: "error"; entries: FsEntry[]; error: string };
  let dirs = $state<Record<string, DirState>>({});
  let expanded = $state<Record<string, boolean>>({});

  // ── 选中 / 编辑 ──
  let selectedPath = $state<string | null>(null);
  let selectedKind = $state<"file" | "dir" | null>(null);
  type EditState =
    | { mode: "new-file"; parent: string; path: string }
    | { mode: "new-folder"; parent: string; path: string }
    | { mode: "rename"; parent: string; path: string };
  let editing = $state<EditState | null>(null);
  let editValue = $state("");

  // ── 弹层 ──
  let menu = $state<{ items: ContextMenuItem[]; x: number; y: number } | null>(null);
  let wsInputOpen = $state(false);
  let wsInput = $state("");
  let ignoreEdit = $state<{ ws: WorkspaceEntry; text: string } | null>(null);
  let wsDeleteReq = $state<WorkspaceEntry | null>(null);
  let moveReq = $state<{ from: string; name: string } | null>(null);
  let movePath = $state("");
  let moveDirs = $state<FsEntry[]>([]);
  let moveLoading = $state(false);
  let moveError = $state("");
  let error = $state("");
  let copiedFlash = $state<string | null>(null);
  let dropTarget = $state<string | null>(null);

  // ── 加载目录 ──
  function sortEntries(list: FsEntry[]): FsEntry[] {
    return [...list].sort((a, b) =>
      a.is_dir === b.is_dir ? a.name.localeCompare(b.name) : a.is_dir ? -1 : 1
    );
  }

  async function loadDir(path: string, force = false): Promise<void> {
    const cur = dirs[path];
    if (!force && cur?.status === "loaded") return;
    dirs[path] = { status: "loading", entries: [] };
    try {
      const list = await api.invoke<FsEntry[]>("fs_list", { path: path || undefined });
      console.log("[FileExplorer] fs_list ok", { path, count: list.length });
      dirs[path] = { status: "loaded", entries: sortEntries(list) };
    } catch (e) {
      console.error("[FileExplorer] fs_list failed", { path }, e);
      dirs[path] = { status: "error", entries: [], error: formatInvokeError(e) };
    }
  }

  // ── 响应后端状态（workspaces 事件：增删/切换/ignore 编辑/fs 写操作）──
  let lastVersion = $state(-1);
  let lastActiveId = $state<string | null>(null);

  $effect(() => {
    const v = dataStore.state.workspacesVersion;
    const aid = dataStore.state.workspaces?.active_id ?? null;
    if (v === lastVersion && aid === lastActiveId) return;
    const switched = aid !== lastActiveId;
    lastVersion = v;
    lastActiveId = aid;
    console.log("[FileExplorer] workspaces effect", { v, aid, switched, wsCount: workspaces.length, dirsLoaded: Object.keys(dirs).length });
    if (switched) {
      // 工作区切换/首载：全量重置（展开与选中清空）
      dirs = {};
      expanded = {};
      selectedPath = null;
      selectedKind = null;
      editing = null;
      dropTarget = null;
      if (aid) void loadDir("");
    } else {
      // 同工作区（写操作/ignore 编辑）：重载已加载目录，保留展开
      for (const key of Object.keys(dirs)) void loadDir(key, true);
    }
  });

  // ── 树扁平化行（含编辑输入行 / 加载与错误提示行）──
  type Row =
    | { key: string; kind: "entry"; depth: number; entry: FsEntry; parent: string }
    | { key: string; kind: "input"; depth: number }
    | { key: string; kind: "hint"; depth: number; text: string };

  let rows = $derived.by(() => {
    const out: Row[] = [];
    const walk = (dirPath: string, depth: number) => {
      const st = dirs[dirPath];
      if (!st) return;
      if (st.status === "error") {
        out.push({ key: `err:${dirPath}`, kind: "hint", depth, text: st.error });
        return;
      }
      for (const e of st.entries) {
        out.push({ key: `e:${e.path}`, kind: "entry", depth, entry: e, parent: dirPath });
        if (e.is_dir && expanded[e.path]) {
          const child = dirs[e.path];
          if (child?.status === "loading") {
            out.push({ key: `load:${e.path}`, kind: "hint", depth: depth + 1, text: t("fileExplorer.loading") });
          } else if (!child) {
            void loadDir(e.path); // 展开但未加载（如跨事件恢复）→ 触发加载
          } else {
            walk(e.path, depth + 1);
          }
        }
      }
      if (editing && editing.parent === dirPath && editing.mode !== "rename") {
        out.push({ key: `new:${dirPath}`, kind: "input", depth: depth + 1 });
      }
    };
    walk("", 0);
    return out;
  });

  let rootLoading = $derived(dirs[""]?.status === "loading");

  // ── 节点交互 ──
  function onRowClick(e: FsEntry) {
    selectedPath = e.path;
    selectedKind = e.is_dir ? "dir" : "file";
    if (e.is_dir) {
      toggleDir(e.path);
    } else if (activeWs) {
      openFile(activeWs, e.path);
    }
  }

  function toggleDir(path: string) {
    expanded[path] = !expanded[path];
    if (expanded[path]) void loadDir(path);
  }

  function openFile(ws: WorkspaceEntry, path: string) {
    const key = fileKey(ws.id, path);
    console.log("[FileExplorer] openFile", { wsId: ws.id, path, key });
    fileEditorStore.open(key, ws.id, path, null);
    layoutStore.insertPanel("file-editor", undefined, key);
  }

  // ── 菜单 ──
  const ICONS = {
    file: '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>',
    folder: '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>',
    rename: '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17 3a2.83 2.83 0 0 1 4 4L7.5 20.5 2 22l1.5-5.5Z"/><path d="m15 5 4 4"/></svg>',
    trash: '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>',
    move: '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="5 9 2 12 5 15"/><polyline points="9 5 12 2 15 5"/><polyline points="15 19 12 22 9 19"/><polyline points="19 9 22 12 19 15"/><line x1="2" y1="12" x2="22" y2="12"/><line x1="12" y1="2" x2="12" y2="22"/></svg>',
    copy: '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>',
    refresh: '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/></svg>',
    plus: '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>',
  };

  function openMenuAt(items: ContextMenuItem[], e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    menu = { items, x: e.clientX, y: e.clientY };
  }

  function onRowContext(e: MouseEvent, entry: FsEntry) {
    selectedPath = entry.path;
    selectedKind = entry.is_dir ? "dir" : "file";
    const items: ContextMenuItem[] = [];
    if (entry.is_dir) {
      items.push(
        { label: t("fileExplorer.newFile"), icon: ICONS.plus, onSelect: () => startNew("new-file", entry.path) },
        { label: t("fileExplorer.newFolder"), icon: ICONS.folder, onSelect: () => startNew("new-folder", entry.path) },
        { label: t("fileExplorer.rename"), icon: ICONS.rename, onSelect: () => startRename(entry.path) },
        { label: t("fileExplorer.move"), icon: ICONS.move, onSelect: () => startMove(entry.path) },
        { label: t("fileExplorer.copyPath"), icon: ICONS.copy, onSelect: () => void copyPath(entry.path) },
        { label: t("fileExplorer.delete"), icon: ICONS.trash, danger: true, onSelect: () => void deletePath(entry.path) },
      );
    } else {
      items.push(
        { label: t("fileExplorer.open"), icon: ICONS.file, onSelect: () => activeWs && openFile(activeWs, entry.path) },
        { label: t("fileExplorer.rename"), icon: ICONS.rename, onSelect: () => startRename(entry.path) },
        { label: t("fileExplorer.move"), icon: ICONS.move, onSelect: () => startMove(entry.path) },
        { label: t("fileExplorer.copyPath"), icon: ICONS.copy, onSelect: () => void copyPath(entry.path) },
        { label: t("fileExplorer.delete"), icon: ICONS.trash, danger: true, onSelect: () => void deletePath(entry.path) },
      );
    }
    openMenuAt(items, e);
  }

  /** pad/移动端：条目右侧 ⋮ 按钮（无右键场景）。 */
  function onRowMore(e: MouseEvent, entry: FsEntry) {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const target = { x: rect.right - 180, y: rect.bottom + 4 };
    selectedPath = entry.path;
    selectedKind = entry.is_dir ? "dir" : "file";
    const items: ContextMenuItem[] = [];
    if (entry.is_dir) {
      items.push(
        { label: t("fileExplorer.newFile"), onSelect: () => startNew("new-file", entry.path) },
        { label: t("fileExplorer.newFolder"), onSelect: () => startNew("new-folder", entry.path) },
        { label: t("fileExplorer.rename"), onSelect: () => startRename(entry.path) },
        { label: t("fileExplorer.move"), onSelect: () => startMove(entry.path) },
        { label: t("fileExplorer.delete"), danger: true, onSelect: () => void deletePath(entry.path) },
      );
    } else {
      items.push(
        { label: t("fileExplorer.open"), onSelect: () => activeWs && openFile(activeWs, entry.path) },
        { label: t("fileExplorer.rename"), onSelect: () => startRename(entry.path) },
        { label: t("fileExplorer.move"), onSelect: () => startMove(entry.path) },
        { label: t("fileExplorer.delete"), danger: true, onSelect: () => void deletePath(entry.path) },
      );
    }
    e.stopPropagation();
    menu = { items, x: target.x, y: target.y };
  }

  /** 空白区 / 树背景右键：根级操作。 */
  function onBlankMenu(e: MouseEvent) {
    selectedPath = null;
    selectedKind = null;
    openMenuAt(
      [
        { label: t("fileExplorer.newFile"), icon: ICONS.plus, onSelect: () => startNew("new-file", "") },
        { label: t("fileExplorer.newFolder"), icon: ICONS.folder, onSelect: () => startNew("new-folder", "") },
        { label: t("fileExplorer.refresh"), icon: ICONS.refresh, onSelect: refresh },
        { label: t("fileExplorer.addWorkspace"), icon: ICONS.plus, onSelect: startAddWorkspace },
      ],
      e,
    );
  }

  /** 工作区栏 ⋯ 菜单。 */
  function onWsMenu(e: MouseEvent) {
    if (!activeWs) return;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = {
      items: [
        { label: t("fileExplorer.editIgnore"), icon: ICONS.rename, onSelect: () => (ignoreEdit = { ws: activeWs, text: activeWs.ignore.join("\n") }) },
        { label: t("fileExplorer.deleteWorkspace"), icon: ICONS.trash, danger: true, onSelect: () => (wsDeleteReq = activeWs) },
      ],
      x: rect.right - 190,
      y: rect.bottom + 4,
    };
    e.stopPropagation();
  }

  // ── 工具条 / 新建 ──
  function refresh() {
    error = "";
    for (const key of Object.keys(dirs)) void loadDir(key, true);
  }

  function startNew(mode: "new-file" | "new-folder", parent: string) {
    editing = { mode, parent, path: "" };
    editValue = "";
    if (parent) {
      expanded[parent] = true;
      void loadDir(parent);
    }
  }

  function toolbarNew(mode: "new-file" | "new-folder") {
    const parent = selectedKind === "dir" && selectedPath ? selectedPath : "";
    startNew(mode, parent);
  }

  function startRename(path: string) {
    const parent = path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
    editing = { mode: "rename", parent, path };
    editValue = path.split("/").pop() ?? "";
  }

  function onEditKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      void commitEditing();
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancelEditing();
    }
  }

  function cancelEditing() {
    editing = null;
    editValue = "";
  }

  function validateName(name: string): boolean {
    return !!name && !name.includes("/") && name !== "." && name !== ".." && !name.includes("\\");
  }

  async function commitEditing() {
    if (!editing) return;
    const name = editValue.trim();
    if (!validateName(name)) {
      error = t("fileExplorer.invalidName");
      return;
    }
    error = "";
    const target = editing.parent ? `${editing.parent}/${name}` : name;
    try {
      if (editing.mode === "new-folder") {
        await api.invoke("fs_create_dir", { path: target });
      } else if (editing.mode === "new-file") {
        await api.invoke("fs_write", { path: target, content: "" });
      } else {
        await api.invoke("fs_rename", { from: editing.path, to: target });
        selectedPath = target;
      }
      editing = null;
      editValue = "";
    } catch (e) {
      error = t("fileExplorer.operationFailed", { error: formatInvokeError(e) });
    }
  }

  // ── 删除（直接删，无确认——用户确认）──
  async function deletePath(path: string) {
    try {
      await api.invoke("fs_delete", { paths: [path] });
      if (selectedPath === path) {
        selectedPath = null;
        selectedKind = null;
      }
    } catch (e) {
      error = t("fileExplorer.operationFailed", { error: formatInvokeError(e) });
    }
  }

  // ── 键盘（F2 重命名 / Delete 删除 / Enter 打开或展开）──
  function onTreeKeydown(e: KeyboardEvent) {
    if (menu) return;
    if (!selectedPath) return;
    if (e.key === "F2") {
      e.preventDefault();
      startRename(selectedPath);
    } else if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      void deletePath(selectedPath);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (selectedKind === "dir") toggleDir(selectedPath);
      else if (activeWs) openFile(activeWs, selectedPath);
    }
  }

  // ── 复制路径 ──
  async function copyText(text: string): Promise<boolean> {
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(text);
        return true;
      }
    } catch {
      // fallthrough → execCommand 回退
    }
    try {
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      const ok = document.execCommand("copy");
      ta.remove();
      return ok;
    } catch {
      return false;
    }
  }

  async function copyPath(path: string) {
    if (!activeWs) return;
    const abs = path ? `${activeWs.root}/${path}` : activeWs.root;
    const ok = await copyText(abs);
    if (ok) {
      copiedFlash = abs;
      setTimeout(() => {
        if (copiedFlash === abs) copiedFlash = null;
      }, 1600);
    } else {
      error = t("fileExplorer.operationFailed", { error: "copy" });
    }
  }

  // ── 添加工作区：系统对话框 + 输入回退 ──
  function startAddWorkspace() {
    wsInputOpen = true;
    wsInput = "";
    void initWsInput();
  }

  /** 打开时默认填入用户主目录（App 模式也保留输入入口）；下拉由组件聚焦时自动拉取。 */
  async function initWsInput() {
    try {
      const home = await api.invoke<string>("get_home_dir");
      wsInput = home.endsWith("/") ? home : home + "/";
    } catch {
      wsInput = "";
    }
  }

  async function pickWorkspaceDir() {
    try {
      const dir = await open({ directory: true, multiple: false });
      if (typeof dir === "string" && dir) await dataStore.addWorkspace(dir);
    } catch {
      // 对话框不可用（异常/降级环境）→ 输入回退
      wsInputOpen = true;
      void initWsInput();
    }
  }

  async function commitWsInput() {
    const root = wsInput.trim();
    if (!root) return;
    try {
      await dataStore.addWorkspace(root);
      wsInputOpen = false;
      wsInput = "";
      error = "";
    } catch (e) {
      error = t("fileExplorer.operationFailed", { error: formatInvokeError(e) });
    }
  }

  // ── 路径补全（注入给 SuggestInput 的建议拉取）：列当前目录直接子项 + 前缀过滤（含 `.` / `..`）──

  /** 拆分输入为「待列出目录 + 过滤前缀」（输入以 / 结尾视为纯目录）。 */
  function splitPathInput(input: string): { parent: string; prefix: string } {
    const hasTrailing = input.endsWith("/");
    const lastSlash = input.lastIndexOf("/");
    const parent = hasTrailing
      ? input.slice(0, -1)
      : lastSlash <= 0
        ? "/"
        : input.slice(0, lastSlash);
    const prefix = hasTrailing ? "" : input.slice(lastSlash + 1);
    return { parent: parent || "/", prefix };
  }

  async function fetchWsSuggest(input: string): Promise<SuggestItem[]> {
    const text = input.trim();
    if (!text.startsWith("/")) return [];
    const { parent } = splitPathInput(text);
    // 目标目录不存在/无权限时逐级向父目录回退，回退后过滤前缀变宽
    let entries: FsEntry[] | null = null;
    let dir = parent;
    while (dir) {
      try {
        entries = await api.invoke<FsEntry[]>("fs_suggest_abs", { path: dir });
        break;
      } catch {
        if (dir === "/") break;
        dir = dir.slice(0, dir.lastIndexOf("/")) || "/";
      }
    }
    if (!entries) return [];
    // 过滤 key：text 中 dir 之后的部分，取第一段（跨层输入回退后仍能匹配）
    const rest = dir === "/" ? text.slice(1) : text.slice(dir.length + 1);
    const key = rest.split("/")[0].toLowerCase();
    const dirs = entries.filter(
      (e) => e.is_dir && e.name.toLowerCase().startsWith(key),
    );
    const files = entries.filter(
      (e) => !e.is_dir && e.name.toLowerCase().startsWith(key),
    );
    const upPath = dir === "/" ? "/" : dir.slice(0, dir.lastIndexOf("/")) || "/";
    // 工作区根须为目录：目录候选以 / 结尾（展示 + 填入），文件候选原样
    const dirLabel = (p: string) => (p.endsWith("/") ? p : p + "/");
    return [
      { label: dirLabel(upPath), value: dirLabel(upPath), expand: true },
      { label: dirLabel(dir), value: dirLabel(dir), expand: true },
      ...dirs.map((e) => ({ label: dirLabel(e.path), value: dirLabel(e.path), expand: true })),
      ...files.map((e) => ({ label: e.path, value: e.path })),
    ];
  }

  async function saveIgnore() {
    if (!ignoreEdit) return;
    const lines = ignoreEdit.text
      .split("\n")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    try {
      await dataStore.updateWorkspaceIgnore(ignoreEdit.ws.id, lines);
      ignoreEdit = null;
    } catch (e) {
      error = t("fileExplorer.operationFailed", { error: formatInvokeError(e) });
    }
  }

  // ── 移动：拖拽 + 右键「移动…」备选 ──
  let dragFrom = $state<string | null>(null);

  function onDragStart(e: DragEvent, path: string) {
    dragFrom = path;
    e.dataTransfer?.setData("text/plain", path);
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
  }

  function onRowDragOver(e: DragEvent, path: string) {
    e.preventDefault();
    dropTarget = path;
  }

  function onRowDrop(e: DragEvent, target: string) {
    e.preventDefault();
    e.stopPropagation();
    dropTarget = null;
    const from = e.dataTransfer?.getData("text/plain") || dragFrom;
    if (!from) return;
    void moveInto(from, target);
  }

  function onTreeDragOver(e: DragEvent) {
    if (dragFrom) e.preventDefault(); // 允许拖到根（空白区）
  }

  function onTreeDrop(e: DragEvent) {
    e.preventDefault();
    const from = e.dataTransfer?.getData("text/plain") || dragFrom;
    dropTarget = null;
    if (!from) return;
    void moveInto(from, "");
  }

  /** 目标为目录相对路径（"" = 根）；目标自动拼上原名。 */
  async function moveInto(from: string, target: string) {
    const name = from.split("/").pop() ?? from;
    const to = target === "" ? name : `${target}/${name}`;
    if (to === from || to.startsWith(from + "/")) {
      error = t("fileExplorer.operationFailed", { error: "move" });
      return;
    }
    dragFrom = null;
    try {
      await api.invoke("fs_move", { from, to });
      error = "";
    } catch (e) {
      error = t("fileExplorer.operationFailed", { error: formatInvokeError(e) });
    }
  }

  function startMove(path: string) {
    moveReq = { from: path, name: path.split("/").pop() ?? path };
    movePath = "";
    moveDirs = [];
    moveError = "";
    void loadMoveLevel("");
  }

  async function loadMoveLevel(dirPath: string) {
    moveLoading = true;
    moveError = "";
    movePath = dirPath;
    try {
      const list = await api.invoke<FsEntry[]>("fs_list", { path: dirPath || undefined });
      moveDirs = sortEntries(list).filter((e) => e.is_dir);
    } catch (e) {
      moveError = formatInvokeError(e);
    } finally {
      moveLoading = false;
    }
  }

  function moveToSegment(seg: string) {
    void loadMoveLevel(seg);
  }

  async function confirmMove() {
    if (!moveReq) return;
    const to = movePath === "" ? moveReq.name : `${movePath}/${moveReq.name}`;
    if (to === moveReq.from || to.startsWith(moveReq.from + "/")) {
      moveError = t("fileExplorer.operationFailed", { error: "move" });
      return;
    }
    try {
      await api.invoke("fs_move", { from: moveReq.from, to });
      moveReq = null;
      error = "";
    } catch (e) {
      moveError = t("fileExplorer.operationFailed", { error: formatInvokeError(e) });
    }
  }

  function moveSegments(): string[] {
    return movePath ? movePath.split("/") : [];
  }

  // ── 输入框自动聚焦 ──
  function focusInput(el: HTMLInputElement) {
    el.focus();
    el.select();
  }
</script>

<div class="file-explorer">
  <!-- 工作区栏 -->
  <div class="ws-bar">
    {#if workspaces.length > 0}
      <Select
        class="ws-select"
        value={activeId ?? ""}
        options={workspaces.map((ws) => ({ value: ws.id, label: ws.name }))}
        onchange={(v) => dataStore.setActiveWorkspace(String(v))}
      />
    {:else}
      <span class="no-ws">{t("fileExplorer.noWorkspace")}</span>
    {/if}
    <div class="ws-actions">
      <button class="icon-btn" title={t("fileExplorer.addWorkspace")} onclick={startAddWorkspace}>＋</button>
      {#if activeWs}
        <button class="icon-btn" title={t("fileExplorer.workspaceActions")} onclick={onWsMenu}>⋯</button>
      {/if}
    </div>
  </div>

  {#if wsInputOpen}
    <div class="ws-input-row">
      <SuggestInput
        bind:value={wsInput}
        placeholder={t("fileExplorer.addWorkspaceInputPlaceholder")}
        fetchSuggest={fetchWsSuggest}
        dropdownWidth={220}
        onsubmit={() => void commitWsInput()}
        onclose={() => (wsInputOpen = false)}
      />
      {#if isTauriEnv}
        <button class="btn btn-sm" onclick={() => void pickWorkspaceDir()}>{t("fileExplorer.addWorkspaceBrowse")}</button>
      {/if}
      <button class="btn btn-sm btn-primary" onclick={() => void commitWsInput()}>{t("fileExplorer.addWorkspaceInputConfirm")}</button>
      <button class="btn btn-sm" onclick={() => (wsInputOpen = false)}>✕</button>
    </div>
  {/if}

  <!-- 树工具条：新建文件 / 新建文件夹 / 刷新 -->
  <div class="tree-bar">
    <button class="icon-btn" title={t("fileExplorer.refresh")} onclick={refresh} aria-label={t("fileExplorer.refresh")}>↻</button>
    <button class="icon-btn" title={t("fileExplorer.newFile")} onclick={() => toolbarNew("new-file")} aria-label={t("fileExplorer.newFile")}>＋</button>
    <button class="tool-btn" onclick={() => toolbarNew("new-folder")}>{t("fileExplorer.newFolder")}</button>
  </div>

  <!-- 文件树 -->
  <div
    class="tree"
    tabindex="0"
    role="tree"
    onkeydown={onTreeKeydown}
    oncontextmenu={onBlankMenu}
    ondragover={onTreeDragOver}
    ondrop={onTreeDrop}
  >
    {#if !activeWs}
      <p class="hint-empty">{t("fileExplorer.noWorkspace")}</p>
    {:else if rootLoading}
      <p class="hint-empty">{t("fileExplorer.loading")}</p>
    {:else}
      {#each rows as row (row.key)}
        {#if row.kind === "entry"}
          {@const e = row.entry}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            class="row"
            class:selected={selectedPath === e.path}
            class:drop-target={dropTarget === e.path}
            style="padding-left:{10 + row.depth * 14}px"
            draggable
            tabindex="-1"
            aria-selected={selectedPath === e.path}
            ondragstart={(ev) => onDragStart(ev, e.path)}
            ondragover={(ev) => e.is_dir && onRowDragOver(ev, e.path)}
            ondrop={(ev) => e.is_dir && onRowDrop(ev, e.path)}
            onclick={() => onRowClick(e)}
            oncontextmenu={(ev) => onRowContext(ev, e)}
            role="treeitem"
          >
            <span class="chevron" class:open={expanded[e.path]}>
              {#if e.is_dir}
                <svg viewBox="0 0 16 16" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 4 10 8 6 12"/></svg>
              {/if}
            </span>
            <span class="node-icon">
              {@html e.is_dir ? ICONS.folder : ICONS.file}
            </span>
            {#if editing?.mode === "rename" && editing.path === e.path}
              <input
                class="row-input"
                bind:value={editValue}
                onkeydown={onEditKeydown}
                oncontextmenu={(ev) => ev.stopPropagation()}
                onclick={(ev) => ev.stopPropagation()}
                use:focusInput
              />
            {:else}
              <span class="name" title={e.path}>{e.name}</span>
            {/if}
            <button
              class="row-more icon-btn"
              title={t("fileExplorer.workspaceActions")}
              aria-label={t("fileExplorer.workspaceActions")}
              onclick={(ev) => onRowMore(ev, e)}
            >⋯</button>
          </div>
        {:else if row.kind === "input"}
          <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
          <div class="row input-row" style="padding-left:{10 + row.depth * 14}px" onclick={(ev) => ev.stopPropagation()}>
            <span class="node-icon">{@html editing?.mode === "new-folder" ? ICONS.folder : ICONS.file}</span>
            <input class="row-input" bind:value={editValue} onkeydown={onEditKeydown} use:focusInput />
            <button class="btn btn-sm btn-primary" onclick={() => void commitEditing()}>✓</button>
            <button class="btn btn-sm" onclick={cancelEditing}>✕</button>
          </div>
        {:else}
          <p class="row hint" style="padding-left:{10 + row.depth * 14}px">{row.text}</p>
        {/if}
      {/each}
      {#if rows.length === 0 && !editing}
        <p class="hint-empty">{t("fileExplorer.empty")}</p>
      {/if}
    {/if}
  </div>

  {#if error}
    <p class="error-bar">{error}</p>
  {/if}
  {#if copiedFlash}
    <p class="copied-flash">{t("fileExplorer.copied")}: {copiedFlash}</p>
  {/if}

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

  {#if moveReq}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="overlay" role="presentation" onclick={() => (moveReq = null)}>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="modal" onclick={(e) => e.stopPropagation()}>
        <div class="modal-header"><h2>{t("fileExplorer.move")}</h2></div>
        <div class="modal-body">
          <p class="modal-hint">{t("fileExplorer.moveTargetHint")}</p>
          <div class="crumb">
            <button class="crumb-btn" class:active={movePath === ""} onclick={() => moveToSegment("")}>/</button>
            {#each moveSegments() as seg, i}
              <span class="crumb-sep">/</span>
              <button class="crumb-btn" onclick={() => moveToSegment(moveSegments().slice(0, i + 1).join("/"))}>{seg}</button>
            {/each}
          </div>
          <div class="move-list">
            {#if moveLoading}
              <p class="hint-empty">{t("fileExplorer.loading")}</p>
            {:else if moveDirs.length === 0}
              <p class="hint-empty">{t("fileExplorer.empty")}</p>
            {:else}
              {#each moveDirs as d (d.path)}
                <button class="move-item" onclick={() => moveToSegment(d.path)}>
                  <span class="node-icon">{@html ICONS.folder}</span>
                  <span class="name">{d.name}</span>
                </button>
              {/each}
            {/if}
          </div>
          {#if moveError}<p class="error-bar">{moveError}</p>{/if}
        </div>
        <div class="modal-footer">
          <button class="btn" onclick={() => (moveReq = null)}>{t("fileExplorer.ignoreCancel")}</button>
          <button class="btn btn-primary" onclick={() => void confirmMove()}>{t("fileExplorer.moveConfirm")}</button>
        </div>
      </div>
    </div>
  {/if}

  {#if wsDeleteReq}
    <ConfirmDialog
      open
      danger
      title={t("fileExplorer.deleteWorkspace")}
      message={t("fileExplorer.deleteWorkspaceConfirm")}
      confirmLabel={t("fileExplorer.delete")}
      onConfirm={() => {
        const ws = wsDeleteReq;
        wsDeleteReq = null;
        if (ws) void dataStore.removeWorkspace(ws.id);
      }}
      onCancel={() => (wsDeleteReq = null)}
    />
  {/if}
</div>

<style>
  .file-explorer {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--color-surface);
    font-size: var(--fs-sm);
  }

  /* ── 工作区栏 ── */
  .ws-bar {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1) var(--space-2);
    border-bottom: var(--border-width) solid var(--color-border);
    flex-shrink: 0;
  }
  .ws-bar :global(.ws-select) {
    flex: 1;
    min-width: 0;
  }
  .no-ws {
    flex: 1;
    font-size: var(--fs-sm);
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ws-actions {
    display: flex;
    gap: 2px;
    flex-shrink: 0;
  }
  .icon-btn {
    border: none;
    background: transparent;
    line-height: 1;
    padding: 3px 6px;
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: var(--fs-base);
  }
  .icon-btn:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .ws-input-row {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1) var(--space-2);
    border-bottom: var(--border-width) solid var(--color-border);
  }
  .row-input {
    flex: 1;
    min-width: 0;
    padding: 2px 6px;
    font-size: var(--fs-sm);
    background: var(--color-elevated);
    color: var(--color-text);
    border: 1px solid var(--color-primary);
    border-radius: var(--radius-sm);
    outline: none;
  }

  /* ── 树工具条 ── */
  .tree-bar {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 2px var(--space-2);
    border-bottom: var(--border-width) solid var(--color-border);
    flex-shrink: 0;
  }
  .tool-btn {
    border: none;
    background: transparent;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    padding: 3px 6px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .tool-btn:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  /* ── 树 ── */
  .tree {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    outline: none;
    padding: var(--space-1) 0;
  }
  .tree:focus-visible {
    box-shadow: inset 0 0 0 1px var(--color-primary);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: 3px var(--space-2) 3px 0;
    cursor: pointer;
    border-radius: var(--radius-sm);
    user-select: none;
    -webkit-user-select: none;
    white-space: nowrap;
  }
  .row:hover {
    background: var(--color-hover);
  }
  .row.selected {
    background: color-mix(in oklch, var(--color-primary) 18%, transparent);
  }
  .row.drop-target {
    outline: 1.5px dashed var(--color-primary);
    outline-offset: -1.5px;
  }
  .chevron {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    color: var(--color-text-muted);
    transition: transform var(--duration-fast) var(--ease-out);
  }
  .chevron.open {
    transform: rotate(90deg);
  }
  .node-icon {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    color: var(--color-text-muted);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--color-text);
    font-size: var(--fs-sm);
  }
  .row-more {
    opacity: 0;
    flex-shrink: 0;
  }
  .row:hover .row-more,
  .row.selected .row-more {
    opacity: 1;
  }
  /* pad/移动端：无 hover，⋮ 常显 */
  @media (hover: none) {
    .row-more {
      opacity: 1;
    }
  }
  .input-row {
    gap: var(--space-1);
  }
  .row.hint {
    cursor: default;
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
  }
  .hint-empty {
    padding: var(--space-3) var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--fs-sm);
  }

  .error-bar {
    padding: var(--space-1) var(--space-2);
    font-size: var(--fs-xs);
    color: var(--color-error);
    border-top: var(--border-width) solid var(--color-border);
    flex-shrink: 0;
    word-break: break-all;
  }
  .copied-flash {
    padding: var(--space-1) var(--space-2);
    font-size: var(--fs-xs);
    color: var(--color-success);
    flex-shrink: 0;
  }

  /* ── 浮层（对齐 ConfirmDialog 词汇）── */
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
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
  }
  .modal-header {
    padding: var(--space-3) var(--space-4);
    border-bottom: var(--border-width) solid var(--color-border);
    flex-shrink: 0;
  }
  .modal-header h2 {
    margin: 0;
    font-size: var(--fs-lg);
    font-weight: 600;
  }
  .modal-body {
    padding: var(--space-3) var(--space-4);
    overflow-y: auto;
    min-height: 0;
  }
  .modal-hint {
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    margin: 0 0 var(--space-2);
    line-height: 1.5;
  }
  .ignore-text {
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
    font-family: var(--font-mono, monospace);
    font-size: var(--fs-sm);
    background: var(--color-elevated);
    color: var(--color-text);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    padding: var(--space-2);
  }
  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4);
    border-top: var(--border-width) solid var(--color-border);
    flex-shrink: 0;
  }
  .crumb {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 2px;
    margin-bottom: var(--space-2);
  }
  .crumb-btn {
    border: none;
    background: transparent;
    color: var(--color-text);
    font-size: var(--fs-sm);
    padding: 2px 6px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .crumb-btn:hover {
    background: var(--color-hover);
  }
  .crumb-btn.active {
    color: var(--color-primary);
  }
  .crumb-sep {
    color: var(--color-text-muted);
  }
  .move-list {
    max-height: 240px;
    overflow-y: auto;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    padding: var(--space-1);
  }
  .move-item {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    width: 100%;
    padding: 4px var(--space-2);
    border: none;
    background: transparent;
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
  }
  .move-item:hover {
    background: var(--color-hover);
  }
</style>
