import { useEffect, useMemo, useState } from "react";
import { Background, Controls, MarkerType, ReactFlow, type Edge, type Node } from "@xyflow/react";
import type { EvolutionTree } from "../contracts";

type GraphData = { label: string; active: boolean; root: boolean };

export function EvolutionGraph({ tree, onSelect }: { tree: EvolutionTree; onSelect: (id: string) => void }) {
  const [nodes, setNodes] = useState<Node<GraphData>[]>([]);
  const edges = useMemo<Edge[]>(
    () =>
      tree.nodes.flatMap((node) =>
        node.parents.map((parent, index) => ({
          id: `${parent.from}:${node.id}:${index}`,
          source: parent.from,
          target: node.id,
          type: "smoothstep",
          animated: node.id === tree.active && parent.kind === "trunk",
          style: {
            stroke: parent.kind === "reference" ? "var(--edge-reference)" : "var(--edge-trunk)",
            strokeDasharray: parent.kind === "reference" ? "7 6" : undefined,
            strokeWidth: parent.kind === "reference" ? 1.4 : 2.2,
          },
          markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16 },
        })),
      ),
    [tree],
  );

  useEffect(() => {
    let cancelled = false;
    const layout = async () => {
      // ELK is much larger than the workbench shell; load it only when a tree
      // actually needs layout instead of putting it on the launch path.
      const { default: ELK } = await import("elkjs/lib/elk.bundled.js");
      const elk = new ELK();
      const result = await elk.layout({
        id: "root",
        layoutOptions: {
          "elk.algorithm": "layered",
          "elk.direction": "RIGHT",
          "elk.spacing.nodeNode": "44",
          "elk.layered.spacing.nodeNodeBetweenLayers": "88",
        },
        children: tree.nodes.map((node) => ({ id: node.id, width: 172, height: 62 })),
        edges: edges.map((edge) => ({ id: edge.id, sources: [edge.source], targets: [edge.target] })),
      });
      if (cancelled) return;
      const source = new Map(tree.nodes.map((node) => [node.id, node]));
      setNodes(
        (result.children ?? []).map((node) => {
          const original = source.get(node.id)!;
          return {
            id: node.id,
            position: { x: node.x ?? 0, y: node.y ?? 0 },
            data: {
              label: original.label ?? original.id.slice(0, 10),
              active: original.id === tree.active,
              root: original.parents.length === 0,
            },
            className: original.id === tree.active ? "graph-node active" : "graph-node",
          };
        }),
      );
    };
    void layout();
    return () => { cancelled = true; };
  }, [edges, tree]);

  return (
    <div className="graph-canvas">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        fitView
        minZoom={0.3}
        maxZoom={1.8}
        nodesDraggable={false}
        nodesConnectable={false}
        onNodeClick={(_, node) => onSelect(node.id)}
        proOptions={{ hideAttribution: true }}
      >
        <Background gap={24} size={1} />
        <Controls showInteractive={false} />
      </ReactFlow>
    </div>
  );
}
