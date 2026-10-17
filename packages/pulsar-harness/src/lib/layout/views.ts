/**
 * 视图注册表 —— ActivityBar / EditorTabs / 视图容器（ViewContainer）统一从这里消费。
 *
 * 容器与内容解耦的边界：容器只认识 ViewRegistration（id/title/icon/component），
 * 组件实例的挂载由 ViewHost 完成；各视图需要的数据/命令通过 ViewContext 自取。
 * 新增视图 = 在此注册一条记录，`+page.svelte` 无需改动。
 */
import type { Component } from "svelte";
import type { ViewContainerId } from "./layoutTypes";
import IconFileText from "~icons/lucide/file-text";
import IconPanelTop from "~icons/lucide/panel-top";
import LogPanel from "$lib/components/LogPanel.svelte";
import ExampleView from "$lib/components/ExampleView.svelte";
import type { MainPanelType } from "./layoutTypes";

export type { ViewContainerId } from "./layoutTypes";

export type ViewMeta = {
  id: string;
  label: string;
  /** 图标组件（`unplugin-icons` 构建期产出的 Svelte 组件），尺寸由容器样式控制。 */
  icon?: Component;
  /** 动态标题（原始文本，非 i18n key）：提供时优先展示 */
  title?: string;
  /** 动态标题截断展示（限制宽度） */
  truncate?: boolean;
  /** 未保存标记（●）：编辑器 tab 未保存时显示 */
  dirty?: boolean;
  /** 完整悬停提示 */
  tooltip?: string;
  /** icon 色调（业务包可用来区分条目来源） */
  iconTone?: string;
};

export type ViewRegistration = {
  id: string;
  title: string;
  /** 图标组件（见 `ViewMeta.icon`）。 */
  icon?: Component;
  component: Component;
  /** 允许被拖拽到的容器；"*" = 任意视图容器。缺省 = 不可移动（仅主区内部视图）。 */
  movableTo?: ViewContainerId[] | "*";
  /** 容器内保活：在视图容器里一旦激活过就常驻挂载，切走仅隐藏不卸载。
   * 有状态视图（如持有连接/缓冲的实例）需要，否则切 tab 回来实例已被销毁。 */
  keepAlive?: boolean;
};

/** 视图容器注册表（sidebar / info / panel 共享）。title/label 存 i18n key（views.*），渲染处以 t() 解析。 */
export const viewRegistry: Record<string, ViewRegistration> = {
  logs: {
    id: "logs",
    title: "views.logs",
    icon: IconFileText,
    component: LogPanel,
    movableTo: "*",
  },
  example: {
    id: "example",
    title: "views.example",
    icon: IconPanelTop,
    component: ExampleView,
    movableTo: "*",
  },
};

/** main 区域（editor area）专用视图：走 EditorTabs + split 语义，不进入视图容器。 */
export const mainViews: ViewRegistration[] = [
  { id: "example", title: "views.example", component: ExampleView },
];

/** Activity Bar 入口（icon 轨）。 */
export const activityItems: ViewMeta[] = [
  { id: "example", label: "views.example", icon: IconPanelTop },
];

/**
 * main 区面板元数据（tab 栏 / 空态提示共用）。
 * icon 统一为组件，尺寸由 EditorTabs 的 tab 图标样式控制。
 */
export const mainPanelMeta: Record<MainPanelType, { label: string; icon: Component }> = {
  example: { label: "views.example", icon: IconPanelTop },
};

export function getRegistration(id: string): ViewRegistration | undefined {
  return viewRegistry[id];
}

/** 视图是否可被移动到指定容器。 */
export function canMoveTo(viewId: string, containerId: ViewContainerId): boolean {
  const reg = viewRegistry[viewId];
  if (!reg?.movableTo) return false;
  return reg.movableTo === "*" || reg.movableTo.includes(containerId);
}
