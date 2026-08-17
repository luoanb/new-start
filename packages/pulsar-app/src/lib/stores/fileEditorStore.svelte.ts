/**
 * 文件编辑器共享状态（模块级 $state 单例，对齐 LayoutStore/dataStore 范式）。
 *
 * 职责：
 * - 按「文件实例 key」维护每个已打开文件编辑器的元数据（所属工作区、相对路径、
 *   未保存标记、打开时 mtime 快照），供 EditorTabs 渲染文件名/● 标记与 +page 关闭确认。
 * - 编辑器内容本身由各 FileEditor 组件（CodeMirror 实例）持有，不入本 store；
 *   组件通过 ViewHost 注入的 `pulsar:panel` context 获取自身 panel.id（= 实例 key）。
 *
 * 实例 key = `${workspaceId}:${relPath}`：工作区 id 前缀避免不同工作区同名文件撞 key。
 */
export type FileEditorEntry = {
  workspaceId: string;
  /** 相对 active workspace 根（`/` 分隔，无前导斜杠）。 */
  relPath: string;
  /** 未保存标记（tab ●）。 */
  dirty: boolean;
  /** 打开/保存时的 mtime 快照（毫秒），保存冲突检测用。 */
  mtimeMs: number | null;
};

const editors = $state<Map<string, FileEditorEntry>>(new Map());

/** 构造文件实例 key（panel id 同名）。 */
export function fileKey(workspaceId: string, relPath: string): string {
  return `${workspaceId}:${relPath}`;
}

/** 从实例 key 解析相对路径（截掉 `${workspaceId}:` 前缀）。 */
export function relPathOfKey(key: string): string {
  const idx = key.indexOf(":");
  return idx >= 0 ? key.slice(idx + 1) : key;
}

export const fileEditorStore = {
  state: editors,

  get(key: string): FileEditorEntry | undefined {
    return editors.get(key);
  },

  /** 打开文件：先注册元数据（未注册即新开，复用已开实例）。 */
  open(key: string, workspaceId: string, relPath: string, mtimeMs: number | null): void {
    if (!editors.has(key)) {
      editors.set(key, { workspaceId, relPath, dirty: false, mtimeMs });
    }
  },

  markDirty(key: string, dirty: boolean): void {
    const e = editors.get(key);
    if (e) e.dirty = dirty;
  },

  setMtime(key: string, mtimeMs: number): void {
    const e = editors.get(key);
    if (e) e.mtimeMs = mtimeMs;
  },

  /** 关闭面板时释放实例元数据。 */
  dispose(key: string): void {
    editors.delete(key);
  },

  isDirty(key: string): boolean {
    return editors.get(key)?.dirty ?? false;
  },

  /** tab 标题：文件名（basename）。 */
  titleOf(key: string): string {
    const rel = this.pathOf(key);
    if (!rel) return "";
    const segs = rel.split("/");
    return segs[segs.length - 1] || rel;
  },

  /** tab tooltip：相对工作区根路径。 */
  pathOf(key: string): string {
    return relPathOfKey(key);
  },

  workspaceOf(key: string): string {
    return editors.get(key)?.workspaceId ?? "";
  },
};
