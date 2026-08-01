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

/**
 * Force-directed layout (deterministic). Models the neuron graph as a directed
 * graph, NOT a tree: nodes are freely placed via repulsion + spring forces over
 * fixed iterations. The seeded PRNG keeps coordinates stable across re-renders.
 */
export function layoutForceNodes(
  subgraph: NeuronSubgraph,
  options?: { iterations?: number; seed?: number },
): LayoutNode[] {
  const iterations = options?.iterations ?? 300;
  const seed = options?.seed ?? 1337;

  const ids = subgraph.neurons.map((n) => n.id);
  const n = ids.length;
  if (n === 0) return [];

  // Deterministic PRNG (mulberry32) → stable initial positions
  let s = seed >>> 0;
  const rand = () => {
    s |= 0;
    s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };

  const pos = new Map<string, { x: number; y: number }>();
  ids.forEach((id, i) => {
    const angle = (i / n) * Math.PI * 2;
    const radius = 260 + (rand() - 0.5) * 120;
    pos.set(id, {
      x: Math.cos(angle) * radius + (rand() - 0.5) * 40,
      y: Math.sin(angle) * radius + (rand() - 0.5) * 40,
    });
  });

  // adjacency for spring forces
  const adj = new Map<string, Set<string>>();
  ids.forEach((id) => adj.set(id, new Set()));
  for (const c of subgraph.connections) {
    if (adj.has(c.source) && adj.has(c.target)) {
      adj.get(c.source)!.add(c.target);
      adj.get(c.target)!.add(c.source);
    }
  }

  const area = Math.max(90000, n * 1600);
  const k = Math.sqrt(area / n); // ideal distance
  const repulse = 6000;
  const spring = 0.05;
  const damping = 0.85;

  const disp = new Map<string, { x: number; y: number }>();
  for (let it = 0; it < iterations; it++) {
    ids.forEach((id) => disp.set(id, { x: 0, y: 0 }));

    // repulsion (all pairs)
    for (let i = 0; i < n; i++) {
      const a = ids[i];
      const pa = pos.get(a)!;
      for (let j = i + 1; j < n; j++) {
        const b = ids[j];
        const pb = pos.get(b)!;
        let dx = pa.x - pb.x;
        let dy = pa.y - pb.y;
        let dist = Math.hypot(dx, dy) || 0.01;
        const f = repulse / (dist * dist);
        const ux = (dx / dist) * f;
        const uy = (dy / dist) * f;
        const da = disp.get(a)!;
        const db = disp.get(b)!;
        da.x += ux;
        da.y += uy;
        db.x -= ux;
        db.y -= uy;
      }
    }

    // attraction (edges)
    for (const [a, neighbors] of adj) {
      const pa = pos.get(a)!;
      const da = disp.get(a)!;
      for (const b of neighbors) {
        const pb = pos.get(b)!;
        const dx = pa.x - pb.x;
        const dy = pa.y - pb.y;
        const dist = Math.hypot(dx, dy) || 0.01;
        const f = (dist - k) * spring;
        const ux = (dx / dist) * f;
        const uy = (dy / dist) * f;
        da.x -= ux;
        da.y -= uy;
      }
    }

    // apply with length limit + damping
    const limit = 80 * damping ** (it / 30);
    ids.forEach((id) => {
      const d = disp.get(id)!;
      const len = Math.hypot(d.x, d.y) || 0.01;
      const scale = Math.min(len, limit) / len;
      const p = pos.get(id)!;
      p.x += d.x * scale;
      p.y += d.y * scale;
    });
  }

  return subgraph.neurons.map((neuron) => ({
    id: neuron.id,
    position: pos.get(neuron.id)!,
    data: {
      label: neuron.desc || neuron.id.slice(0, 8),
      weight: neuron.weight,
      systemType: neuron.system_type,
      isSeed: neuron.id === subgraph.seed_id,
    },
  }));
}
