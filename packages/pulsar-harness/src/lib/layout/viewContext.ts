/**
 * ViewContext —— 容器与内容解耦的核心。
 *
 * 容器只消费注册表（id → component），视图组件通过 context 自取所需
 * （数据 store / 命令 / 视图级 UI 状态），而非由组合根逐个传 props。
 * 新增视图 = 注册表加一条，零改 `+page.svelte`。
 */
import { getContext, setContext } from "svelte";
import { dataStore } from "$lib/stores/dataStore.svelte";
import { layoutStore } from "./LayoutStore.svelte";

export const VIEW_CTX_KEY = "pulsar-harness:view-ctx";

/** 组合根（`+page.svelte`）提供的视图命令集合。业务包在此扩展各自命令。 */
export type ViewCommands = {
  /** 顶部错误横幅：设置文案（组件自清）。 */
  showError: (msg: string) => void;
  /** 关闭错误横幅。 */
  dismissError: () => void;
};

/**
 * 视图级 UI 状态（组合根持有，`$state` 保证响应式传播）。
 * 骨架内为空对象：业务包在此加字段（如当前选中项），视图经 `ctx.ui` 读取。
 */
export type ViewUiState = Record<string, never>;

export type ViewContext = {
  stores: {
    data: typeof dataStore;
    layout: typeof layoutStore;
  };
  ui: ViewUiState;
  commands: ViewCommands;
};

export function setViewContext(ctx: ViewContext): void {
  setContext(VIEW_CTX_KEY, ctx);
}

export function useViewContext(): ViewContext {
  return getContext<ViewContext>(VIEW_CTX_KEY);
}
