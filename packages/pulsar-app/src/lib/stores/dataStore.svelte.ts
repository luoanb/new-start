/**
 * 统一数据 Store（模块级 $state 单例，对齐 LayoutStore 范式）。
 *
 * 职责：
 * - bootstrap()：一次性拉取全部领域数据（providers/models/skills/conversations/status/topics/poller）。
 * - subscribe()：通过 `api` 客户端订阅状态变更事件，按 StateChange.kind 增量刷新对应领域。
 * - 组件只读 dataStore.state，写操作一律走 dataStore actions（内部 api.invoke + refresh），
 *   保证后端与 store 一致，消除各面板本地数组。
 *
 * 通信方式：统一走 `$lib/api`（本机 Tauri IPC / 远程 HTTP+SSE），业务层无感知。
 * 后端推送策略：写操作完成后广播 StateChange；Conversations/Topics 走「重拉」，
 * Poller 负载直带最新 PollerStatus（数据小，避免一次额外 invoke）。
 */
import { api } from "$lib/api";
import type { StateChangePayload } from "$lib/api/types";
import { layoutStore } from "$lib/layout/LayoutStore.svelte";
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
  SamplingParams,
  ThinkingConfig,
  ChatModelSelection,
} from "$lib/types";
import { formatInvokeError } from "$lib/utils/formatInvokeError";

export type { StateChangePayload } from "$lib/api/types";

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
  neuronsVersion: 0,
  toolsVersion: 0,
  // ── 神经元统一管理：列表 ←→ 画布共享状态 ──
  /** 画布核心（单选=1 项，多选=数组；列表行点击驱动）。 */
  neuronSelection: [] as string[],
  /** 列表顶栏多选开关：single = 点击行替换核心；multi = 点击行切换勾选。 */
  neuronSelectionMode: "single" as "single" | "multi",
  /** 列表「编辑」→ 画布抽屉（消费后置 null）。 */
  neuronEditRequestId: null as string | null,
  /** 列表「创建」→ 画布创建弹窗（计数触发，消费后自增）。 */
  neuronCreateRequest: 0,
  /** 列表「发起」→ 打开发起神经元会话（消费后置 null）。 */
  neuronLaunchRequestId: null as string | null,
  // ── 服务商/模型管理：聚合面板 → main 区编辑器共享状态 ──
  /** 面板「编辑」→ 编辑器打开该服务商（消费后置 null）。 */
  providerEditRequestId: null as string | null,
  /** 面板「新增」→ 编辑器进入新建态（计数触发，消费后自增）。 */
  providerCreateRequest: 0,
});

let unlisten: (() => void) | null = null;

// ── 内部刷新 ──

// runningSessions 刷新版本守卫：register/update/unregister 每次变化都会触发一次
// 全量 api.invoke，响应可能乱序；只允许"最后一次发起的刷新"写入，丢弃过期旧快照，
// 防止已结束的会话残留（表现为永久"思考中"）。
let runningSessionsSeq = 0;

async function refreshRunningSessions(): Promise<void> {
  const seq = ++runningSessionsSeq;
  const list = await api.invoke<RunningSession[]>("list_running_sessions");
  if (seq !== runningSessionsSeq) return;
  state.runningSessions = list;
}

async function refreshMessages(): Promise<void> {
  if (!state.activeConversationId) {
    state.messages = [];
    return;
  }
  state.messages = await api.invoke<Message[]>("history", {
    conversationId: state.activeConversationId,
  });
}

async function refreshConversations(): Promise<void> {
  state.conversations = await api.invoke<Conversation[]>("list_conversations");
  // 会话变化往往伴随消息变化（发送/清空/后台推进），同步刷新当前会话消息。
  await refreshMessages();
}

async function refreshTopics(): Promise<void> {
  state.topics = await api.invoke<Topic[]>("list_topics");
}

async function refreshPoller(): Promise<void> {
  state.poller = await api.invoke<PollerStatus>("poll_status");
}

/** 服务商/模型配置变化（保存后广播）：重新拉取 providers 与 models。 */
async function refreshProvidersModels(): Promise<void> {
  const [providersRes, modelsRes] = await Promise.all([
    api.invoke<ProviderInfo[]>("list_providers"),
    api.invoke<ModelInfo[]>("list_models"),
  ]);
  state.providers = providersRes;
  state.models = modelsRes;
}

// ── 事件订阅 ──

async function handleStateChanged(payload: StateChangePayload): Promise<void> {
  try {
    if (payload.kind === "topics") {
      await refreshTopics();
    } else if (payload.kind === "conversations") {
      // affected 为实际发生写入的会话；空转轮询后端不 emit，这里仅防御。
      const affected = payload.affected ?? [];
      if (affected.length === 0) return;
      // 会话列表摘要始终重拉（标题/最后消息/时间可能变）。
      state.conversations = await api.invoke<Conversation[]>("list_conversations");
      // 仅当受影响会话含当前激活会话时才重拉消息，
      // 避免后台推进其他会话时误触发当前会话重拉与滚动。
      if (state.activeConversationId && affected.includes(state.activeConversationId)) {
        await refreshMessages();
      }
    } else if (payload.kind === "poller") {
      state.poller = payload.status;
    } else if (payload.kind === "sessions") {
      await refreshRunningSessions();
    } else if (payload.kind === "neurons") {
      // 写入（创建/编辑/绑定行为）广播 Neurons：列表与画布各自订阅刷新。
      state.neuronsVersion++;
    } else if (payload.kind === "providers") {
      await refreshProvidersModels();
    } else if (payload.kind === "tools") {
      state.toolsVersion++;
    }
  } catch (e) {
    state.error = `State refresh failed: ${formatInvokeError(e)}`;
  }
}

/** 订阅后端状态变更事件；幂等（重复调用不会重复订阅）。 */
async function subscribe(): Promise<void> {
  if (unlisten) return;
  unlisten = api.subscribe((payload) => {
    void handleStateChanged(payload);
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
      api.invoke<ProviderInfo[]>("list_providers"),
      api.invoke<ModelInfo[]>("list_models"),
      api.invoke<SkillInfo[]>("list_skills"),
      api.invoke<Conversation[]>("list_conversations"),
      api.invoke<RuntimeStatus>("status"),
      api.invoke<Topic[]>("list_topics"),
      api.invoke<PollerStatus>("poll_status"),
      api.invoke<RunningSession[]>("list_running_sessions"),
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

// ── Actions（写操作：内部 api.invoke，成功后依赖事件刷新 + 兜底 refresh）──

async function selectConversation(id: string): Promise<void> {
  state.activeConversationId = id;
  await refreshMessages();
}

async function createConversation(mode: string): Promise<string> {
  const id = await api.invoke<string>("create_conversation", { mode });
  state.activeConversationId = id;
  await refreshConversations();
  return id;
}

async function closeSession(sessionId: string): Promise<void> {
  await api.invoke<string>("close_session", { sessionId });
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
  params?: SamplingParams,
  thinking?: ThinkingConfig,
): Promise<ChatResponse> {
  if (!state.activeConversationId) {
    throw new Error("No active session. Create a new session first.");
  }
  const userMsg: Message = {
    role: "user",
    body: { kind: "text", content: text },
    timestamp: Date.now(),
  };
  state.messages = [...state.messages, userMsg];
  const res = await api.invoke<ChatResponse>("send_chat_message", {
    message: text,
    providerId,
    modelId,
    conversationId: state.activeConversationId,
    params,
    thinking,
  });
  // 乐观追加 assistant 回复；后端 emit 会再触发一次权威刷新（幂等）。
  state.messages = [
    ...state.messages,
    {
      role: "assistant",
      body: { kind: "text", content: res.response },
      timestamp: Date.now(),
    },
  ];
  return res;
}

/** 持久化会话级模型选择到后端（后端持有）；写成功依赖 Conversations 事件刷新列表回显。 */
async function setSessionModel(
  conversationId: string,
  selection: ChatModelSelection,
): Promise<void> {
  await api.invoke("set_session_model", { conversationId, selection });
}

async function clearConversation(): Promise<void> {
  await api.invoke<string>("clear_conversation", {
    conversationId: state.activeConversationId,
  });
  await refreshConversations();
}

// 人工评价：按被评消息所在介入区间应用评分 delta（后端 emit Neurons 触发刷新）。
async function scoreFeedback(
  conversationId: string,
  messageIndex: number,
  score: number
): Promise<void> {
  await api.invoke("score_feedback", { conversationId, messageIndex, score });
}

// ── 神经元统一管理（列表 ←→ 画布共享状态）actions ──

/** 列表行点击：single 模式替换核心，multi 模式走 toggleNeuronSelection。 */
function setNeuronSelection(ids: string[]): void {
  state.neuronSelection = ids;
}

/** 多选模式：切换勾选（去重）。 */
function toggleNeuronSelection(id: string): void {
  state.neuronSelection = state.neuronSelection.includes(id)
    ? state.neuronSelection.filter((x) => x !== id)
    : [...state.neuronSelection, id];
}

/** 列表「编辑」→ 画布打开该神经元抽屉（NeuronManager 消费后置 null；先确保画布面板存在）。 */
function requestEditNeuron(id: string): void {
  state.neuronEditRequestId = id;
  layoutStore.insertPanel("neurons");
}

/** 列表「＋ 创建」→ 画布打开创建弹窗（NeuronManager 消费后自增；先确保画布面板存在）。 */
function requestCreateNeuron(): void {
  state.neuronCreateRequest++;
  layoutStore.insertPanel("neurons");
}

/** 列表「发起」→ 以 chat 模式打开发起神经元会话并插入会话面板（消费后置 null）。 */
async function requestLaunchNeuron(id: string): Promise<void> {
  state.neuronLaunchRequestId = null;
  await openSession(id, "chat");
  layoutStore.insertPanel("chat");
}

/** 按发起神经元发起会话（assistant 模式），选中新会话并跳转会话视图。 */
async function openSession(
  specNeuronId: string,
  mode: string = "assistant",
): Promise<Conversation> {
  const conv = await api.invoke<Conversation>("open_session", { specNeuronId, mode });
  state.activeConversationId = conv.id;
  await refreshConversations();
  return conv;
}

// ── 服务商/模型管理 actions（聚合面板 → main 区编辑器）──

/** 面板「编辑」→ 编辑器打开该服务商（先确保 provider-manager 面板存在）。 */
function requestEditProvider(id: string): void {
  state.providerEditRequestId = id;
  layoutStore.insertPanel("provider-manager");
}

/** 面板「新增」→ 编辑器进入新建态（先确保 provider-manager 面板存在）。 */
function requestCreateProvider(): void {
  state.providerCreateRequest++;
  layoutStore.insertPanel("provider-manager");
}

/** 会话一轮直调（resolve_round → execute_round）。 */
async function converseSession(
  sessionId: string,
  input: string,
  providerId: string,
  modelId: string,
): Promise<ChatResponse> {
  return api.invoke<ChatResponse>("converse_session", {
    sessionId,
    input,
    providerId,
    modelId,
  });
}

// Topic actions
async function createTopic(name: string, description: string): Promise<Topic> {
  const topic = await api.invoke<Topic>("create_topic", { name, description });
  await refreshTopics();
  return topic;
}

async function pauseTopic(id: string): Promise<Topic> {
  const topic = await api.invoke<Topic>("pause_topic", { id });
  await refreshTopics();
  return topic;
}

async function resumeTopic(id: string): Promise<Topic> {
  const topic = await api.invoke<Topic>("resume_topic", { id });
  await refreshTopics();
  return topic;
}

async function deleteTopic(id: string): Promise<boolean> {
  const deleted = await api.invoke<boolean>("delete_topic", { id });
  await refreshTopics();
  return deleted;
}

async function addScopeItem(
  topicId: string,
  goal: string,
  doneContract: string,
): Promise<Topic> {
  const topic = await api.invoke<Topic>("add_topic_scope_item", {
    topicId,
    goal,
    doneContract,
  });
  await refreshTopics();
  return topic;
}

async function completeScopeItem(topicId: string, itemId: string): Promise<Topic> {
  const topic = await api.invoke<Topic>("complete_topic_scope_item", { topicId, itemId });
  await refreshTopics();
  return topic;
}

async function deleteScopeItem(topicId: string, itemId: string): Promise<Topic> {
  const topic = await api.invoke<Topic>("delete_topic_scope_item", { topicId, itemId });
  await refreshTopics();
  return topic;
}

// Poller actions
async function pausePoller(): Promise<void> {
  await api.invoke<void>("poll_pause");
  await refreshPoller();
}

async function resumePoller(): Promise<void> {
  await api.invoke<void>("poll_resume");
  await refreshPoller();
}

async function triggerPoller(): Promise<void> {
  await api.invoke<void>("poll_trigger");
  await refreshPoller();
}

async function setPollParallelism(n: number): Promise<void> {
  await api.invoke<number>("poll_set_parallelism", { n });
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
  refreshProvidersModels,
  // actions
  selectConversation,
  createConversation,
  closeSession,
  sendMessage,
  setSessionModel,
  clearConversation,
  scoreFeedback,
  // 神经元统一管理（列表 ←→ 画布共享）
  setNeuronSelection,
  toggleNeuronSelection,
  requestEditNeuron,
  requestCreateNeuron,
  requestLaunchNeuron,
  openSession,
  converseSession,
  // 服务商/模型管理（聚合面板 → main 区编辑器）
  requestEditProvider,
  requestCreateProvider,
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
