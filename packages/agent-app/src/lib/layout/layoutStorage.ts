import type { LayoutState } from "./layoutTypes";
import { DEFAULT_LAYOUT } from "./layoutTypes";

// 抽象存储接口 —— 未来可替换为后端文件存储（如 Tauri fs 插件），LayoutStore 零改动
export interface LayoutStorage {
  load(): LayoutState | null;
  save(state: LayoutState): void;
}

export class LocalStorageLayoutStorage implements LayoutStorage {
  private readonly key = "agent-app:layout";

  load(): LayoutState | null {
    try {
      const raw = localStorage.getItem(this.key);
      if (!raw) return null;
      const parsed = JSON.parse(raw) as Partial<LayoutState>;
      return normalize(parsed);
    } catch {
      return null;
    }
  }

  save(state: LayoutState): void {
    try {
      localStorage.setItem(this.key, JSON.stringify(state));
    } catch {
      // storage full / disabled —— 静默失败，不阻塞 UI
    }
  }
}

// 未知字段丢弃；version 匹配时逐段浅合并，缺字段补默认值。
// 旧版本迁移：保留面板宽度/高度/激活视图，视图归属回退默认。
function normalize(parsed: Partial<LayoutState>): LayoutState {
  if (parsed.version === DEFAULT_LAYOUT.version) {
    return sanitizeLegacyInfo(merge(parsed));
  }

  // v3/v4/v5/v6 → v7：v3 的 Info 是单一组合视图 "info"，v4 起拆为三个独立面板；
  // v5/v6 曾默认把 topics 放在 info/panel，v6 起 topics 已归位 sidebar；
  // v7 起 tools 也默认归位左侧 sidebar（原默认在底部 panel）。
  // 迁移时 v3 重置 info 容器为默认；topics 若仍停留在旧默认位置则移入 sidebar；
  // tools 若仍停留在 panel 容器则移入 sidebar；已被用户自定义的保持不动。
  if (parsed.version === 6 || parsed.version === 5 || parsed.version === 4 || parsed.version === 3) {
    const merged = merge(parsed, parsed.version === 3);
    const withTopics = placeTopicsInSidebar(merged, parsed.version === 3);
    return sanitizeLegacyInfo(placeToolsInSidebar(withTopics));
  }

  if (parsed.version === 2) {
    const old = parsed as Partial<LayoutState> & {
      panel?: { visible?: boolean; height?: number; activeView?: string };
    };
    return {
      version: DEFAULT_LAYOUT.version,
      sidebar: { ...DEFAULT_LAYOUT.sidebar, ...old.sidebar },
      info: { ...DEFAULT_LAYOUT.info, ...old.info },
      panel: { ...DEFAULT_LAYOUT.panel, ...old.panel },
      containers: {
        ...DEFAULT_LAYOUT.containers,
        panel: {
          ...DEFAULT_LAYOUT.containers.panel,
          activeView: old.panel?.activeView ?? DEFAULT_LAYOUT.containers.panel.activeView,
        },
      },
      hiddenViews: [],
      main: { ...DEFAULT_LAYOUT.main, ...old.main },
      activity: { ...DEFAULT_LAYOUT.activity, ...old.activity },
    };
  }

  return { ...DEFAULT_LAYOUT };
}

/** 按默认布局浅合并用户持久化数据；resetInfo 时 info 容器强制使用默认（旧组合视图场景）。 */
function merge(parsed: Partial<LayoutState>, resetInfo = false): LayoutState {
  return {
    version: DEFAULT_LAYOUT.version,
    sidebar: { ...DEFAULT_LAYOUT.sidebar, ...parsed.sidebar },
    info: { ...DEFAULT_LAYOUT.info, ...parsed.info },
    panel: { ...DEFAULT_LAYOUT.panel, ...parsed.panel },
    containers: {
      sidebar: { ...DEFAULT_LAYOUT.containers.sidebar, ...parsed.containers?.sidebar },
      info: resetInfo
        ? { ...DEFAULT_LAYOUT.containers.info }
        : { ...DEFAULT_LAYOUT.containers.info, ...parsed.containers?.info },
      panel: { ...DEFAULT_LAYOUT.containers.panel, ...parsed.containers?.panel },
    },
    hiddenViews: parsed.hiddenViews ?? [],
    main: { ...DEFAULT_LAYOUT.main, ...parsed.main },
    activity: { ...DEFAULT_LAYOUT.activity, ...parsed.activity },
  };
}

/** topics 归位左侧 sidebar（sessions 之后，v6 默认归属）。
 * 仅迁移"仍停留在旧默认位置"的 topics（v4 的 info 容器、v5 的 panel poller 右侧）；
 * ensure 用于 v3：视图尚不存在，直接补齐到 sidebar。 */
function placeTopicsInSidebar(state: LayoutState, ensure = false): LayoutState {
  const containers = { ...state.containers } as Record<string, { views: string[]; activeView: string }>;
  const inInfo = containers.info.views.includes("topics");
  const panelViews = containers.panel.views;
  const pollerIdx = panelViews.indexOf("poller");
  const inPanelDefault = pollerIdx >= 0 && panelViews[pollerIdx + 1] === "topics";
  const anywhere = Object.values(containers).some((c) => c.views.includes("topics"));
  const hidden = state.hiddenViews.includes("topics");

  // v3：topics 视图不存在且未被隐藏 → 补齐默认归属；其余版本仅处理旧默认位置的残留
  const shouldPlace = ensure ? !anywhere && !hidden : inInfo || inPanelDefault;
  if (!shouldPlace) return state;

  for (const cid of Object.keys(containers)) {
    const c = containers[cid];
    if (c.views.includes("topics")) {
      const views = c.views.filter((v) => v !== "topics");
      containers[cid] = {
        views,
        activeView: c.activeView === "topics" ? views[0] ?? "" : c.activeView,
      };
    }
  }

  const sidebarViews = containers.sidebar.views.filter((v) => v !== "topics");
  const sessionsIdx = sidebarViews.indexOf("sessions");
  sidebarViews.splice(sessionsIdx >= 0 ? sessionsIdx + 1 : sidebarViews.length, 0, "topics");
  containers.sidebar = { views: sidebarViews, activeView: containers.sidebar.activeView };

  return { ...state, containers: containers as LayoutState["containers"] };
}

/** tools 归位左侧 sidebar（末尾，v7 默认归属）。仅迁移仍停留在底部 panel 容器的 tools。 */
function placeToolsInSidebar(state: LayoutState): LayoutState {
  const panel = state.containers.panel;
  if (!panel.views.includes("tools")) return state;

  const containers = { ...state.containers } as Record<string, { views: string[]; activeView: string }>;
  const panelViews = panel.views.filter((v) => v !== "tools");
  containers.panel = {
    views: panelViews,
    activeView: panel.activeView === "tools" ? panelViews[0] ?? "" : panel.activeView,
  };

  const sidebarViews = [...containers.sidebar.views];
  if (!sidebarViews.includes("tools")) sidebarViews.push("tools");
  containers.sidebar = { views: sidebarViews, activeView: containers.sidebar.activeView };

  return { ...state, containers: containers as LayoutState["containers"] };
}

/** 兜底清理旧版组合视图 "info" 的残留 id（已被 providers/models/topics 取代）。 */
function sanitizeLegacyInfo(state: LayoutState): LayoutState {
  const LEGACY = "info";
  const containers = { ...state.containers } as Record<string, { views: string[]; activeView: string }>;
  for (const cid of Object.keys(containers)) {
    const views = containers[cid].views.filter((v) => v !== LEGACY);
    const activeView =
      containers[cid].activeView === LEGACY ? views[0] ?? "" : containers[cid].activeView;
    containers[cid] = { views, activeView };
  }
  return {
    ...state,
    containers: containers as LayoutState["containers"],
    hiddenViews: state.hiddenViews.filter((v) => v !== LEGACY),
  };
}
