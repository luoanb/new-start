<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  type AppError = {
    code: string;
    message: string;
  };

  type ChatResponse = {
    conversation_id: string;
    response: string;
  };

  type Conversation = {
    id: string;
    messages: Message[];
    created_at: number;
    updated_at: number;
  };

  type Message = {
    role: "user" | "assistant" | "system";
    content: string;
    timestamp: number;
  };

  type RuntimeStatus = {
    app_name: string;
    storage_path: string;
    current_conversation_id: string;
    skill_count: number;
    conversation_count: number;
  };

  type SkillInfo = {
    name: string;
    description: string;
  };

  let message = $state("");
  let response = $state("");
  let error = $state("");
  let status = $state<RuntimeStatus | null>(null);
  let skills = $state<SkillInfo[]>([]);
  let conversations = $state<Conversation[]>([]);
  let loading = $state(false);

  onMount(() => {
    void refresh();
  });

  async function refresh() {
    error = "";
    try {
      const [nextStatus, nextSkills, nextConversations] = await Promise.all([
        invoke<RuntimeStatus>("status"),
        invoke<SkillInfo[]>("list_skills"),
        invoke<Conversation[]>("list_conversations"),
      ]);

      status = nextStatus;
      skills = nextSkills;
      conversations = nextConversations;
    } catch (caught) {
      error = formatError(caught);
    }
  }

  async function send(event: Event) {
    event.preventDefault();
    if (!message.trim()) {
      error = "消息不能为空";
      return;
    }

    loading = true;
    error = "";

    try {
      const result = await invoke<ChatResponse>("send_message", { message });
      response = result.response;
      message = "";
      await refresh();
    } catch (caught) {
      error = formatError(caught);
    } finally {
      loading = false;
    }
  }

  async function clearCurrentConversation() {
    error = "";
    try {
      await invoke<string>("clear_conversation", {
        conversationId: status?.current_conversation_id,
      });
      response = "";
      await refresh();
    } catch (caught) {
      error = formatError(caught);
    }
  }

  function formatError(caught: unknown) {
    const appError = caught as Partial<AppError>;
    return appError.message ?? "未知错误";
  }
</script>

<main class="container">
  <header>
    <div>
      <p class="eyebrow">Rust Core / Tauri / CLI / TUI</p>
      <h1>Agent App</h1>
    </div>
    <button type="button" onclick={refresh}>刷新状态</button>
  </header>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  <section class="panel">
    <h2>运行状态</h2>
    {#if status}
      <dl>
        <div>
          <dt>当前会话</dt>
          <dd>{status.current_conversation_id}</dd>
        </div>
        <div>
          <dt>存储路径</dt>
          <dd>{status.storage_path}</dd>
        </div>
        <div>
          <dt>技能 / 会话</dt>
          <dd>{status.skill_count} / {status.conversation_count}</dd>
        </div>
      </dl>
    {:else}
      <p>状态加载中...</p>
    {/if}
  </section>

  <section class="panel">
    <h2>发送消息</h2>
    <form onsubmit={send}>
      <input
        id="message-input"
        placeholder="输入消息，或试试 /time、/echo hello"
        bind:value={message}
      />
      <button type="submit" disabled={loading}>
        {loading ? "发送中..." : "发送"}
      </button>
    </form>

    {#if response}
      <article class="response">
        <strong>Assistant</strong>
        <p>{response}</p>
      </article>
    {/if}
  </section>

  <section class="grid">
    <div class="panel">
      <h2>技能</h2>
      {#each skills as skill}
        <p><strong>{skill.name}</strong>：{skill.description}</p>
      {/each}
    </div>

    <div class="panel">
      <div class="panel-title">
        <h2>会话</h2>
        <button type="button" onclick={clearCurrentConversation}>清空当前</button>
      </div>
      {#if conversations.length === 0}
        <p>暂无会话</p>
      {:else}
        {#each conversations as conversation}
          <p>
            <strong>{conversation.id}</strong>
            <span>{conversation.messages.length} 条消息</span>
          </p>
        {/each}
      {/if}
    </div>
  </section>
</main>

<style>
:root {
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
  font-size: 16px;
  line-height: 24px;
  font-weight: 400;

  color: #0f0f0f;
  background-color: #f6f6f6;

  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  -webkit-text-size-adjust: 100%;
}

.container {
  max-width: 980px;
  margin: 0 auto;
  padding: 48px 24px;
  display: flex;
  flex-direction: column;
  gap: 20px;
}

header,
.panel-title,
form {
  display: flex;
  gap: 12px;
  align-items: center;
  justify-content: space-between;
}

.eyebrow {
  margin: 0;
  color: #396cd8;
  font-size: 13px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

h1,
h2,
p {
  margin-top: 0;
}

h1 {
  margin-bottom: 0;
  font-size: 42px;
}

h2 {
  font-size: 18px;
}

.panel {
  border: 1px solid #d8d8d8;
  border-radius: 16px;
  padding: 20px;
  background: #ffffff;
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.06);
}

.grid {
  display: grid;
  gap: 20px;
  grid-template-columns: repeat(2, minmax(0, 1fr));
}

dl {
  display: grid;
  gap: 16px;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  margin: 0;
}

dt {
  color: #666666;
  font-size: 13px;
}

dd {
  margin: 4px 0 0;
  overflow-wrap: anywhere;
  font-weight: 700;
}

input,
button {
  border-radius: 8px;
  border: 1px solid transparent;
  padding: 0.6em 1.2em;
  font-size: 1em;
  font-weight: 500;
  font-family: inherit;
  color: #0f0f0f;
  background-color: #ffffff;
  transition: border-color 0.25s;
  box-shadow: 0 2px 2px rgba(0, 0, 0, 0.2);
}

input {
  flex: 1;
}

button {
  cursor: pointer;
}

button:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}

button:hover {
  border-color: #396cd8;
}
button:active {
  border-color: #396cd8;
  background-color: #e8e8e8;
}

input,
button {
  outline: none;
}

.response,
.error {
  border-radius: 12px;
  padding: 14px;
}

.response {
  margin-top: 16px;
  background: #eef4ff;
}

.response p {
  margin-bottom: 0;
}

.error {
  color: #7a1f1f;
  background: #ffe7e7;
}

span {
  color: #666666;
}

@media (prefers-color-scheme: dark) {
  :root {
    color: #f6f6f6;
    background-color: #2f2f2f;
  }

  .panel {
    border-color: #444444;
    background: #1f1f1f;
  }

  .response {
    background: #162338;
  }

  .error {
    color: #ffd0d0;
    background: #4a1f1f;
  }

  dt,
  span {
    color: #b8b8b8;
  }

  input,
  button {
    color: #ffffff;
    background-color: #0f0f0f98;
  }
  button:active {
    background-color: #0f0f0f69;
  }
}

</style>
