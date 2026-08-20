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
  WorkspaceView,
  GitView,
  GitRepo,
  GitStatusView,
  GitBranchItem,
  GitCommitInfo,
  GitShowFile,
  GitFileDiff,
  GitStashEntry,
  GitConfirmRequest,
  GitResetMode,
  GitStashAction,
  ConflictTake,
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
  /** 当前流式占位消息在 `messages` 中的 index（无流式进行中为 null）。 */
  streamingIndex: null as number | null,
  runtimeStatus: null as RuntimeStatus | null,
  topics: [] as Topic[],
  poller: null as PollerStatus | null,
  runningSessions: [] as RunningSession[],
  neuronsVersion: 0,
  toolsVersion: 0,
  workspacesVersion: 0,
  /** 工作区集合 + active（文件树/编辑器数据源）。 */
  workspaces: null as WorkspaceView | null,
  // ── Git（git 面板 / 文件树徽标 / git-diff 数据源）──
  /** git 聚合视图（repos/status/branches/log/stash/开关）。 */
  git: null as GitView | null,
  /** 后端写操作确认请求队列（ConfirmDialog 消费后 resolve）。 */
  gitConfirmQueue: [] as GitConfirmRequest[],
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

/** 工作区集合 + active：workspaces 事件（增删/切换/ignore 编辑/fs 写操作）后重拉。 */
async function refreshWorkspaces(): Promise<void> {
  state.workspaces = await api.invoke<WorkspaceView>("list_workspaces");
  state.workspacesVersion++;
}

/** 空 git 视图（无仓库 / 拉取失败兜底）。 */
function emptyGitView(): GitView {
  return {
    repos: [],
    activeRepoId: null,
    statusByRepo: {},
    status: null,
    branches: [],
    log: [],
    stash: [],
    confirmConfig: { dangerous_writes: false },
  };
}

/**
 * Git 聚合视图刷新：git 事件（写操作/仓库切换）后重拉。
 * repos 先拉；active repo 失效时回落第一个 repo（后端 active_repo() 同策略）。
 * status 按 repo 逐仓库拉取（文件树徽标按文件归属 repo 取数，不切换 active repo）；
 * branches/log/stash 对 active repo 并行拉取，单个失败不影响其余（无仓库时整组跳过）。
 */
async function refreshGit(): Promise<void> {
  try {
    const repos = await api.invoke<GitRepo[]>("git_repos");
    const previousActive = state.git?.activeRepoId ?? null;
    let activeRepoId: string | null = null;
    if (repos.length > 0) {
      activeRepoId =
        previousActive && repos.some((r) => r.id === previousActive)
          ? previousActive
          : repos[0].id;
      if (activeRepoId !== previousActive) {
        await api.invoke("git_set_active_repo", { repoId: activeRepoId });
      }
    }
    const statusByRepo = Object.fromEntries(
      await Promise.all(
        repos.map(async (r) => [
          r.id,
          await api
            .invoke<GitStatusView>("git_status", { repoId: r.id })
            .catch(() => null),
        ]),
      ),
    ) as Record<string, GitStatusView | null>;
    const [branches, log, stash, confirmConfig] =
      repos.length > 0
        ? await Promise.all([
            api.invoke<GitBranchItem[]>("git_branches").catch(() => []),
            api.invoke<GitCommitInfo[]>("git_log").catch(() => []),
            api.invoke<GitStashEntry[]>("git_stash_list").catch(() => []),
            api
              .invoke<{ dangerous_writes: boolean }>("git_get_confirm_config")
              .catch(() => ({ dangerous_writes: false })),
          ])
        : [[], [], [], { dangerous_writes: false }];
    state.git = {
      repos,
      activeRepoId,
      statusByRepo,
      status: activeRepoId ? (statusByRepo[activeRepoId] ?? null) : null,
      branches,
      log,
      stash,
      confirmConfig: confirmConfig ?? { dangerous_writes: false },
    };
  } catch (e) {
    state.git = emptyGitView();
    state.error = `Git refresh failed: ${formatInvokeError(e)}`;
  }
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
    } else if (payload.kind === "message_delta") {
      // 流式增量：仅处理当前激活会话。
      // done=false 增量合并到「最后一条 assistant text」（流式占位消息）——不用绝对
      // message_index：resolve 阶段角色切换会额外落库 RoleContext/System，后端占位 index
      // 比前端乐观数组偏移，绝对 index 无法对齐；占位始终是「本轮最后落库的 assistant」，
      // 工具轮 ToolResult 追加在占位之后，从尾部倒数定位依然正确。
      // done=true 全量重拉收敛为权威数据（兜底广播丢弃/积压）。
      if (state.activeConversationId === payload.conversation_id) {
        if (payload.done) {
          state.streamingIndex = null;
          await refreshMessages();
        } else {
          let target = -1;
          for (let i = state.messages.length - 1; i >= 0; i--) {
            const m = state.messages[i];
            if (m.role === "assistant" && m.body.kind === "text") {
              target = i;
              break;
            }
          }
          // 兜底：无占位（乐观占位未生效/被覆盖）时补一条再合并。
          if (target === -1) {
            state.messages = [
              ...state.messages,
              { role: "assistant", body: { kind: "text", content: "" }, timestamp: Date.now() },
            ];
            target = state.messages.length - 1;
          }
          state.streamingIndex = target;
          state.messages = state.messages.map((m, i) =>
            i === target && m.body.kind === "text"
              ? {
                  ...m,
                  body: {
                    ...m.body,
                    content: payload.content,
                    reasoning: payload.reasoning || undefined,
                  },
                }
              : m
          );
        }
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
    } else if (payload.kind === "workspaces") {
      await refreshWorkspaces();
    } else if (payload.kind === "git") {
      await refreshGit();
    } else if (payload.kind === "git_confirm") {
      // 写操作确认请求：入队由 ConfirmDialog 消费（resolve 后弹出下一个）。
      state.gitConfirmQueue = [
        ...state.gitConfirmQueue,
        { op_id: payload.op_id, kind: payload.op_kind, title: payload.title, detail: payload.detail },
      ];
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
      workspacesRes,
    ] = await Promise.all([
      api.invoke<ProviderInfo[]>("list_providers"),
      api.invoke<ModelInfo[]>("list_models"),
      api.invoke<SkillInfo[]>("list_skills"),
      api.invoke<Conversation[]>("list_conversations"),
      api.invoke<RuntimeStatus>("status"),
      api.invoke<Topic[]>("list_topics"),
      api.invoke<PollerStatus>("poll_status"),
      api.invoke<RunningSession[]>("list_running_sessions"),
      api.invoke<WorkspaceView>("list_workspaces"),
    ]);

    state.providers = providersRes;
    state.models = modelsRes;
    state.skills = skillsRes;
    state.conversations = convsRes;
    state.runtimeStatus = statusRes;
    state.topics = topicsRes;
    state.poller = pollerRes;
    state.runningSessions = runningSessionsRes;
    state.workspaces = workspacesRes;
    state.error = "";

    // Git 面板/文件树徽标数据源（无仓库时内部回落空视图，不抛错）。
    await refreshGit();

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
  state.streamingIndex = null;
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

/** 中断当前运行中的会话（后端 close_session 触发 abort 回调）；广播 Sessions 事件自动刷新，兜底重拉。 */
async function stopRunningSession(sessionId: string): Promise<void> {
  await api.invoke<string>("close_session", { sessionId });
  await refreshRunningSessions();
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
  const conversationId = state.activeConversationId;
  state.streamingIndex = null; // 新轮开始：清除上一轮流式标记
  const userMsg: Message = {
    role: "user",
    body: { kind: "text", content: text },
    timestamp: Date.now(),
  };
  // 乐观追加 user + 空 assistant 占位：与后端落库顺序（user → assistant 占位）对齐，
  // 保证流式 MessageDelta.message_index 与本地数组 index 一致（增量原地合并）。
  // 仅追加 user 会使本地数组比后端少一条占位，index 越界 → 增量被丢弃、回答整块出现。
  state.messages = [
    ...state.messages,
    userMsg,
    {
      role: "assistant",
      body: { kind: "text", content: "" },
      timestamp: Date.now(),
    },
  ];
  try {
    const res = await api.invoke<ChatResponse>("send_chat_message", {
      message: text,
      providerId,
      modelId,
      conversationId,
      params,
      thinking,
    });
    return res;
  } finally {
    // 收敛/回滚统一走权威重拉：成功时兜底事件丢失（done:true / Conversations），
    // 失败时回滚乐观占位；不在本地追加 assistant——避免与 done:true 重拉竞态重复。
    if (state.activeConversationId === conversationId) {
      await refreshMessages();
    }
  }
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

// ── 工作区 actions（写操作后后端广播 workspaces 事件刷新 + 兜底 refresh）──

/** 添加工作区：root 为目录绝对路径；后端校验存在/重复并 canonicalize。 */
async function addWorkspace(root: string): Promise<void> {
  await api.invoke<WorkspaceView>("add_workspace", { root });
  await refreshWorkspaces();
}

/** 移除工作区条目（不删目录）；active 失效时后端自动清除。 */
async function removeWorkspace(id: string): Promise<void> {
  await api.invoke<WorkspaceView>("remove_workspace", { id });
  await refreshWorkspaces();
}

/** 切换 active 工作区（文件树/编辑器数据源随之切换）。 */
async function setActiveWorkspace(id: string): Promise<void> {
  await api.invoke<WorkspaceView>("set_active_workspace", { id });
  await refreshWorkspaces();
}

/** 更新工作区 ignore 过滤规则（写入 workspaces.json，立即生效）。 */
async function updateWorkspaceIgnore(id: string, ignore: string[]): Promise<void> {
  await api.invoke<WorkspaceView>("update_workspace_ignore", { id, ignore });
  await refreshWorkspaces();
}

// ── Git actions（写操作后依赖 StateChange::Git 事件刷新 + 兜底 refreshGit）──

/** 切换当前操作仓库（git 面板作用域；不改变文件树）。 */
async function setActiveGitRepo(repoId: string): Promise<void> {
  await api.invoke("git_set_active_repo", { repoId });
  await refreshGit();
}

/** 暂存：all=true 暂存全部，或按 paths（相对 repo 根）暂存指定路径。 */
async function gitAdd(paths: string[], all = false): Promise<void> {
  await api.invoke("git_add", { paths, all });
  await refreshGit();
}

/** 取消暂存：paths 为空 = 取消全部（git restore --staged）。 */
async function gitUnstage(paths: string[] = []): Promise<void> {
  await api.invoke("git_unstage", { paths });
  await refreshGit();
}

/** 撤销工作区改动（需确认；用户经 ConfirmDialog 决定）。 */
async function gitRestore(paths: string[]): Promise<void> {
  await api.invoke("git_restore", { paths });
  await refreshGit();
}

/** 提交暂存区（需确认，弹窗展示 staged diff 摘要）。 */
async function gitCommit(message: string): Promise<void> {
  await api.invoke("git_commit", { message });
  await refreshGit();
}

/** 重置到目标（默认 HEAD）；--hard/--keep 高危，开关+确认。 */
async function gitReset(mode: GitResetMode, target?: string): Promise<void> {
  await api.invoke("git_reset", { mode, target });
  await refreshGit();
}

/** 切换分支/提交（丢弃改动场景开关+确认）。 */
async function gitCheckout(target: string): Promise<void> {
  await api.invoke("git_checkout", { target });
  await refreshGit();
}

/** stash：push/apply 直接执行；pop/drop 需确认。 */
async function gitStash(action: GitStashAction, message?: string): Promise<void> {
  await api.invoke("git_stash", { action, message });
  await refreshGit();
}

/** 推送（需确认）。 */
async function gitPush(remote?: string, branch?: string): Promise<void> {
  await api.invoke("git_push", { remote, branch });
  await refreshGit();
}

/** 拉取并合并（需确认）。 */
async function gitPull(): Promise<void> {
  await api.invoke("git_pull");
  await refreshGit();
}

/** 冲突解决：ours / theirs / both（指定 repo；缺省 active）。 */
async function gitResolveConflict(path: string, take: ConflictTake, repoId?: string): Promise<void> {
  await api.invoke("git_resolve_conflict", { repoId, path, take });
  await refreshGit();
}

/** 确认服务唯一入口：投递用户决定并出队。 */
async function gitConfirm(opId: string, approved: boolean): Promise<void> {
  await api.invoke("git_confirm", { opId, approved });
  state.gitConfirmQueue = state.gitConfirmQueue.filter((r) => r.op_id !== opId);
}

/** 持久化并热更新危险写开关（config.json git 节）。 */
async function setDangerousWrites(enabled: boolean): Promise<void> {
  await api.invoke("git_set_dangerous_writes", { enabled });
  await refreshGit();
}

/**
 * 打开 git-diff 面板（实例 key = `git-diff:${repoId}:${relPath}:${range}`，按文件路径多开）。
 * range = 默认 diff 范围（按来源分组：暂存 → staged / 工作区 → unstaged / 冲突 → both）。
 */
function openGitDiff(repoId: string, relPath: string, range: "staged" | "unstaged" | "both"): void {
  layoutStore.insertPanel("git-diff", undefined, `git-diff:${repoId}:${relPath}:${range}`);
}

/** 某提交的变更文件统计列表（懒加载，不入全局 state）。 */
async function gitShowFiles(hash: string): Promise<GitShowFile[]> {
  return await api.invoke("git_show_files", { hash });
}

/** 某提交中单个文件的 unified diff（懒加载）。 */
async function gitShowDiff(hash: string, path: string): Promise<GitFileDiff> {
  return await api.invoke("git_show_diff", { hash, path });
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
  refreshWorkspaces,
  refreshGit,
  // actions
  selectConversation,
  createConversation,
  closeSession,
  stopRunningSession,
  sendMessage,
  setSessionModel,
  clearConversation,
  scoreFeedback,
  // 工作区（文件树/编辑器数据源）
  addWorkspace,
  removeWorkspace,
  setActiveWorkspace,
  updateWorkspaceIgnore,
  // Git
  setActiveGitRepo,
  gitAdd,
  gitUnstage,
  gitRestore,
  gitCommit,
  gitReset,
  gitCheckout,
  gitStash,
  gitPush,
  gitPull,
  gitResolveConflict,
  gitConfirm,
  setDangerousWrites,
  gitShowFiles,
  gitShowDiff,
  openGitDiff,
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
