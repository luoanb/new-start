import type { LayoutState, MainPane, MainPanel } from "./layoutTypes";
import { DEFAULT_LAYOUT, LAYOUT_VERSION } from "./layoutTypes";

// 抽象存储接口 —— 未来可替换为后端文件存储（如 Tauri fs 插件），LayoutStore 零改动
export interface LayoutStorage {
  load(): LayoutState | null;
  save(state: LayoutState): void;
}

export class LocalStorageLayoutStorage implements LayoutStorage {
  private readonly key = "pulsar-harness:layout";
  /** 改名前的旧 key（`pulsar:` 前缀时代），仅首次启动一次性迁移。 */
  private static readonly LEGACY_KEY = "pulsar:layout";

  load(): LayoutState | null {
    this.migrateLegacyKey();
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

/**
 * 版本迁移链入口：把持久化数据迁移到当前版本。
 *
 * 迁移框架保留原样，只是骨架内尚无历史结构 —— 当前版本只有 v1，
 * 因此「版本匹配 → 浅合并」，「版本未知/更早 → 回落默认」。
 * 后续结构变更时：`LAYOUT_VERSION` +1，并在此按 `parsed.version` 追加迁移分支。
 */
function normalize(parsed: Partial<LayoutState>): LayoutState {
  if (parsed.version === LAYOUT_VERSION) return merge(parsed);
  return { ...DEFAULT_LAYOUT };
}

/** 按默认布局浅合并用户持久化数据；未知字段丢弃，缺字段补默认值。 */
function merge(parsed: Partial<LayoutState>): LayoutState {
  const main = normalizeMain(parsed.main);
  return {
    version: LAYOUT_VERSION,
    sidebar: { ...DEFAULT_LAYOUT.sidebar, ...parsed.sidebar },
    info: { ...DEFAULT_LAYOUT.info, ...parsed.info },
    panel: { ...DEFAULT_LAYOUT.panel, ...parsed.panel },
    containers: {
      sidebar: { ...DEFAULT_LAYOUT.containers.sidebar, ...parsed.containers?.sidebar },
      info: { ...DEFAULT_LAYOUT.containers.info, ...parsed.containers?.info },
      panel: { ...DEFAULT_LAYOUT.containers.panel, ...parsed.containers?.panel },
    },
    hiddenViews: parsed.hiddenViews ?? [],
    main,
    activity: { ...DEFAULT_LAYOUT.activity, ...parsed.activity },
  };
}

/** main 一致性：panes 缺失或 activePaneId 悬空时回退（默认空 / 首个分栏）。 */
function normalizeMain(parsed: LayoutState["main"] | undefined): LayoutState["main"] {
  if (!Array.isArray(parsed?.panes)) return { ...DEFAULT_LAYOUT.main };
  const panes = normalizePanes(parsed.panes);
  return {
    panes,
    activePaneId: panes.some((p) => p.id === parsed.activePaneId)
      ? parsed.activePaneId
      : panes[0]?.id ?? null,
  };
}

/** 分栏面板归一化：旧单 `panel` 形态 → `panels[]` 形态；校验 activePanelId 悬空回退。
 * 唯一分栏的 grow 强制为 1：历史拖拽残留的非 1 值在单分栏时无意义且会导致宽度不满。 */
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
