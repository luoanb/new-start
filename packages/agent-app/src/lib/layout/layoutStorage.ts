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
    return sanitizeLegacyInfo({
      version: DEFAULT_LAYOUT.version,
      sidebar: { ...DEFAULT_LAYOUT.sidebar, ...parsed.sidebar },
      info: { ...DEFAULT_LAYOUT.info, ...parsed.info },
      panel: { ...DEFAULT_LAYOUT.panel, ...parsed.panel },
      containers: {
        sidebar: { ...DEFAULT_LAYOUT.containers.sidebar, ...parsed.containers?.sidebar },
        info: { ...DEFAULT_LAYOUT.containers.info, ...parsed.containers?.info },
        panel: { ...DEFAULT_LAYOUT.containers.panel, ...parsed.containers?.panel },
      },
      hiddenViews: parsed.hiddenViews ?? [],
      main: { ...DEFAULT_LAYOUT.main, ...parsed.main },
      activity: { ...DEFAULT_LAYOUT.activity, ...parsed.activity },
    });
  }

  // v3 → v4：旧版 Info 是单一组合视图 "info"，v4 起拆为 providers/models/topics 三个独立面板。
  // 迁移时重置 info 容器为默认三面板；其余容器保留用户自定义归属。
  if (parsed.version === 3) {
    return sanitizeLegacyInfo({
      version: DEFAULT_LAYOUT.version,
      sidebar: { ...DEFAULT_LAYOUT.sidebar, ...parsed.sidebar },
      info: { ...DEFAULT_LAYOUT.info, ...parsed.info },
      panel: { ...DEFAULT_LAYOUT.panel, ...parsed.panel },
      containers: {
        sidebar: { ...DEFAULT_LAYOUT.containers.sidebar, ...parsed.containers?.sidebar },
        info: { ...DEFAULT_LAYOUT.containers.info },
        panel: { ...DEFAULT_LAYOUT.containers.panel, ...parsed.containers?.panel },
      },
      hiddenViews: parsed.hiddenViews ?? [],
      main: { ...DEFAULT_LAYOUT.main, ...parsed.main },
      activity: { ...DEFAULT_LAYOUT.activity, ...parsed.activity },
    });
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
