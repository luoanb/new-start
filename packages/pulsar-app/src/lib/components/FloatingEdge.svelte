<script lang="ts">
  import {
    BaseEdge,
    getBezierPath,
    getStraightPath,
    getSmoothStepPath,
    useInternalNode,
    Position,
  } from "@xyflow/svelte";
  import type { EdgeProps } from "@xyflow/svelte";

  let {
    id,
    source,
    target,
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
    markerEnd,
    style,
    label,
    data,
  }: EdgeProps = $props();

  // floating edge variant: auto connects to the nearest edge point of each node
  let variant = $derived<string>((data as { variant?: string } | undefined)?.variant ?? "bezier");

  // 边实例的 source/target 固定；xyflow 的 useInternalNode 需在组件初始化时调用一次
  // svelte-ignore state_referenced_locally
  const sourceNode = useInternalNode(source);
  // svelte-ignore state_referenced_locally
  const targetNode = useInternalNode(target);

  type Pt = { x: number; y: number };

  // choose the nearest side handle position on a node rect for a given direction
  function nearestHandle(node: { internals: { positionAbsolute: Pt }; measured: { width: number; height: number } }, towards: Pt) {
    const x = node.internals.positionAbsolute.x;
    const y = node.internals.positionAbsolute.y;
    const w = node.measured.width || 0;
    const h = node.measured.height || 0;
    const cx = x + w / 2;
    const cy = y + h / 2;
    const dx = towards.x - cx;
    const dy = towards.y - cy;
    if (Math.abs(dx) * h > Math.abs(dy) * w) {
      // connect left/right
      if (dx > 0) return { x: x + w, y: cy, position: Position.Right };
      return { x, y: cy, position: Position.Left };
    }
    // connect top/bottom
    if (dy > 0) return { x: cx, y: y + h, position: Position.Bottom };
    return { x: cx, y, position: Position.Top };
  }

  let path = $derived.by(() => {
    const sn = sourceNode.current;
    const tn = targetNode.current;
    if (!sn || !tn) {
      // fallback to provided endpoints
      if (variant === "straight") {
        const [p] = getStraightPath({ sourceX, sourceY, targetX, targetY });
        return p;
      }
      if (variant === "smoothstep" || variant === "step") {
        const [p] = getSmoothStepPath({ sourceX, sourceY, targetX, targetY, borderRadius: variant === "step" ? 0 : 12 });
        return p;
      }
      const [p] = getBezierPath({ sourceX, sourceY, targetX, targetY });
      return p;
    }
    const sCenter = { x: sn.internals.positionAbsolute.x + (sn.measured.width ?? 0) / 2, y: sn.internals.positionAbsolute.y + (sn.measured.height ?? 0) / 2 };
    const tCenter = { x: tn.internals.positionAbsolute.x + (tn.measured.width ?? 0) / 2, y: tn.internals.positionAbsolute.y + (tn.measured.height ?? 0) / 2 };
    const sh = nearestHandle(sn as any, tCenter);
    const th = nearestHandle(tn as any, sCenter);
    if (variant === "straight") {
      const [p] = getStraightPath({ sourceX: sh.x, sourceY: sh.y, targetX: th.x, targetY: th.y });
      return p;
    }
    if (variant === "smoothstep" || variant === "step") {
      const [p] = getSmoothStepPath({ sourceX: sh.x, sourceY: sh.y, sourcePosition: sh.position, targetX: th.x, targetY: th.y, targetPosition: th.position, borderRadius: variant === "step" ? 0 : 12 });
      return p;
    }
    const [p] = getBezierPath({ sourceX: sh.x, sourceY: sh.y, sourcePosition: sh.position, targetX: th.x, targetY: th.y, targetPosition: th.position });
    return p;
  });
</script>

<BaseEdge
  {id}
  {path}
  {markerEnd}
  {style}
  {label}
/>