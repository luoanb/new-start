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
      return mergeWithDefault(parsed);
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

// 浅合并：缺字段补默认值，未知字段丢弃。version 不匹配时直接回退默认。
function mergeWithDefault(parsed: Partial<LayoutState>): LayoutState {
  if (parsed.version !== DEFAULT_LAYOUT.version) return { ...DEFAULT_LAYOUT };

  return {
    version: DEFAULT_LAYOUT.version,
    sidebar: { ...DEFAULT_LAYOUT.sidebar, ...parsed.sidebar },
    info: { ...DEFAULT_LAYOUT.info, ...parsed.info },
    panel: { ...DEFAULT_LAYOUT.panel, ...parsed.panel },
    main: { ...DEFAULT_LAYOUT.main, ...parsed.main },
    activity: { ...DEFAULT_LAYOUT.activity, ...parsed.activity },
  };
}
