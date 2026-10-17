// ── Layout state types & defaults ──
//
// 本包为功能无关的应用骨架：只保留布局容器机制（可移动视图 / 主区分栏 / 折叠 /
// 持久化 / 版本迁移），不携带任何业务视图 id。新增视图 = 在 `views.ts` 注册一条。

export type SplitOrientation = "horizontal" | "vertical";

/** 可承载可移动视图的容器。main 是编辑器区域，有独立 split 语义，不参与视图拖拽。 */
export type ViewContainerId = "sidebar" | "info" | "panel";

/** main 区可插入的面板类型。骨架内只注册示例类型；
 * 新增类型 = 此处加一个 id + `views.ts` 的 `mainViews` / `mainPanelMeta` 各加一条。 */
export type MainPanelType = "example";

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

/** 布局快照版本号：结构变更时 +1，并在 `layoutStorage.ts` 的迁移链补一支。 */
export const LAYOUT_VERSION = 1;

export type LayoutState = {
  version: typeof LAYOUT_VERSION;
  sidebar: { visible: boolean; width: number };
  info: { visible: boolean; width: number };
  panel: { visible: boolean; height: number };
  /** 视图容器（可移动视图的归属与顺序）。main 是编辑器区域，不在此列。 */
  containers: Record<ViewContainerId, ViewContainerState>;
  /** 被显式隐藏（从所有容器移除）的视图 id。 */
  hiddenViews: string[];
  /** main 区：分栏列表（按用户操作插入/关闭，默认空）+ 激活分栏。 */
  main: { panes: MainPane[]; activePaneId: string | null };
  /** 侧栏/信息栏激活（ActivityBar 高亮）。 */
  activity: { active: string | null };
};

export const DEFAULT_LAYOUT: LayoutState = {
  version: LAYOUT_VERSION,
  sidebar: { visible: true, width: 260 },
  info: { visible: true, width: 280 },
  panel: { visible: true, height: 200 },
  // 骨架默认：侧栏挂示例视图，底部面板挂日志面板，信息栏留空演示「拖入视图」空态。
  containers: {
    sidebar: { views: ["example"], activeView: "example" },
    info: { views: [], activeView: "" },
    panel: { views: ["logs"], activeView: "logs" },
  },
  hiddenViews: [],
  // main 区默认空，面板全部由用户交互插入
  main: { panes: [], activePaneId: null },
  activity: { active: null },
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
