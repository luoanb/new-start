import type { Connection, Neuron, NeuronSubgraph } from "$lib/types";

export type NetworkTreeRow = {
  neuron: Neuron;
  depth: number;
  /** Edge from BFS parent toward this node when known */
  fromParent: Connection | null;
  /** Direction relative to parent traversal */
  direction: "out" | "in" | "seed";
};

/** Undirected BFS depths from seed using subgraph edges. */
export function computeDepths(
  seedId: string,
  connections: Connection[],
): Map<string, number> {
  const adj = new Map<string, string[]>();
  for (const c of connections) {
    if (!adj.has(c.source)) adj.set(c.source, []);
    if (!adj.has(c.target)) adj.set(c.target, []);
    adj.get(c.source)!.push(c.target);
    adj.get(c.target)!.push(c.source);
  }
  const depths = new Map<string, number>();
  const queue: string[] = [seedId];
  depths.set(seedId, 0);
  while (queue.length) {
    const cur = queue.shift()!;
    const d = depths.get(cur)!;
    for (const n of adj.get(cur) ?? []) {
      if (!depths.has(n)) {
        depths.set(n, d + 1);
        queue.push(n);
      }
    }
  }
  return depths;
}

/** Build tree rows for network list view. */
export function buildTreeRows(subgraph: NeuronSubgraph): NetworkTreeRow[] {
  const { seed_id, neurons, connections } = subgraph;
  const byId = new Map(neurons.map((n) => [n.id, n]));
  const depths = computeDepths(seed_id, connections);

  // parent via undirected BFS predecessor
  const parent = new Map<string, string>();
  const adj: { id: string; other: string; conn: Connection }[] = [];
  for (const c of connections) {
    adj.push({ id: c.source, other: c.target, conn: c });
    adj.push({ id: c.target, other: c.source, conn: c });
  }
  const seen = new Set<string>([seed_id]);
  const q = [seed_id];
  while (q.length) {
    const cur = q.shift()!;
    for (const edge of adj.filter((a) => a.id === cur)) {
      if (seen.has(edge.other)) continue;
      seen.add(edge.other);
      parent.set(edge.other, cur);
      q.push(edge.other);
    }
  }

  const rows: NetworkTreeRow[] = [];
  // order by depth then desc
  const ordered = [...neurons].sort((a, b) => {
    const da = depths.get(a.id) ?? 999;
    const db = depths.get(b.id) ?? 999;
    if (da !== db) return da - db;
    return b.weight - a.weight;
  });

  for (const neuron of ordered) {
    const depth = depths.get(neuron.id) ?? 0;
    if (neuron.id === seed_id) {
      rows.push({ neuron, depth: 0, fromParent: null, direction: "seed" });
      continue;
    }
    const p = parent.get(neuron.id);
    let fromParent: Connection | null = null;
    let direction: "out" | "in" | "seed" = "out";
    if (p) {
      fromParent =
        connections.find((c) => c.source === p && c.target === neuron.id) ??
        connections.find((c) => c.source === neuron.id && c.target === p) ??
        null;
      if (fromParent) {
        direction = fromParent.source === p ? "out" : "in";
      }
    }
    rows.push({ neuron, depth, fromParent, direction });
  }
  return rows;
}

export type LayoutNode = {
  id: string;
  position: { x: number; y: number };
  data: {
    label: string;
    weight: number;
    systemType?: string | null;
    isSeed: boolean;
  };
};

/** Layered layout for Svelte Flow (horizontal layers by depth). */
export function layoutFlowNodes(subgraph: NeuronSubgraph): LayoutNode[] {
  const depths = computeDepths(subgraph.seed_id, subgraph.connections);
  const byDepth = new Map<number, Neuron[]>();
  for (const n of subgraph.neurons) {
    const d = depths.get(n.id) ?? 0;
    if (!byDepth.has(d)) byDepth.set(d, []);
    byDepth.get(d)!.push(n);
  }
  for (const list of byDepth.values()) {
    list.sort((a, b) => b.weight - a.weight);
  }

  const xGap = 220;
  const yGap = 90;
  const nodes: LayoutNode[] = [];
  for (const [depth, list] of [...byDepth.entries()].sort((a, b) => a[0] - b[0])) {
    const totalH = (list.length - 1) * yGap;
    list.forEach((n, i) => {
      nodes.push({
        id: n.id,
        position: {
          x: depth * xGap,
          y: i * yGap - totalH / 2,
        },
        data: {
          label: n.desc || n.id.slice(0, 8),
          weight: n.weight,
          systemType: n.system_type,
          isSeed: n.id === subgraph.seed_id,
        },
      });
    });
  }
  return nodes;
}
