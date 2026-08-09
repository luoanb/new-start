<script lang="ts">
  import type { ViewRegistration } from "./views";

  // ViewHost 是纯动态挂载器：组合根（+page.svelte）已 setViewContext，
  // 视图组件直接通过 useViewContext() 自取数据，无需在此重复注入。
  let { registration }: { registration: ViewRegistration | undefined } = $props();
</script>

{#if registration}
  {@const Comp = registration.component}
  <Comp />
{/if}
