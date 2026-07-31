// 视图元数据注册表 —— ActivityBar 与 dock 标题使用。
// 组件实例的挂载仍由 +page.svelte 负责（各视图 props 不同，避免泛型组件注册的复杂度）。

export type ViewMeta = {
  id: string;
  label: string;
  icon?: string;
};

export const activityItems: ViewMeta[] = [
  { id: "sessions", label: "Sessions", icon: "💬" },
  { id: "chat", label: "Chat", icon: "🖥" },
  { id: "neurons", label: "Neurons", icon: "🧠" },
  { id: "info", label: "Info", icon: "ⓘ" },
];

export const panelViews: ViewMeta[] = [
  { id: "poller", label: "Poller" },
  { id: "logs", label: "Logs" },
];

// 主区域（chat-area）tab 栏：VS Code editor group 风格，split 时并排显示
export const mainTabs: ViewMeta[] = [
  { id: "chat", label: "Chat", icon: "🖥" },
  { id: "neurons", label: "Neurons", icon: "🧠" },
];
