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
    /** 0..1，相对当前 subgraph 权重范围归一化（渲染尺寸/配色同源）。 */
    weightNorm: number;
  };
};

/** 权重归一化：相对当前图 min/max 映射到 0..1（单节点图 = 1）。 */
export function weightNorm(weight: number, minW: number, maxW: number): number {
  const span = maxW - minW || 1;
  return (weight - minW) / span;
}

/** 节点尺寸 ∝ 归一化权重：宽度 140→260px，高度固定。布局斥力/碰撞与渲染共用。 */
export function nodeSizeFor(weight: number, minW: number, maxW: number): { w: number; h: number } {
  return { w: Math.round(140 + weightNorm(weight, minW, maxW) * 120), h: 56 };
}

/** 布局算法注册表：新增排版只需在此注册一个实现，画布工具栏自动列出。 */
export type LayoutId = "force" | "layered";

export type LayoutOptions = {
  seedId: string;
  minW: number;
  maxW: number;
  /** 与渲染同源的节点尺寸（内部会按需预计算，避免热循环内重复归一化）。 */
  nodeSize: (id: string) => { w: number; h: number };
};

export type LayoutAlgorithm = {
  id: LayoutId;
  labelKey: string;
  run: (subgraph: NeuronSubgraph, opts: LayoutOptions) => LayoutNode[];
};

const LAYOUT_PREF_KEY = "neuron-canvas-layout";

export function readLayoutPref(): LayoutId {
  try {
    return localStorage.getItem(LAYOUT_PREF_KEY) === "layered" ? "layered" : "force";
  } catch {
    return "force";
  }
}

export function writeLayoutPref(id: LayoutId): void {
  try {
    localStorage.setItem(LAYOUT_PREF_KEY, id);
  } catch {
    // 忽略持久化失败
  }
}

/** Layered layout for Svelte Flow (horizontal layers by depth). */
export function runLayeredLayout(
  subgraph: NeuronSubgraph,
  opts: LayoutOptions,
): LayoutNode[] {
  const depths = computeDepths(opts.seedId, subgraph.connections);
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
          isSeed: n.id === opts.seedId,
          weightNorm: weightNorm(n.weight, opts.minW, opts.maxW),
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
 *
 * Anti-crowding measures:
 * - Springs only act on a sparse skeleton (each node's top-k heaviest edges),
 *   so a near-complete graph no longer collapses into a blob.
 * - Repulsion uses node bounding boxes (effective distance), and a collide
 *   pass pushes overlapping nodes apart.
 */

/**
 * Sparse skeleton for spring forces: keep each node's top-k heaviest incident
 * edges (union). Low-weight edges still render but exert no pull, preventing
 * a near-complete graph from collapsing toward its center.
 */
export function selectSpringEdges(
  connections: Connection[],
  k = 3,
): Connection[] {
  const byNode = new Map<string, Connection[]>();
  for (const c of connections) {
    for (const id of [c.source, c.target]) {
      if (!byNode.has(id)) byNode.set(id, []);
      byNode.get(id)!.push(c);
    }
  }
  const chosen = new Set<Connection>();
  for (const list of byNode.values()) {
    const top = [...list].sort((a, b) => b.weight - a.weight).slice(0, k);
    for (const c of top) chosen.add(c);
  }
  return [...chosen];
}

export function runForceLayout(
  subgraph: NeuronSubgraph,
  opts: LayoutOptions,
): LayoutNode[] {
  const iterations = 400;
  const seed = 1337;

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

  // Node sizes (bbox) for effective-distance repulsion + collide pass
  const sizes = new Map<string, { w: number; h: number }>();
  for (const neuron of subgraph.neurons) {
    sizes.set(neuron.id, opts.nodeSize(neuron.id));
  }
  const radiusOf = (id: string) => {
    const { w, h } = sizes.get(id)!;
    return Math.hypot(w, h) / 4;
  };

  const pos = new Map<string, { x: number; y: number }>();
  ids.forEach((id, i) => {
    const angle = (i / n) * Math.PI * 2;
    const radius = 360 + (rand() - 0.5) * 160;
    pos.set(id, {
      x: Math.cos(angle) * radius + (rand() - 0.5) * 40,
      y: Math.sin(angle) * radius + (rand() - 0.5) * 40,
    });
  });

  // spring adjacency: sparse skeleton only (anti-collapse)
  const adj = new Map<string, Set<string>>();
  ids.forEach((id) => adj.set(id, new Set()));
  for (const c of selectSpringEdges(subgraph.connections)) {
    if (adj.has(c.source) && adj.has(c.target)) {
      adj.get(c.source)!.add(c.target);
      adj.get(c.target)!.add(c.source);
    }
  }

  // ideal distance: k lower-bounded by node width so springs don't pull into overlap
  const avgW = [...sizes.values()].reduce((acc, s) => acc + s.w, 0) / n;
  const area = Math.max(90000, n * 1600);
  const k = Math.max(Math.sqrt(area / n), avgW * 0.9);
  const repulse = 6000;
  const spring = 0.05;
  const damping = 0.85;

  const disp = new Map<string, { x: number; y: number }>();
  for (let it = 0; it < iterations; it++) {
    ids.forEach((id) => disp.set(id, { x: 0, y: 0 }));

    // repulsion (all pairs, effective distance = center dist - node radii)
    for (let i = 0; i < n; i++) {
      const a = ids[i];
      const pa = pos.get(a)!;
      const ra = radiusOf(a);
      for (let j = i + 1; j < n; j++) {
        const b = ids[j];
        const pb = pos.get(b)!;
        const rb = radiusOf(b);
        const dx = pa.x - pb.x;
        const dy = pa.y - pb.y;
        const dist = Math.hypot(dx, dy) || 0.01;
        const eff = Math.max(dist - (ra + rb), 1);
        const f = repulse / (eff * eff);
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

    // attraction (skeleton edges only)
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
    const limit = Math.max(24, 120 * damping ** (it / 30));
    ids.forEach((id) => {
      const d = disp.get(id)!;
      const len = Math.hypot(d.x, d.y) || 0.01;
      const scale = Math.min(len, limit) / len;
      const p = pos.get(id)!;
      p.x += d.x * scale;
      p.y += d.y * scale;
    });

    // collide pass: push overlapping bboxes apart along the least-overlap axis
    for (let i = 0; i < n; i++) {
      const a = ids[i];
      const sa = sizes.get(a)!;
      const pa = pos.get(a)!;
      for (let j = i + 1; j < n; j++) {
        const b = ids[j];
        const sb = sizes.get(b)!;
        const pb = pos.get(b)!;
        const dx = pb.x - pa.x;
        const dy = pb.y - pa.y;
        const overlapX = (sa.w + sb.w) / 2 - Math.abs(dx);
        const overlapY = (sa.h + sb.h) / 2 - Math.abs(dy);
        if (overlapX > 0 && overlapY > 0) {
          if (overlapX < overlapY) {
            const sign = dx === 0 ? 1 : Math.sign(dx);
            pa.x -= sign * (overlapX / 2);
            pb.x += sign * (overlapX / 2);
          } else {
            const sign = dy === 0 ? 1 : Math.sign(dy);
            pa.y -= sign * (overlapY / 2);
            pb.y += sign * (overlapY / 2);
          }
        }
      }
    }
  }

  return subgraph.neurons.map((neuron) => ({
    id: neuron.id,
    position: pos.get(neuron.id)!,
    data: {
      label: neuron.desc || neuron.id.slice(0, 8),
      weight: neuron.weight,
      systemType: neuron.system_type,
      isSeed: neuron.id === opts.seedId,
      weightNorm: weightNorm(neuron.weight, opts.minW, opts.maxW),
    },
  }));
}

/** 布局算法注册表：切换/新增排版在此登记，画布工具栏自动列出。 */
export const layoutRegistry: Record<LayoutId, LayoutAlgorithm> = {
  force: { id: "force", labelKey: "neuronPanel.layoutForce", run: runForceLayout },
  layered: { id: "layered", labelKey: "neuronPanel.layoutLayered", run: runLayeredLayout },
};

export const layoutOptions: LayoutAlgorithm[] = Object.values(layoutRegistry);
