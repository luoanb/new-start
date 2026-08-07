// Runes（$state/$derived）在 .svelte.ts 中由 Svelte 编译器直接转换，无需 import。
import type { LayoutState, MainSplit, ViewContainerId } from "./layoutTypes";
import { DEFAULT_LAYOUT, BOUNDS, clamp } from "./layoutTypes";
import { LocalStorageLayoutStorage, type LayoutStorage } from "./layoutStorage";

const storage: LayoutStorage = new LocalStorageLayoutStorage();

const state = $state<LayoutState>(storage.load() ?? DEFAULT_LAYOUT);

function persist() {
  storage.save(state);
}

/** 从指定容器移除视图；若移除的是激活视图，回退到容器首个视图（无则置空）。 */
function removeViewFromContainer(containerId: ViewContainerId, viewId: string): void {
  const c = state.containers[containerId];
  if (!c) return;
  const idx = c.views.indexOf(viewId);
  if (idx >= 0) {
    c.views.splice(idx, 1);
    if (c.activeView === viewId) c.activeView = c.views[0] ?? "";
  }
}

/** 从所有视图容器移除 viewId（移动/隐藏共用的前置步骤）。 */
function detachView(viewId: string): void {
  for (const cid of Object.keys(state.containers) as ViewContainerId[]) {
    removeViewFromContainer(cid, viewId);
  }
  const hi = state.hiddenViews.indexOf(viewId);
  if (hi >= 0) state.hiddenViews.splice(hi, 1);
}

export const layoutStore = {
  state,

  /** 拖动结束后手动持久化（拖动中 setXxx(persistNow=false) 避免高频写入） */
  persistNow: persist,

  toggleSidebar() {
    state.sidebar.visible = !state.sidebar.visible;
    persist();
  },
  toggleInfo() {
    state.info.visible = !state.info.visible;
    persist();
  },
  togglePanel() {
    state.panel.visible = !state.panel.visible;
    persist();
  },

  setSidebarWidth(width: number, persistNow = true) {
    state.sidebar.width = clamp(width, BOUNDS.sidebar.min, BOUNDS.sidebar.max);
    if (persistNow) persist();
  },
  setInfoWidth(width: number, persistNow = true) {
    state.info.width = clamp(width, BOUNDS.info.min, BOUNDS.info.max);
    if (persistNow) persist();
  },
  setPanelHeight(height: number, persistNow = true) {
    state.panel.height = clamp(height, BOUNDS.panel.min, window.innerHeight * 0.6);
    if (persistNow) persist();
  },

  /** 切换指定视图容器内的激活视图（tab 点击）。 */
  setContainerView(containerId: ViewContainerId, viewId: string) {
    const c = state.containers[containerId];
    if (!c || !c.views.includes(viewId)) return;
    c.activeView = viewId;
    persist();
  },

  /** 跨容器移动视图（拖拽）：从所有容器脱离后挂入目标容器并激活。 */
  moveView(viewId: string, targetContainerId: ViewContainerId) {
    detachView(viewId);
    const target = state.containers[targetContainerId];
    if (!target.views.includes(viewId)) target.views.push(viewId);
    target.activeView = viewId;
    persist();
  },

  /** 隐藏视图（移入 hiddenViews，可从 ⋯ 菜单重新显示）。 */
  hideView(viewId: string) {
    detachView(viewId);
    if (!state.hiddenViews.includes(viewId)) state.hiddenViews.push(viewId);
    persist();
  },

  /** 显示隐藏视图：挂入目标容器并激活。 */
  showView(viewId: string, targetContainerId: ViewContainerId) {
    detachView(viewId);
    const target = state.containers[targetContainerId];
    if (!target.views.includes(viewId)) target.views.push(viewId);
    target.activeView = viewId;
    persist();
  },

  setMainSplits(splits: MainSplit[]) {
    state.main.splits = splits;
    persist();
  },
  updateMainSplitRatio(ratio: number, persistNow = true) {
    if (state.main.splits.length === 0) return;
    state.main.splits[0].ratio = clamp(ratio, BOUNDS.splitRatio.min, BOUNDS.splitRatio.max);
    if (persistNow) persist();
  },

  setActivity(id: string | null) {
    state.activity.active = id;
    persist();
  },
};
