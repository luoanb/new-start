// 视图元数据注册表 —— ActivityBar 与 dock 标题使用。
// 组件实例的挂载仍由 +page.svelte 负责（各视图 props 不同，避免泛型组件注册的复杂度）。

export type ViewMeta = {
  id: string;
  label: string;
  icon?: string;
};

export const activityItems: ViewMeta[] = [
  {
    id: "sessions",
    label: "Sessions",
    icon: '<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M21 11.5a8.38 8.38 0 0 1-8.5 8.5 8.5 8.5 0 0 1-3.8-.9L3 21l1.9-5.7a8.5 8.5 0 0 1-.9-3.8A8.38 8.38 0 0 1 12.5 3 8.38 8.38 0 0 1 21 11.5z"/></svg>',
  },
];

export const panelViews: ViewMeta[] = [
  { id: "poller", label: "Poller" },
  { id: "tools", label: "Tools" },
  { id: "logs", label: "Logs" },
];

// 主区域（chat-area）tab 栏：VS Code editor group 风格，split 时并排显示
export const mainTabs: ViewMeta[] = [
  { id: "chat", label: "Chat", icon: "🖥" },
  { id: "neurons", label: "Neurons", icon: '<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="5" cy="6" r="2"/><circle cx="19" cy="7" r="2"/><circle cx="12" cy="18" r="2"/><line x1="6.5" y1="7" x2="11" y2="16"/><line x1="17.5" y1="8" x2="13" y2="16"/></svg>' },
];
