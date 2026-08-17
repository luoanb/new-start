<script lang="ts">
  import { setContext } from "svelte";
  import type { ViewRegistration } from "./views";
  import type { MainPanel } from "./layoutTypes";

  // ViewHost 是纯动态挂载器：组合根（+page.svelte）已 setViewContext，
  // 视图组件直接通过 useViewContext() 自取数据，无需在此重复注入。
  // panel（可选）：多实例面板（file-editor 按文件路径多开）通过 context 获取自身面板实例
  // （panel.id 即实例 key），单实例视图不依赖此 context。
  let { registration, panel }: { registration: ViewRegistration | undefined; panel?: MainPanel } =
    $props();
  if (panel) setContext("pulsar:panel", panel);
</script>

{#if registration}
  {@const Comp = registration.component}
  <Comp />
{/if}
