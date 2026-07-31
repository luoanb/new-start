// ── Layout state types & defaults ──

export type SplitOrientation = "horizontal" | "vertical";

export type MainSplit = {
  id: string; // "chat" | "neuron"
  orientation: SplitOrientation;
  ratio: number; // first pane ratio 0.3~0.7
};

export type LayoutState = {
  version: 2;
  sidebar: { visible: boolean; width: number };
  info: { visible: boolean; width: number };
  panel: { visible: boolean; height: number; activeView: string };
  main: { splits: MainSplit[] }; // empty = single view
  activity: { active: string | null }; // "sessions" | "chat" | "neurons" | "info"
};

export const DEFAULT_LAYOUT: LayoutState = {
  version: 2,
  sidebar: { visible: true, width: 260 },
  info: { visible: true, width: 280 },
  // v2: 底部面板默认展开（对齐 VS Code 底部栏习惯）
  panel: { visible: true, height: 200, activeView: "poller" },
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
