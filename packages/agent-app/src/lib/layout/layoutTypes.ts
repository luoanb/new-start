// ── Layout state types & defaults ──

export type SplitOrientation = "horizontal" | "vertical";

/** 可承载可移动视图的容器。main 是编辑器区域，有独立 split 语义，不参与视图拖拽。 */
export type ViewContainerId = "sidebar" | "info" | "panel";

export type MainSplit = {
  id: string; // "chat" | "neuron"
  orientation: SplitOrientation;
  ratio: number; // first pane ratio 0.3~0.7
};

/** 单个视图容器的状态：容器内视图顺序 + 激活视图。 */
export type ViewContainerState = {
  views: string[];
  activeView: string;
};

export type LayoutState = {
  version: 4;
  sidebar: { visible: boolean; width: number };
  info: { visible: boolean; width: number };
  panel: { visible: boolean; height: number };
  /** 视图容器（可移动视图的归属与顺序）。main 是编辑器区域，不在此列。 */
  containers: Record<ViewContainerId, ViewContainerState>;
  /** 被显式隐藏（从所有容器移除）的视图 id。 */
  hiddenViews: string[];
  main: { splits: MainSplit[] }; // empty = single view
  activity: { active: string | null }; // "sessions" | "chat" | "neurons" | "info"
};

export const DEFAULT_LAYOUT: LayoutState = {
  version: 4,
  sidebar: { visible: true, width: 260 },
  info: { visible: true, width: 280 },
  // v2/v3: 底部面板默认展开（对齐 VS Code 底部栏习惯）
  panel: { visible: true, height: 200 },
  containers: {
    sidebar: { views: ["sessions"], activeView: "sessions" },
    // v4: 原 Info 组合面板拆分为 providers/models/topics 三个独立视图
    info: { views: ["providers", "models", "topics"], activeView: "providers" },
    panel: { views: ["poller", "tools", "logs"], activeView: "poller" },
  },
  hiddenViews: [],
  main: { splits: [] },
  activity: { active: "sessions" },
};

// ── Bounds ──

export const BOUNDS = {
  sidebar: { min: 120, max: 400 },
  info: { min: 160, max: 480 },
  panel: { min: 100 },
  splitRatio: { min: 0.3, max: 0.7 },
} as const;

export function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}
