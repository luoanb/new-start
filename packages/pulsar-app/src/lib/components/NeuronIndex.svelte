<script lang="ts">
  import type { Neuron } from "$lib/types";
  import { t } from "$lib/i18n";

  export let neurons: Neuron[] = [];
  export let selectedId: string | null = null;
  export let linkCounts: Record<string, number> = {};
  export let onSelect: (id: string) => void = () => {};

  // 按 system_type 分组，便于折叠浏览
  type Group = { type: string; items: Neuron[] };
  let collapsed: Record<string, boolean> = {};

  $: groups = (() => {
    const map = new Map<string, Neuron[]>();
    for (const n of neurons) {
      const key = n.system_type || "uncategorized";
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(n);
    }
    const result: Group[] = [];
    for (const [type, items] of map) {
      items.sort((a, b) => b.weight - a.weight);
      result.push({ type, items });
    }
    result.sort((a, b) => a.type.localeCompare(b.type));
    return result;
  })();

  function toggle(type: string) {
    collapsed[type] = !collapsed[type];
    collapsed = { ...collapsed };
  }

  function fmtTime(ts: number): string {
    if (!ts) return "";
    const d = new Date(ts * 1000);
    return `${d.getMonth() + 1}/${d.getDate()}`;
  }
</script>

<div class="neuron-index">
  {#if neurons.length === 0}
    <div class="index-empty">{t("neuronPanel.noNeurons")}</div>
  {:else}
    {#each groups as group (group.type)}
      <div class="group">
        <button class="group-head" onclick={() => toggle(group.type)}>
          <span class="caret" class:open={!collapsed[group.type]} aria-hidden="true">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6" /></svg>
          </span>
          <span class="type-name">{group.type}</span>
          <span class="type-count">{group.items.length}</span>
        </button>
        {#if !collapsed[group.type]}
          {#each group.items as n (n.id)}
            <button
              class="index-row"
              class:selected={n.id === selectedId}
              onclick={() => onSelect(n.id)}
              title={n.desc}
            >
              <span class="row-main">
                <span class="row-name">{n.desc || n.id}</span>
                <span class="row-meta">
                  <span class="weight">w{n.weight.toFixed(3)}</span>
                  <span class="links"
                    >{t("neuronPanel.connectionsCount", {
                      count: linkCounts[n.id] ?? 0,
                    })}</span
                  >
                  <span class="time">{fmtTime(n.created_at)}</span>
                </span>
              </span>
            </button>
          {/each}
        {/if}
      </div>
    {/each}
  {/if}
</div>

<style>
  .neuron-index {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow-y: auto;
    padding: var(--space-1) 0 var(--space-4);
    gap: 2px;
  }

  .index-empty {
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
    padding: 16px 10px;
    text-align: center;
  }

  .group {
    margin-bottom: 4px;
  }

  .group-head {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    background: none;
    border: none;
    cursor: pointer;
    padding: var(--space-1) var(--space-2);
    color: var(--color-text-muted);
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .group-head:hover {
    color: var(--color-text);
  }

  .caret {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    color: var(--color-text-muted);
    transition: transform var(--duration-fast) var(--ease-out);
  }
  .caret svg {
    width: 10px;
    height: 10px;
  }
  .caret.open {
    transform: rotate(90deg);
  }

  .type-name {
    flex: 1;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .type-count {
    opacity: 0.6;
  }

  .index-row {
    display: flex;
    align-items: stretch;
    gap: var(--space-2);
    width: 100%;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    padding: var(--space-1) var(--space-2);
    text-align: left;
    transition: background var(--duration-fast) var(--ease-out);
  }
  .index-row:hover {
    background: var(--color-hover);
  }
  .index-row.selected {
    background: color-mix(in oklch, var(--color-primary) 14%, transparent);
  }

  .row-main {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .row-name {
    font-size: var(--fs-sm);
    color: var(--color-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-meta {
    display: flex;
    gap: 8px;
    font-size: var(--fs-xs);
    color: var(--color-text-muted);
  }
  .row-meta .links {
    opacity: 0.85;
  }
</style>
