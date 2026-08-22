// ── Layout state types & defaults ──

export type SplitOrientation = "horizontal" | "vertical";

/** 可承载可移动视图的容器。main 是编辑器区域，有独立 split 语义，不参与视图拖拽。 */
export type ViewContainerId = "sidebar" | "info" | "panel";

/** main 区可插入的面板类型。多数类型全局唯一（多个会话共享同一个 chat 面板）；
 * `file-editor` 例外：按文件路径多实例（实例 id = 文件 key，见 LayoutStore.insertPanel）；
 * `git-diff` 复用同一实例语义（实例 id = `git-diff:${repoId}:${relPath}`，按文件路径多开）。 */
export type MainPanelType =
  | "chat"
  | "neurons"
  | "tool-editor"
  | "provider-manager"
  | "file-editor"
  | "git-diff"
  | "commit-diff";

/** main 区面板实例（insertPanel 返回其 id，供外部关闭）。 */
export type MainPanel = {
  id: string;
  type: MainPanelType;
};

/**
 * main 区分栏：可持有多个面板（同一分栏内 tab 切换），每个分栏有独立 tab 列表。
 * grow 为宽度权重（flex-grow），分栏之间由 Splitter 拖拽调整；
 * 分栏内面板全部关闭后分栏本身移除（空 main = panes 空数组）。
 */
export type MainPane = {
  id: string;
  grow: number;
  panels: MainPanel[];
  activePanelId: string | null;
};

/** 单个视图容器的状态：容器内视图顺序 + 激活视图。 */
export type ViewContainerState = {
  views: string[];
  activeView: string;
};

export type LayoutState = {
  version: 11;
  sidebar: { visible: boolean; width: number };
  info: { visible: boolean; width: number };
  panel: { visible: boolean; height: number };
  /** 视图容器（可移动视图的归属与顺序）。main 是编辑器区域，不在此列。 */
  containers: Record<ViewContainerId, ViewContainerState>;
  /** 被显式隐藏（从所有容器移除）的视图 id。 */
  hiddenViews: string[];
  /** main 区：分栏列表（按用户操作插入/关闭，默认空）+ 激活分栏。 */
  main: { panes: MainPane[]; activePaneId: string | null };
  /** 侧栏/信息栏激活（ActivityBar 高亮）。main 区显示不再由它驱动。 */
  activity: { active: string | null }; // "sessions" | "info"
};

export const DEFAULT_LAYOUT: LayoutState = {
  version: 11,
  sidebar: { visible: true, width: 260 },
  info: { visible: true, width: 280 },
  // v2/v3: 底部面板默认展开（对齐 VS Code 底部栏习惯）
  panel: { visible: true, height: 200 },
  containers: {
    // v5/v6: topics 曾默认在 panel/topics 位置演变；v7: topics 与 tools 默认归位左侧 sidebar
    // v10: 新增 files（文件管理）默认归位左侧 sidebar（sessions 之后，VSCode 资源管理器语义）
    // v11: 新增 terminal（集成终端面板）默认归位底部 panel（poller/logs 之后，VS Code 底部终端语义）
    sidebar: { views: ["sessions", "files", "topics", "tools"], activeView: "sessions" },
    // v10: providers+models 聚合为单个视图 providers-models（服务商分组 + 模型子项）
    info: { views: ["providers-models", "neurons-list"], activeView: "providers-models" },
    panel: { views: ["poller", "logs", "terminal"], activeView: "poller" },
  },
  hiddenViews: [],
  // v8: main 区默认空，面板全部由用户交互插入
  main: { panes: [], activePaneId: null },
  activity: { active: "sessions" },
};

/**
 * 归一化插入目标分栏：0~n-1 为既有栏索引；n（"new" 或越界收敛）表示新增一栏；
 * 非法值（负数/非数字/小数）收敛到 0 或 n。
 */
export function normalizePaneTarget(
  target: number | "new" | undefined,
  paneCount: number,
): number {
  if (target === undefined) return 0;
  if (target === "new") return paneCount;
  if (!Number.isFinite(target)) return 0;
  const t = Math.trunc(target);
  if (t <= 0) return 0;
  if (t > paneCount) return paneCount;
  return t;
}

// ── Bounds ──

export const BOUNDS = {
  sidebar: { min: 120, max: 400 },
  info: { min: 160, max: 480 },
  panel: { min: 100 },
  paneGrow: { min: 0.2, max: 8 },
} as const;

export function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}
