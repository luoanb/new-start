/**
 * 会话模式清单 —— 新建会话入口的单一真相源。
 *
 * 消费方：
 * - `SessionCreateModal`：弹窗卡片选择；
 * - `SessionList`：会话面板「[会话模式 ▾] + [+]」组合按钮；
 * - 顶栏新建入口（StatusBar）：hover 展示当前选中模式。
 *
 * 任何新增/隐藏模式只改这里，避免多处模式列表漂移。
 *
 * 注意：Agent 模式暂不提供新建入口（隐藏，后端与历史会话保留）。
 */

export type SessionMode = {
  /** 模式 id：透传给后端 createConversation(mode)。 */
  id: string;
  /** 模式名 i18n key（createModal.*Label）。 */
  labelKey: string;
  /** 模式说明 i18n key（createModal.*Desc）。 */
  descKey: string;
};

/** 可新建的会话模式（顺序即下拉/卡片展示顺序）。 */
export const SESSION_MODES: readonly SessionMode[] = [
  { id: "chat", labelKey: "createModal.chatLabel", descKey: "createModal.chatDesc" },
  { id: "assistant", labelKey: "createModal.assistantLabel", descKey: "createModal.assistantDesc" },
  { id: "system", labelKey: "createModal.systemLabel", descKey: "createModal.systemDesc" },
];

/** 缺省会话模式（新建入口初始选择）。 */
export const DEFAULT_SESSION_MODE = "chat";

/** 按 id 查模式定义（未知 id 返回 undefined，调用方自行回退展示 id 原文）。 */
export function findSessionMode(id: string): SessionMode | undefined {
  return SESSION_MODES.find((mode) => mode.id === id);
}
