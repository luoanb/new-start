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
          <span class="caret" class:open={!collapsed[group.type]}>▸</span>
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
              <span
                class="type-bar"
                style:background={`var(--color-system-${n.system_type || "default"}, var(--color-system-default))`}
              ></span>
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
    padding: 6px 4px 16px;
    gap: 2px;
  }

  .index-empty {
    color: var(--color-text-muted);
    font-size: 12px;
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
    padding: 4px 6px;
    color: var(--color-text-muted);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .group-head:hover {
    color: var(--color-text);
  }

  .caret {
    transition: transform 0.18s ease;
    font-size: 9px;
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
    gap: 8px;
    width: 100%;
    background: none;
    border: none;
    border-radius: 8px;
    cursor: pointer;
    padding: 5px 6px 5px 4px;
    text-align: left;
    transition: background 0.15s ease;
  }
  .index-row:hover {
    background: var(--color-hover);
  }
  .index-row.selected {
    background: var(--color-selected);
  }

  .type-bar {
    width: 3px;
    border-radius: 2px;
    flex-shrink: 0;
  }

  .row-main {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .row-name {
    font-size: 12.5px;
    color: var(--color-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-meta {
    display: flex;
    gap: 8px;
    font-size: 10.5px;
    color: var(--color-text-muted);
  }
  .row-meta .links {
    opacity: 0.85;
  }
</style>
