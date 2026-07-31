// Runes（$state/$derived）在 .svelte.ts 中由 Svelte 编译器直接转换，无需 import。
import type { LayoutState, MainSplit } from "./layoutTypes";
import { DEFAULT_LAYOUT, BOUNDS, clamp } from "./layoutTypes";
import { LocalStorageLayoutStorage, type LayoutStorage } from "./layoutStorage";

const storage: LayoutStorage = new LocalStorageLayoutStorage();

const state = $state<LayoutState>(storage.load() ?? DEFAULT_LAYOUT);

function persist() {
  storage.save(state);
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

  setPanelView(id: string) {
    state.panel.activeView = id;
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
