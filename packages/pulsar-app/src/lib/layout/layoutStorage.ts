import type { LayoutState, MainPane, MainPanel } from "./layoutTypes";
import { DEFAULT_LAYOUT } from "./layoutTypes";

// 抽象存储接口 —— 未来可替换为后端文件存储（如 Tauri fs 插件），LayoutStore 零改动
export interface LayoutStorage {
  load(): LayoutState | null;
  save(state: LayoutState): void;
}

export class LocalStorageLayoutStorage implements LayoutStorage {
  private readonly key = "pulsar:layout";
  private static readonly LEGACY_KEY = "agent-app:layout";

  load(): LayoutState | null {
    this.migrateLegacyKey();
    try {
      const raw = localStorage.getItem(this.key);
      if (!raw) return null;
      const parsed = JSON.parse(raw) as Partial<LayoutState>;
      // 旧布局中的 providers/models 两个独立视图 → 聚合为 providers-models（任意版本兼容）
      return mergeProvidersModels(normalize(parsed));
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

  /** 一次性的键迁移：旧 key 存在且新 key 未写入时，搬数据并清理旧键。 */
  private migrateLegacyKey(): void {
    const legacy = localStorage.getItem(LocalStorageLayoutStorage.LEGACY_KEY);
    if (legacy === null || localStorage.getItem(this.key) !== null) return;
    try {
      localStorage.setItem(this.key, legacy);
      localStorage.removeItem(LocalStorageLayoutStorage.LEGACY_KEY);
    } catch {
      // ignore
    }
  }
}

// 未知字段丢弃；version 匹配时逐段浅合并，缺字段补默认值。
// 旧版本迁移：保留面板宽度/高度/激活视图，视图归属回退默认。
function normalize(parsed: Partial<LayoutState>): LayoutState {
  // v8 及更早统一迁移 main（v7 的 splits → v8 的 panes；v8 直接沿用）
  const migratedMain = migrateMain(parsed);

  // v9+：merge 后仍确保 info 容器含 neurons-list（覆盖"v9 数据缺 neurons-list"的历史残留）
  if (parsed.version === DEFAULT_LAYOUT.version) {
    const merged = merge(parsed);
    if (migratedMain) merged.main = migratedMain;
    return migrateV8ToV9(sanitizeLegacyInfo(merged));
  }

  // v8 → v9：info 容器补 neurons-list、清理 main 区残留 session-specs 面板
  if (parsed.version === 8) {
    const merged = merge(parsed);
    if (migratedMain) merged.main = migratedMain;
    return migrateV8ToV9(sanitizeLegacyInfo(merged));
  }

  // v7：仅 main 结构变化（splits → panes），其余与 v8 相同
  if (parsed.version === 7) {
    const merged = merge(parsed);
    if (migratedMain) merged.main = migratedMain;
    return migrateV8ToV9(sanitizeLegacyInfo(merged));
  }

  // v3/v4/v5/v6 → v7：v3 的 Info 是单一组合视图 "info"，v4 起拆为三个独立面板；
  // v5/v6 曾默认把 topics 放在 info/panel，v6 起 topics 已归位 sidebar；
  // v7 起 tools 也默认归位左侧 sidebar（原默认在底部 panel）。
  // 迁移时 v3 重置 info 容器为默认；topics 若仍停留在旧默认位置则移入 sidebar；
  // tools 若仍停留在 panel 容器则移入 sidebar；已被用户自定义的保持不动。
  if (parsed.version === 6 || parsed.version === 5 || parsed.version === 4 || parsed.version === 3) {
    const merged = merge(parsed, parsed.version === 3);
    const withTopics = placeTopicsInSidebar(merged, parsed.version === 3);
    const placed = sanitizeLegacyInfo(placeToolsInSidebar(withTopics));
    if (migratedMain) placed.main = migratedMain;
    return migrateV8ToV9(placed);
  }

  if (parsed.version === 2) {
    const old = parsed as Partial<LayoutState> & {
      panel?: { visible?: boolean; height?: number; activeView?: string };
    };
    const result: LayoutState = {
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
    if (migratedMain) result.main = migratedMain;
    return migrateV8ToV9(result);
  }

  return { ...DEFAULT_LAYOUT };
}

/** v7 及更早的 main.splits（chat|neurons 并排）→ v8 panes。v8 数据无 splits，返回 null 沿用原结构。 */
function migrateMain(parsed: Partial<LayoutState>): LayoutState["main"] | null {
  const raw = parsed.main as { splits?: { id?: string }[] } | null | undefined;
  const splits = raw?.splits;
  if (!Array.isArray(splits) || splits.length === 0) return null;
  const panes: MainPane[] = splits.map((s, i) => {
    const panel: MainPanel = {
      id: `panel-${i}-${Math.random().toString(36).slice(2, 8)}`,
      type: s.id === "neuron" ? "neurons" : "chat",
    };
    return {
      id: `pane-${i}-${Math.random().toString(36).slice(2, 8)}`,
      grow: 1,
      panels: [panel],
      activePanelId: panel.id,
    };
  });
  return { panes, activePaneId: panes[0]?.id ?? null };
}

/** v8 → v9：info 容器补 `neurons-list`（models 之后，默认位置；用户自定义 info 也仅追加）、
 * 清理 main 区残留 `session-specs` 面板（旧会话规格面板已被统一管理取代）。 */
function migrateV8ToV9(state: LayoutState): LayoutState {
  const containers = { ...state.containers } as Record<
    string,
    { views: string[]; activeView: string }
  >;
  const info = containers.info;
  if (!info.views.includes("neurons-list")) {
    const modelsIdx = info.views.indexOf("models");
    const views = [...info.views];
    views.splice(modelsIdx >= 0 ? modelsIdx + 1 : views.length, 0, "neurons-list");
    containers.info = { views, activeView: info.activeView };
  }

  const panes = (state.main?.panes ?? []).map((p) => {
    // 迁移期过滤：移除已废弃的 session-specs 面板（类型已从 MainPanelType 移除，故按 string 比较）
    const panels = p.panels.filter((x) => (x.type as string) !== "session-specs");
    return {
      ...p,
      panels,
      activePanelId: panels.some((x) => x.id === p.activePanelId)
        ? p.activePanelId
        : (panels[0]?.id ?? null),
    };
  });
  return {
    ...state,
    containers: containers as LayoutState["containers"],
    main: { ...state.main, panes },
    // 升级后强制恢复神经元列表可见（不被历史隐藏状态吞掉）
    hiddenViews: (state.hiddenViews ?? []).filter((v) => v !== "neurons-list"),
  };
}

/** 分栏面板归一化：旧 v8 单 `panel` 形态 → `panels[]` 形态；校验 activePanelId 悬空回退。
 * 唯一分栏的 grow 强制为 1：历史拖拽残留的非 1 值（如 0.95）在单分栏时无意义且会导致宽度不满。 */
function normalizePanes(panes: MainPane[]): MainPane[] {
  const normalized = panes.map((p) => {
    const legacy = p as MainPane & { panel?: MainPanel };
    const panels = Array.isArray(legacy.panels)
      ? legacy.panels
      : legacy.panel
        ? [legacy.panel]
        : [];
    const activePanelId = panels.some((x) => x.id === p.activePanelId)
      ? p.activePanelId
      : panels[0]?.id ?? null;
    return {
      id: p.id,
      grow: typeof p.grow === "number" && Number.isFinite(p.grow) ? p.grow : 1,
      panels,
      activePanelId,
    };
  });
  if (normalized.length === 1) normalized[0].grow = 1;
  return normalized;
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

/** providers+models 聚合迁移：容器中出现的旧视图 id（providers/models）合并为
 * 单个 providers-models（去重、取首个出现位置），并清理隐藏列表中的旧 id。
 * 一次性迁移；已存 providers-models 的布局不受影响。 */
function mergeProvidersModels(state: LayoutState): LayoutState {
  const containers = { ...state.containers } as Record<
    string,
    { views: string[]; activeView: string }
  >;
  let changed = false;
  for (const cid of Object.keys(containers)) {
    const c = containers[cid];
    if (!c.views.some((v) => v === "providers" || v === "models")) continue;
    const views = c.views
      .map((v) => (v === "providers" || v === "models" ? "providers-models" : v))
      .filter((v, i, arr) => arr.indexOf(v) === i);
    let activeView = c.activeView;
    if (activeView === "providers" || activeView === "models") activeView = "providers-models";
    if (!views.includes(activeView)) activeView = views[0] ?? "";
    containers[cid] = { views, activeView };
    changed = true;
  }
  if (!changed && !state.hiddenViews.some((v) => v === "providers" || v === "models")) {
    return state;
  }
  return {
    ...state,
    containers: containers as LayoutState["containers"],
    hiddenViews: state.hiddenViews.filter((v) => v !== "providers" && v !== "models"),
  };
}

/** 兜底清理旧版组合视图 "info" 的残留 id（已被 providers/models/topics 取代），并修正 main 一致性。 */
function sanitizeLegacyInfo(state: LayoutState): LayoutState {
  const LEGACY = "info";
  const containers = { ...state.containers } as Record<string, { views: string[]; activeView: string }>;
  for (const cid of Object.keys(containers)) {
    const views = containers[cid].views.filter((v) => v !== LEGACY);
    const activeView =
      containers[cid].activeView === LEGACY ? views[0] ?? "" : containers[cid].activeView;
    containers[cid] = { views, activeView };
  }
  // main 一致性：panes 缺失或 activePaneId 悬空时回退（默认空 / 首个分栏）；分栏面板归一化为 panels[] 形态
  const main = Array.isArray(state.main?.panes)
    ? {
        ...state.main,
        panes: normalizePanes(state.main.panes),
        activePaneId: state.main.panes.some((p) => p.id === state.main.activePaneId)
          ? state.main.activePaneId
          : state.main.panes[0]?.id ?? null,
      }
    : { ...DEFAULT_LAYOUT.main };
  return {
    ...state,
    containers: containers as LayoutState["containers"],
    hiddenViews: state.hiddenViews.filter((v) => v !== LEGACY),
    main,
  };
}
