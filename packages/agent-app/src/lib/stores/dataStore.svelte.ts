/**
 * 统一数据 Store（模块级 $state 单例，对齐 LayoutStore 范式）。
 *
 * 职责：
 * - bootstrap()：一次性拉取全部领域数据（providers/models/skills/conversations/status/topics/poller）。
 * - subscribe()：监听后端 `app://state-changed`，按 StateChange.kind 增量刷新对应领域。
 * - 组件只读 dataStore.state，写操作一律走 dataStore actions（内部 invoke + refresh），
 *   保证后端与 store 一致，消除各面板本地数组。
 *
 * 后端推送策略：写操作完成后广播 StateChange；Conversations/Topics 走「重拉」，
 * Poller 负载直带最新 PollerStatus（数据小，避免一次额外 invoke）。
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ProviderInfo,
  ModelInfo,
  SkillInfo,
  Conversation,
  Message,
  ChatResponse,
  RuntimeStatus,
  Topic,
  PollerStatus,
  RunningSession,
} from "$lib/types";
import { formatInvokeError } from "$lib/utils/formatInvokeError";

/** 与后端 core/events.rs STATE_CHANGED_EVENT 保持一致 */
export const STATE_CHANGED_EVENT = "app://state-changed";

export type StateEventKind = "topics" | "conversations" | "poller" | "sessions";

export type StateChangePayload =
  | { kind: "topics" }
  | { kind: "conversations" }
  | { kind: "poller"; status: PollerStatus }
  | { kind: "sessions" };

const state = $state({
  ready: false,
  error: "",
  providers: [] as ProviderInfo[],
  models: [] as ModelInfo[],
  skills: [] as SkillInfo[],
  conversations: [] as Conversation[],
  activeConversationId: null as string | null,
  messages: [] as Message[],
  runtimeStatus: null as RuntimeStatus | null,
  topics: [] as Topic[],
  poller: null as PollerStatus | null,
  runningSessions: [] as RunningSession[],
});

let unlisten: UnlistenFn | null = null;

// ── 内部刷新 ──

async function refreshMessages(): Promise<void> {
  if (!state.activeConversationId) {
    state.messages = [];
    return;
  }
  state.messages = await invoke<Message[]>("history", {
    conversationId: state.activeConversationId,
  });
}

async function refreshConversations(): Promise<void> {
  state.conversations = await invoke<Conversation[]>("list_conversations");
  // 会话变化往往伴随消息变化（发送/清空/后台推进），同步刷新当前会话消息。
  await refreshMessages();
}

async function refreshTopics(): Promise<void> {
  state.topics = await invoke<Topic[]>("list_topics");
}

async function refreshPoller(): Promise<void> {
  state.poller = await invoke<PollerStatus>("poll_status");
}

async function refreshRunningSessions(): Promise<void> {
  state.runningSessions = await invoke<RunningSession[]>("list_running_sessions");
}

// ── 事件订阅 ──

async function handleStateChanged(payload: StateChangePayload): Promise<void> {
  try {
    if (payload.kind === "topics") {
      await refreshTopics();
    } else if (payload.kind === "conversations") {
      await refreshConversations();
    } else if (payload.kind === "poller") {
      state.poller = payload.status;
    } else if (payload.kind === "sessions") {
      await refreshRunningSessions();
    }
  } catch (e) {
    state.error = `State refresh failed: ${formatInvokeError(e)}`;
  }
}

/** 监听后端状态变更事件；幂等（重复调用不会重复 listen）。 */
async function subscribe(): Promise<void> {
  if (unlisten) return;
  unlisten = await listen<StateChangePayload>(STATE_CHANGED_EVENT, (event) => {
    void handleStateChanged(event.payload);
  });
}

function unsubscribe(): void {
  unlisten?.();
  unlisten = null;
}

// ── 首次拉取 ──

async function bootstrap(): Promise<void> {
  try {
    const [
      providersRes,
      modelsRes,
      skillsRes,
      convsRes,
      statusRes,
      topicsRes,
      pollerRes,
      runningSessionsRes,
    ] = await Promise.all([
      invoke<ProviderInfo[]>("list_providers"),
      invoke<ModelInfo[]>("list_models"),
      invoke<SkillInfo[]>("list_skills"),
      invoke<Conversation[]>("list_conversations"),
      invoke<RuntimeStatus>("status"),
      invoke<Topic[]>("list_topics"),
      invoke<PollerStatus>("poll_status"),
      invoke<RunningSession[]>("list_running_sessions"),
    ]);

    state.providers = providersRes;
    state.models = modelsRes;
    state.skills = skillsRes;
    state.conversations = convsRes;
    state.runtimeStatus = statusRes;
    state.topics = topicsRes;
    state.poller = pollerRes;
    state.runningSessions = runningSessionsRes;
    state.error = "";

    // 默认选中第一个会话（若存在），并加载其消息。
    if (!state.activeConversationId && convsRes.length > 0) {
      state.activeConversationId = convsRes[0].id;
      await refreshMessages();
    }
  } catch (e) {
    state.error = `Failed to load: ${formatInvokeError(e)}`;
  } finally {
    state.ready = true;
  }
}

// ── Actions（写操作：内部 invoke，成功后依赖事件刷新 + 兜底 refresh）──

async function selectConversation(id: string): Promise<void> {
  state.activeConversationId = id;
  await refreshMessages();
}

async function createConversation(mode: string): Promise<string> {
  const id = await invoke<string>("create_conversation", { mode });
  state.activeConversationId = id;
  await refreshConversations();
  return id;
}

async function closeSession(sessionId: string): Promise<void> {
  await invoke<string>("close_session", { sessionId });
  // 若关闭的是当前会话，先清空本地选中，让列表刷新后由回退逻辑接管。
  if (state.activeConversationId === sessionId) {
    state.activeConversationId = null;
    state.messages = [];
  }
  await refreshConversations();
}

async function sendMessage(
  text: string,
  providerId: string,
  modelId: string,
): Promise<ChatResponse> {
  if (!state.activeConversationId) {
    throw new Error("No active session. Create a new session first.");
  }
  const userMsg: Message = { role: "user", content: text, timestamp: Date.now() };
  state.messages = [...state.messages, userMsg];
  const res = await invoke<ChatResponse>("send_chat_message", {
    message: text,
    providerId,
    modelId,
    conversationId: state.activeConversationId,
  });
  // 乐观追加 assistant 回复；后端 emit 会再触发一次权威刷新（幂等）。
  state.messages = [
    ...state.messages,
    { role: "assistant", content: res.response, timestamp: Date.now() },
  ];
  return res;
}

async function clearConversation(): Promise<void> {
  await invoke<string>("clear_conversation", {
    conversationId: state.activeConversationId,
  });
  await refreshConversations();
}

// Topic actions
async function createTopic(name: string, description: string): Promise<Topic> {
  const topic = await invoke<Topic>("create_topic", { name, description });
  await refreshTopics();
  return topic;
}

async function pauseTopic(id: string): Promise<Topic> {
  const topic = await invoke<Topic>("pause_topic", { id });
  await refreshTopics();
  return topic;
}

async function resumeTopic(id: string): Promise<Topic> {
  const topic = await invoke<Topic>("resume_topic", { id });
  await refreshTopics();
  return topic;
}

async function deleteTopic(id: string): Promise<boolean> {
  const deleted = await invoke<boolean>("delete_topic", { id });
  await refreshTopics();
  return deleted;
}

async function addScopeItem(
  topicId: string,
  goal: string,
  doneContract: string,
): Promise<Topic> {
  const topic = await invoke<Topic>("add_topic_scope_item", {
    topicId,
    goal,
    doneContract,
  });
  await refreshTopics();
  return topic;
}

async function completeScopeItem(topicId: string, itemId: string): Promise<Topic> {
  const topic = await invoke<Topic>("complete_topic_scope_item", { topicId, itemId });
  await refreshTopics();
  return topic;
}

async function deleteScopeItem(topicId: string, itemId: string): Promise<Topic> {
  const topic = await invoke<Topic>("delete_topic_scope_item", { topicId, itemId });
  await refreshTopics();
  return topic;
}

// Poller actions
async function pausePoller(): Promise<void> {
  await invoke<void>("poll_pause");
  await refreshPoller();
}

async function resumePoller(): Promise<void> {
  await invoke<void>("poll_resume");
  await refreshPoller();
}

async function triggerPoller(): Promise<void> {
  await invoke<void>("poll_trigger");
  await refreshPoller();
}

async function setPollParallelism(n: number): Promise<void> {
  await invoke<number>("poll_set_parallelism", { n });
  await refreshPoller();
}

export const dataStore = {
  state,
  bootstrap,
  subscribe,
  unsubscribe,
  refreshTopics,
  refreshConversations,
  refreshPoller,
  refreshRunningSessions,
  // actions
  selectConversation,
  createConversation,
  closeSession,
  sendMessage,
  clearConversation,
  createTopic,
  pauseTopic,
  resumeTopic,
  deleteTopic,
  addScopeItem,
  completeScopeItem,
  deleteScopeItem,
  pausePoller,
  resumePoller,
  triggerPoller,
  setPollParallelism,
};
