<script lang="ts">
  // 全局 git 写操作确认消费器：监听 dataStore.state.gitConfirmQueue（后端确认服务入队，
  // GitPanel / GitDiff / FileExplorer 徽标触发的写操作统一经此弹窗），每次弹一个，
  // resolve 后出队弹下一个。挂在 +page.svelte 组合根，跨面板常驻。
  import { dataStore } from "$lib/stores/dataStore.svelte";
  import { t } from "$lib/i18n";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import type { GitConfirmRequest } from "$lib/types";

  /** 按 kind 渲染确认内容：常规写（commit/push/pull/stash apply）= 摘要；高危写 = danger + 受影响文件清单。 */
  function describe(req: GitConfirmRequest): { title: string; message: string; danger: boolean } {
    const d = (req.detail ?? {}) as Record<string, unknown>;
    const list = (key: string): string[] =>
      Array.isArray(d[key]) ? (d[key] as string[]).map(String) : [];
    const currentBranch = dataStore.state.git?.status?.branch ?? "HEAD";
    switch (req.kind) {
      case "Commit": {
        const files = list("staged_files");
        let message = t("git.commitConfirmBody", { n: files.length });
        if (files.length > 0) message += "\n" + files.map((f) => `  • ${f}`).join("\n");
        return { title: t("git.commitConfirmTitle"), message, danger: false };
      }
      case "Push":
        return {
          title: t("git.pushConfirmTitle"),
          message: t("git.pushConfirmBody", {
            branch: String(d.branch ?? currentBranch),
            ahead: Number(d.ahead ?? 0),
          }),
          danger: false,
        };
      case "Pull":
        return {
          title: t("git.pullConfirmTitle"),
          message: t("git.pullConfirmBody", { branch: currentBranch }),
          danger: false,
        };
      case "Reset":
      case "Checkout": {
        // Reset 携带 lost 清单；restore/丢弃携带 paths 清单；分支切换仅携带 target。
        const files = [...list("lost"), ...list("paths")];
        if (files.length === 0 && req.kind === "Checkout" && d.target) {
          return {
            title: t("git.discardConfirmTitle"),
            message: t("git.checkoutConfirmBody", { target: String(d.target) }),
            danger: true,
          };
        }
        let message = t("git.discardConfirmBody", { n: files.length });
        if (files.length > 0) message += "\n" + files.map((f) => `  • ${f}`).join("\n");
        return { title: t("git.discardConfirmTitle"), message, danger: true };
      }
      case "StashApply":
        return { title: t("git.stashApplyConfirmTitle"), message: t("git.stashApplyConfirmBody"), danger: false };
      case "StashDrop":
        return { title: t("git.stashDropConfirmTitle"), message: t("git.stashDropConfirmBody"), danger: true };
      default:
        return { title: req.title, message: "", danger: false };
    }
  }

  const dialog = $derived.by(() => {
    const req = dataStore.state.gitConfirmQueue[0];
    if (!req) return null;
    const d = describe(req);
    return { req, ...d };
  });

  function resolve(approved: boolean) {
    const d = dialog;
    if (d) void dataStore.gitConfirm(d.req.op_id, approved);
  }
</script>

{#if dialog}
  <ConfirmDialog
    open={true}
    title={dialog.title}
    message={dialog.message}
    danger={dialog.danger}
    confirmLabel={dialog.danger ? t("git.confirmDiscard") : undefined}
    onConfirm={() => resolve(true)}
    onCancel={() => resolve(false)}
  />
{/if}
