<script lang="ts">
  // 全局 root 授权消费器：后端「命令因权限不足失败」时入队，全屏压暗提示用户决定
  // （授权 = 允许以 root 重跑，root 与密码由系统原生提权程序接管）。每次弹一个，
  // resolve 后弹下一个。挂在 +page.svelte 组合根，跨面板常驻且阻断其余交互。
  import { onMount } from "svelte";
  import { dataStore } from "$lib/stores/dataStore.svelte";
  import { t } from "$lib/i18n";

  const request = $derived(dataStore.state.rootAuthQueue[0] ?? null);

  function resolve(approved: boolean) {
    const req = request;
    if (req) void dataStore.rootAuthResolve(req.request_id, approved);
  }

  // 遮罩点击不关闭（必须明确选择）；Esc = 拒绝；Enter 故意不绑定，避免误触授权。
  function handleKeydown(e: KeyboardEvent) {
    if (!request) return;
    if (e.key === "Escape") {
      e.preventDefault();
      resolve(false);
    }
  }

  onMount(() => {
    window.addEventListener("keydown", handleKeydown);
    return () => window.removeEventListener("keydown", handleKeydown);
  });
</script>

{#if request}
  <div
    class="root-auth-overlay"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="root-auth-title"
  >
    <div class="root-auth-panel">
      <h2 id="root-auth-title">{t("rootAuth.title")}</h2>
      <p class="body">{t("rootAuth.body")}</p>

      <dl class="facts">
        <dt>{t("rootAuth.commandLabel")}</dt>
        <dd><code class="command">{request.command}</code></dd>
        {#if request.cwd}
          <dt>{t("rootAuth.cwdLabel")}</dt>
          <dd><code class="path">{request.cwd}</code></dd>
        {/if}
        {#if request.stderr_excerpt}
          <dt>{t("rootAuth.reasonLabel")}</dt>
          <dd class="reason">{request.stderr_excerpt}</dd>
        {/if}
      </dl>

      <p class="note">{t("rootAuth.mechanismNote")}</p>

      <div class="actions">
        <button class="btn" onclick={() => resolve(false)}>{t("rootAuth.deny")}</button>
        <button class="btn btn-primary" onclick={() => resolve(true)}>
          {t("rootAuth.authorize")}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  /* 全屏遮罩：高于普通弹窗（ConfirmDialog z-index 100）与加载层（200），
     确保安全决策期间其余界面不可交互。 */
  .root-auth-overlay {
    position: fixed;
    inset: 0;
    z-index: 300;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-4);
  }
  .root-auth-panel {
    width: min(640px, 92vw);
    max-height: 90vh;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-5);
    background: var(--color-surface);
    border: var(--border-width) solid var(--color-warning);
    border-radius: var(--radius-lg);
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
  }
  h2 {
    margin: 0;
    font-size: var(--fs-lg);
    font-weight: 600;
  }
  .body {
    margin: 0;
    font-size: var(--fs-base);
    color: var(--color-text);
    line-height: 1.6;
  }
  .facts {
    margin: 0;
    display: grid;
    grid-template-columns: auto;
    gap: var(--space-1);
  }
  .facts dt {
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .facts dd {
    margin: 0 0 var(--space-1);
  }
  .command,
  .path,
  .reason {
    display: block;
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    background: var(--color-elevated);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    padding: var(--space-2);
    white-space: pre-wrap;
    word-break: break-all;
  }
  .reason {
    color: var(--color-text-muted);
  }
  .note {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--color-text-muted);
    line-height: 1.6;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
