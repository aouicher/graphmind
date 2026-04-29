import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import ForceGraph2D, { type ForceGraphMethods } from "react-force-graph-2d";
import ForceGraph3D from "react-force-graph-3d";
import { Filter, Maximize2, Minimize2, Box, Square, Search } from "lucide-react";
import { api, GraphData, GraphNode } from "../lib/tauri";
import { useProjects } from "../hooks/useProjects";
import { Button } from "../components/ui/Button";
import { Spinner } from "../components/ui/Spinner";

type HeatmapMode = "connections" | "kind" | "file";
type ViewMode = "2d" | "3d";

const KIND_COLORS: Record<string, string> = {
  function: "#6366f1",
  method: "#818cf8",
  class: "#f59e0b",
  interface: "#22c55e",
  struct: "#ef4444",
  enum: "#ec4899",
  type: "#14b8a6",
  constant: "#64748b",
  variable: "#94a3b8",
  module: "#a78bfa",
  trait: "#f97316",
  impl: "#fb923c",
};

const FILE_PALETTE = [
  "#6366f1", "#22c55e", "#f59e0b", "#ef4444", "#ec4899",
  "#14b8a6", "#a78bfa", "#f97316", "#06b6d4", "#84cc16",
  "#e879f9", "#fb923c", "#64748b", "#fbbf24", "#34d399",
];

function getConnectionColor(connections: number, max: number): string {
  const t = max > 0 ? Math.min(connections / max, 1) : 0;
  const r = Math.round(99 + t * (239 - 99));
  const g = Math.round(102 + t * (68 - 102));
  const b = Math.round(241 + t * (68 - 241));
  return `rgb(${r}, ${g}, ${b})`;
}

export function Graph() {
  const { projects } = useProjects();
  const [selectedProject, setSelectedProject] = useState<string>("");
  const [graphData, setGraphData] = useState<GraphData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [fileFilter, setFileFilter] = useState<string>("");
  const [kindFilter, setKindFilter] = useState<string>("");
  const [languageFilter, setLanguageFilter] = useState<string>("");
  const [heatmap, setHeatmap] = useState<HeatmapMode>("connections");
  const [showFilters, setShowFilters] = useState(true);
  const [hoveredNode, setHoveredNode] = useState<GraphNode | null>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const [viewMode, setViewMode] = useState<ViewMode>("2d");
  const [searchQuery, setSearchQuery] = useState("");
  const [minConnections, setMinConnections] = useState(0);
  const [nodeLimit, setNodeLimit] = useState(1500);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const graphRef = useRef<ForceGraphMethods<{ id: string }> | undefined>(undefined);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (projects.length > 0 && !selectedProject) {
      setSelectedProject(projects[0].slug);
    }
  }, [projects, selectedProject]);

  const loadGraph = useCallback(async () => {
    if (!selectedProject) return;
    setLoading(true);
    setError(null);
    try {
      const data = await api.getGraphData(
        selectedProject,
        fileFilter || undefined,
        kindFilter || undefined,
        languageFilter || undefined,
        nodeLimit
      );
      setGraphData(data);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [selectedProject, fileFilter, kindFilter, languageFilter, nodeLimit]);

  useEffect(() => {
    loadGraph();
  }, [loadGraph]);

  const maxConnections = useMemo(
    () => graphData?.nodes.reduce((m, n) => Math.max(m, n.connections), 0) ?? 0,
    [graphData]
  );

  const fileColorMap = useMemo(() => {
    if (!graphData) return new Map<string, string>();
    const files = [...new Set(graphData.nodes.map((n) => n.file))];
    const map = new Map<string, string>();
    files.forEach((f, i) => map.set(f, FILE_PALETTE[i % FILE_PALETTE.length]));
    return map;
  }, [graphData]);

  // Connected node IDs for highlight
  const connectedNodeIds = useMemo(() => {
    if (!selectedNodeId || !graphData) return new Set<string>();
    const ids = new Set<string>([selectedNodeId]);
    for (const edge of graphData.edges) {
      if (edge.source === selectedNodeId) ids.add(edge.target);
      if (edge.target === selectedNodeId) ids.add(edge.source);
    }
    return ids;
  }, [selectedNodeId, graphData]);

  const getNodeColor = useCallback(
    (node: GraphNode): string => {
      // Dim non-connected nodes when a node is selected
      if (selectedNodeId && !connectedNodeIds.has(node.id)) {
        return "rgba(60, 60, 60, 0.4)";
      }
      switch (heatmap) {
        case "connections":
          return getConnectionColor(node.connections, maxConnections);
        case "kind":
          return KIND_COLORS[node.kind.toLowerCase()] ?? "#666666";
        case "file":
          return fileColorMap.get(node.file) ?? "#666666";
      }
    },
    [heatmap, maxConnections, fileColorMap, selectedNodeId, connectedNodeIds]
  );

  const getLinkColor = useCallback(
    (link: any): string => {
      if (!selectedNodeId) return "rgba(255, 255, 255, 0.18)";
      const src = typeof link.source === "object" ? link.source.id : link.source;
      const tgt = typeof link.target === "object" ? link.target.id : link.target;
      if (src === selectedNodeId || tgt === selectedNodeId) {
        return "rgba(255, 255, 255, 0.6)";
      }
      return "rgba(255, 255, 255, 0.06)";
    },
    [selectedNodeId]
  );

  const getLinkWidth = useCallback(
    (link: any): number => {
      if (!selectedNodeId) return 1.5;
      const src = typeof link.source === "object" ? link.source.id : link.source;
      const tgt = typeof link.target === "object" ? link.target.id : link.target;
      if (src === selectedNodeId || tgt === selectedNodeId) return 2.5;
      return 0.5;
    },
    [selectedNodeId]
  );

  // Filtered graph data (min connections + search)
  const forceGraphData = useMemo(() => {
    if (!graphData) return { nodes: [], links: [] };
    const filteredNodes = graphData.nodes.filter((n) => n.connections >= minConnections);
    const nodeIds = new Set(filteredNodes.map((n) => n.id));
    const filteredEdges = graphData.edges.filter(
      (e) => nodeIds.has(e.source) && nodeIds.has(e.target)
    );
    return {
      nodes: filteredNodes.map((n) => ({ ...n })),
      links: filteredEdges.map((e) => ({ ...e })),
    };
  }, [graphData, minConnections]);

  // Search: find matching nodes
  const searchResults = useMemo(() => {
    if (!searchQuery || !graphData) return [];
    const q = searchQuery.toLowerCase();
    return graphData.nodes
      .filter((n) => n.name.toLowerCase().includes(q))
      .slice(0, 8);
  }, [searchQuery, graphData]);

  const handleSearchSelect = (node: GraphNode) => {
    setSearchQuery("");
    setSelectedNodeId(node.id);
    if (graphRef.current && viewMode === "2d") {
      const graphNode = forceGraphData.nodes.find((n) => n.id === node.id) as any;
      if (graphNode?.x != null && graphNode?.y != null) {
        graphRef.current.centerAt(graphNode.x, graphNode.y, 400);
        graphRef.current.zoom(4, 400);
      }
    }
  };

  const handleNodeClick = (node: any) => {
    const id = (node as GraphNode).id;
    if (selectedNodeId === id) {
      setSelectedNodeId(null);
    } else {
      setSelectedNodeId(id);
      if (graphRef.current && viewMode === "2d") {
        graphRef.current.centerAt(node.x!, node.y!, 400);
        graphRef.current.zoom(4, 400);
      }
    }
  };

  const handleBackgroundClick = () => {
    setSelectedNodeId(null);
  };

  const nodeCanvasObject = useCallback(
    (node: any, ctx: CanvasRenderingContext2D, globalScale: number) => {
      const gNode = node as GraphNode & { x: number; y: number };
      const size = Math.max(3, Math.min(12, 3 + (gNode.connections / Math.max(maxConnections, 1)) * 9));
      const color = getNodeColor(gNode);
      const isHovered = hoveredNode?.id === gNode.id;
      const isSelected = selectedNodeId === gNode.id;
      const isConnected = connectedNodeIds.has(gNode.id);

      ctx.beginPath();
      ctx.arc(gNode.x, gNode.y, size, 0, 2 * Math.PI);
      ctx.fillStyle = color;
      ctx.fill();

      if (isSelected) {
        ctx.strokeStyle = "#ffffff";
        ctx.lineWidth = 2.5;
        ctx.stroke();
      } else if (isHovered) {
        ctx.strokeStyle = "#ffffff";
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }

      const showLabel = globalScale > 2.5 || isHovered || isSelected || (selectedNodeId && isConnected);
      if (showLabel) {
        ctx.font = `${Math.max(10, 12) / globalScale}px Inter, sans-serif`;
        ctx.textAlign = "center";
        ctx.textBaseline = "top";
        ctx.fillStyle = isSelected || isConnected ? "#ffffff" : "#e5e5e5";
        ctx.fillText(gNode.name, gNode.x, gNode.y + size + 2);
      }
    },
    [getNodeColor, hoveredNode, maxConnections, selectedNodeId, connectedNodeIds]
  );

  return (
    <div className={`flex flex-col h-full ${fullscreen ? "fixed inset-0 z-50 bg-bg-primary" : ""}`}>
      {/* Header */}
      <header className="flex items-center justify-between px-6 py-4 border-b border-border shrink-0">
        <div>
          <h1 className="text-lg font-semibold text-text-primary">Graph</h1>
          <p className="text-sm text-text-secondary mt-0.5">
            {graphData ? `${forceGraphData.nodes.length} nodes, ${forceGraphData.links.length} edges` : "Explore code relationships"}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <select
            className="bg-bg-card border border-border rounded-md px-2.5 py-1.5 text-sm text-text-primary focus:outline-none focus:border-accent"
            value={selectedProject}
            onChange={(e) => setSelectedProject(e.target.value)}
          >
            {projects.map((p) => (
              <option key={p.slug} value={p.slug}>{p.slug}</option>
            ))}
          </select>
          <Button variant="ghost" size="sm" onClick={() => setViewMode(viewMode === "2d" ? "3d" : "2d")} title={viewMode === "2d" ? "Switch to 3D" : "Switch to 2D"}>
            {viewMode === "2d" ? <Box className="w-3.5 h-3.5" /> : <Square className="w-3.5 h-3.5" />}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => setShowFilters(!showFilters)}>
            <Filter className="w-3.5 h-3.5" />
          </Button>
          <Button variant="ghost" size="sm" onClick={() => setFullscreen(!fullscreen)}>
            {fullscreen ? <Minimize2 className="w-3.5 h-3.5" /> : <Maximize2 className="w-3.5 h-3.5" />}
          </Button>
        </div>
      </header>

      {/* Filters panel */}
      {showFilters && (
        <div className="px-6 py-3 border-b border-border flex items-center gap-4 shrink-0 bg-bg-sidebar/50 flex-wrap">
          {/* Search */}
          <div className="flex items-center gap-2 relative">
            <Search className="w-3.5 h-3.5 text-text-muted" />
            <input
              type="text"
              placeholder="Search symbol..."
              className="bg-bg-card border border-border rounded-md px-2.5 py-1 text-sm text-text-primary w-40 focus:outline-none focus:border-accent"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
            />
            {searchResults.length > 0 && (
              <div className="absolute top-full left-0 mt-1 bg-bg-card border border-border rounded-md shadow-xl z-30 w-64 max-h-48 overflow-y-auto">
                {searchResults.map((node) => (
                  <button
                    key={node.id}
                    className="w-full text-left px-3 py-1.5 hover:bg-bg-card-hover text-sm flex items-center gap-2"
                    onClick={() => handleSearchSelect(node)}
                  >
                    <span className="text-text-primary truncate">{node.name}</span>
                    <span className="text-[10px] text-text-muted ml-auto shrink-0">{node.kind}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
          <div className="flex items-center gap-2">
            <label className="text-xs text-text-secondary whitespace-nowrap">File</label>
            <input
              type="text"
              placeholder="Filter by path..."
              className="bg-bg-card border border-border rounded-md px-2.5 py-1 text-sm text-text-primary w-36 focus:outline-none focus:border-accent"
              value={fileFilter}
              onChange={(e) => setFileFilter(e.target.value)}
            />
          </div>
          <div className="flex items-center gap-2">
            <label className="text-xs text-text-secondary whitespace-nowrap">Lang</label>
            <select
              className="bg-bg-card border border-border rounded-md px-2.5 py-1 text-sm text-text-primary focus:outline-none focus:border-accent"
              value={languageFilter}
              onChange={(e) => setLanguageFilter(e.target.value)}
            >
              <option value="">All</option>
              {graphData?.languages.map((l) => (
                <option key={l} value={l}>{l}</option>
              ))}
            </select>
          </div>
          <div className="flex items-center gap-2">
            <label className="text-xs text-text-secondary whitespace-nowrap">Kind</label>
            <select
              className="bg-bg-card border border-border rounded-md px-2.5 py-1 text-sm text-text-primary focus:outline-none focus:border-accent"
              value={kindFilter}
              onChange={(e) => setKindFilter(e.target.value)}
            >
              <option value="">All</option>
              {graphData?.kinds.map((k) => (
                <option key={k} value={k}>{k}</option>
              ))}
            </select>
          </div>
          <div className="flex items-center gap-2">
            <label className="text-xs text-text-secondary whitespace-nowrap">Color</label>
            <select
              className="bg-bg-card border border-border rounded-md px-2.5 py-1 text-sm text-text-primary focus:outline-none focus:border-accent"
              value={heatmap}
              onChange={(e) => setHeatmap(e.target.value as HeatmapMode)}
            >
              <option value="connections">Connections (heatmap)</option>
              <option value="kind">Symbol type</option>
              <option value="file">File grouping</option>
            </select>
          </div>
          <div className="flex items-center gap-2">
            <label className="text-xs text-text-secondary whitespace-nowrap">Min conn.</label>
            <input
              type="range"
              min={0}
              max={Math.max(maxConnections, 1)}
              value={minConnections}
              onChange={(e) => setMinConnections(Number(e.target.value))}
              className="w-20 accent-accent"
            />
            <span className="text-xs text-text-muted w-6 text-right">{minConnections}</span>
          </div>
          <div className="flex items-center gap-2">
            <label className="text-xs text-text-secondary whitespace-nowrap">Nodes</label>
            <input
              type="range"
              min={100}
              max={10000}
              step={100}
              value={nodeLimit}
              onChange={(e) => setNodeLimit(Number(e.target.value))}
              className="w-20 accent-accent"
            />
            <span className="text-xs text-text-muted w-12 text-right">{nodeLimit}</span>
          </div>
        </div>
      )}

      {/* Graph canvas */}
      <div ref={containerRef} className="flex-1 relative min-h-0">
        {loading && (
          <div className="absolute inset-0 flex items-center justify-center bg-bg-primary/80 z-10">
            <Spinner size={32} />
          </div>
        )}
        {error && (
          <div className="absolute inset-0 flex items-center justify-center">
            <p className="text-sm text-text-muted">{error}</p>
          </div>
        )}
        {!loading && !error && graphData && forceGraphData.nodes.length > 0 && viewMode === "2d" && (
          <ForceGraph2D
            ref={graphRef as any}
            graphData={forceGraphData}
            nodeId="id"
            nodeCanvasObject={forceGraphData.nodes.length <= 2000 ? nodeCanvasObject : undefined}
            nodeColor={forceGraphData.nodes.length > 2000 ? ((node: any) => getNodeColor(node as GraphNode)) : undefined}
            nodeVal={forceGraphData.nodes.length > 2000 ? ((node: any) => Math.max(1, Math.log2((node as GraphNode).connections + 1))) : undefined}
            nodePointerAreaPaint={(node: any, color, ctx) => {
              const size = Math.max(3, Math.min(12, 3 + ((node as GraphNode).connections / Math.max(maxConnections, 1)) * 9));
              ctx.beginPath();
              ctx.arc(node.x!, node.y!, size + 2, 0, 2 * Math.PI);
              ctx.fillStyle = color;
              ctx.fill();
            }}
            linkSource="source"
            linkTarget="target"
            linkColor={getLinkColor}
            linkWidth={getLinkWidth}
            linkDirectionalArrowLength={3}
            linkDirectionalArrowRelPos={1}
            backgroundColor="#1a1a1a"
            onNodeHover={(node) => setHoveredNode(node as GraphNode | null)}
            onNodeClick={handleNodeClick}
            onBackgroundClick={handleBackgroundClick}
            warmupTicks={forceGraphData.nodes.length > 2000 ? 100 : 50}
            cooldownTicks={forceGraphData.nodes.length > 2000 ? 0 : 100}
            d3AlphaDecay={forceGraphData.nodes.length > 2000 ? 0.03 : 0.02}
            d3VelocityDecay={forceGraphData.nodes.length > 2000 ? 0.4 : 0.3}
            enablePointerInteraction={forceGraphData.nodes.length <= 3000}
            nodeRelSize={4}
            width={containerRef.current?.clientWidth ?? 800}
            height={containerRef.current?.clientHeight ?? 500}
          />
        )}
        {!loading && !error && graphData && forceGraphData.nodes.length > 0 && viewMode === "3d" && (
          <ForceGraph3D
            graphData={forceGraphData}
            nodeId="id"
            nodeLabel={(node: any) => `${(node as GraphNode).name} (${(node as GraphNode).kind})`}
            nodeColor={(node: any) => getNodeColor(node as GraphNode)}
            nodeVal={(node: any) => Math.max(1, Math.log2((node as GraphNode).connections + 1) * 2)}
            nodeResolution={16}
            linkSource="source"
            linkTarget="target"
            linkColor={getLinkColor}
            linkWidth={(link: any) => getLinkWidth(link)}
            linkDirectionalArrowLength={3}
            linkDirectionalArrowRelPos={1}
            backgroundColor="#1a1a1a"
            onNodeHover={(node) => setHoveredNode(node as GraphNode | null)}
            onNodeClick={handleNodeClick}
            onBackgroundClick={handleBackgroundClick}
            cooldownTicks={100}
            d3AlphaDecay={0.02}
            d3VelocityDecay={0.3}
            width={containerRef.current?.clientWidth ?? 800}
            height={containerRef.current?.clientHeight ?? 500}
          />
        )}
        {!loading && !error && graphData && forceGraphData.nodes.length === 0 && (
          <div className="absolute inset-0 flex items-center justify-center">
            <p className="text-sm text-text-muted">
              {minConnections > 0 ? `No symbols with ${minConnections}+ connections. Lower the filter.` : "No symbols found. Build the project first."}
            </p>
          </div>
        )}
      </div>

      {/* Hover tooltip */}
      {hoveredNode && (
        <div className="absolute bottom-4 left-4 bg-bg-card border border-border rounded-lg px-4 py-3 shadow-xl max-w-xs z-20">
          <p className="text-sm font-medium text-text-primary">{hoveredNode.name}</p>
          <p className="text-xs text-text-secondary mt-1">
            <span className="text-accent">{hoveredNode.kind}</span> &middot; {hoveredNode.file}:{hoveredNode.line_start}
          </p>
          <p className="text-xs text-text-muted mt-0.5">{hoveredNode.connections} connections</p>
        </div>
      )}

      {/* Selected node info */}
      {selectedNodeId && graphData && (
        <div className="absolute top-20 left-4 bg-bg-card border border-accent/30 rounded-lg px-4 py-3 shadow-xl max-w-xs z-20">
          <p className="text-[10px] text-accent uppercase tracking-wider mb-1">Selected</p>
          {(() => {
            const node = graphData.nodes.find((n) => n.id === selectedNodeId);
            if (!node) return null;
            return (
              <>
                <p className="text-sm font-medium text-text-primary">{node.name}</p>
                <p className="text-xs text-text-secondary mt-0.5">{node.kind} &middot; {connectedNodeIds.size - 1} connected</p>
              </>
            );
          })()}
          <button
            className="text-[11px] text-text-muted hover:text-text-primary mt-1.5"
            onClick={() => setSelectedNodeId(null)}
          >
            Clear selection
          </button>
        </div>
      )}

      {/* Legend */}
      {heatmap === "kind" && !selectedNodeId && (
        <div className="absolute bottom-4 right-4 bg-bg-card border border-border rounded-lg px-3 py-2 shadow-xl z-20">
          <p className="text-[10px] text-text-muted uppercase tracking-wider mb-1.5">Legend</p>
          <div className="grid grid-cols-2 gap-x-4 gap-y-1">
            {Object.entries(KIND_COLORS).slice(0, 10).map(([kind, color]) => (
              <div key={kind} className="flex items-center gap-1.5">
                <span className="w-2 h-2 rounded-full shrink-0" style={{ backgroundColor: color }} />
                <span className="text-[11px] text-text-secondary">{kind}</span>
              </div>
            ))}
          </div>
        </div>
      )}
      {heatmap === "connections" && !selectedNodeId && (
        <div className="absolute bottom-4 right-4 bg-bg-card border border-border rounded-lg px-3 py-2 shadow-xl z-20">
          <p className="text-[10px] text-text-muted uppercase tracking-wider mb-1.5">Heatmap</p>
          <div className="flex items-center gap-2">
            <span className="text-[11px] text-text-secondary">Low</span>
            <div className="w-20 h-2 rounded-full" style={{
              background: "linear-gradient(to right, rgb(99, 102, 241), rgb(239, 68, 68))"
            }} />
            <span className="text-[11px] text-text-secondary">High</span>
          </div>
        </div>
      )}
    </div>
  );
}
