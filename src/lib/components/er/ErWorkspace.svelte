<script lang="ts">
  /**
   * ER canvas — DBeaver-like column links on a grid.
   * - Normal/Broken arrow tools: click col A → optional path → col B
   * - Right-click table → Delete
   * - Click a relationship path → drag bend handles (grid-snapped)
   * - Mid-segment diamonds insert new bends
   * - Grid show/hide toggle
   */
  import type {
    DataFamily,
    ErColumn,
    ErDiagramState,
    ErEdge,
    ErEdgeKind,
    ErNode,
    ErPoint,
  } from "$lib/types/er";
  import {
    ER_GRID,
    FAMILY_META,
    SQL_TYPES,
    emptyDiagram,
    snapToGrid,
  } from "$lib/types/er";
  import { Button, Badge, Inline, Input, Select } from "$lib/components/ui";
  import { cn } from "$lib/utils/cn";
  import { listDiagrams, saveDiagram, type ErDiagramRecord } from "$lib/api/diagrams";
  import { isTauri } from "$lib/api/tauri";

  interface Props {
    family: DataFamily;
    onback: () => void;
    onchangeFamily: () => void;
  }

  let { family, onback, onchangeFamily }: Props = $props();

  const TABLE_W = 248;
  const HEADER_H = 40;
  const ROW_H = 30;

  let diagram = $state<ErDiagramState | null>(null);
  let diagramId = $state<string | null>(null);
  let diagramName = $state("Untitled diagram");
  let selectedTableId = $state<string | null>(null);
  let selectedEdgeId = $state<string | null>(null);
  let toolMsg = $state<string | null>(null);
  let saving = $state(false);
  let dirty = $state(false);
  let showGrid = $state(true);

  /** null = move tables · normal | broken = column linking */
  let arrowTool = $state<"normal" | "broken" | null>(null);
  let linkFrom = $state<{ tableId: string; columnId: string } | null>(null);
  let draftWaypoints = $state<ErPoint[]>([]);
  let cursorPt = $state<ErPoint | null>(null);

  // table drag
  let dragTableId = $state<string | null>(null);
  let dragOffX = 0;
  let dragOffY = 0;
  let didDrag = false;
  let canvasEl: HTMLDivElement | null = $state(null);

  // edge waypoint drag: { edgeId, waypointIndex }
  let dragWaypoint = $state<{ edgeId: string; index: number } | null>(null);

  // right-click context menu on a table
  let ctxMenu = $state<{ clientX: number; clientY: number; tableId: string } | null>(null);

  let showAddCol = $state(false);
  let newColName = $state("");
  let newColType = $state("TEXT");
  let newColPk = $state(false);

  const meta = $derived(FAMILY_META[family]);
  const isSql = $derived(family === "sql");

  $effect(() => {
    diagram = emptyDiagram(family);
    diagramId = null;
    diagramName = `Untitled ${FAMILY_META[family].short} diagram`;
    selectedTableId = null;
    selectedEdgeId = null;
    arrowTool = null;
    linkFrom = null;
    draftWaypoints = [];
    cursorPt = null;
    showAddCol = false;
    dirty = false;
    ctxMenu = null;
    dragWaypoint = null;
    toolMsg =
      "Add a table, or pick Normal/Broken → click source column → (optional path) → target column. Right-click a table to delete. Click a link to reshape it.";
  });

  function uid(p: string) {
    return `${p}_${Math.random().toString(36).slice(2, 9)}`;
  }

  function markDirty() {
    dirty = true;
  }

  function canvasPoint(e: { clientX: number; clientY: number }): ErPoint | null {
    if (!canvasEl) return null;
    const rect = canvasEl.getBoundingClientRect();
    return {
      x: e.clientX - rect.left + canvasEl.scrollLeft,
      y: e.clientY - rect.top + canvasEl.scrollTop,
    };
  }

  function setArrowTool(kind: "normal" | "broken" | null) {
    arrowTool = kind;
    linkFrom = null;
    draftWaypoints = [];
    cursorPt = null;
    selectedEdgeId = null;
    ctxMenu = null;
    dragWaypoint = null;
    if (kind === "normal") {
      toolMsg =
        "1) Click a column on table A  ·  2) Optional: click canvas to bend the path  ·  3) Click a column on table B";
    } else if (kind === "broken") {
      toolMsg =
        "Broken (dashed) link: click source column → optional path points → target column";
    } else {
      toolMsg =
        "Move mode: drag tables. Click a link to reshape its path. Right-click a table to delete.";
    }
  }

  function cancelLink() {
    linkFrom = null;
    draftWaypoints = [];
    cursorPt = null;
    toolMsg = arrowTool
      ? "Link cancelled. Click a source column to start again."
      : "Move mode.";
  }

  function columnCenter(node: ErNode, columnId: string): ErPoint {
    const idx = Math.max(0, node.columns.findIndex((c) => c.id === columnId));
    return {
      x: node.x + TABLE_W / 2,
      y: node.y + HEADER_H + idx * ROW_H + ROW_H / 2,
    };
  }

  /** Full path points: source column → waypoints → target column */
  function edgePoints(edge: ErEdge): ErPoint[] {
    if (!diagram) return [];
    const a = diagram.nodes.find((n) => n.id === edge.from);
    const b = diagram.nodes.find((n) => n.id === edge.to);
    if (!a || !b) return [];
    return [
      columnCenter(a, edge.fromColumnId),
      ...(edge.waypoints ?? []),
      columnCenter(b, edge.toColumnId),
    ];
  }

  function edgePolyline(edge: ErEdge): string {
    const pts = edgePoints(edge);
    if (!pts.length) return "";
    return pts.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x} ${p.y}`).join(" ");
  }

  function previewPolyline(): string {
    const from = linkFrom;
    if (!diagram || !from || !cursorPt) return "";
    const a = diagram.nodes.find((n) => n.id === from.tableId);
    if (!a) return "";
    const p0 = columnCenter(a, from.columnId);
    const pts = [p0, ...draftWaypoints, cursorPt];
    return pts.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x} ${p.y}`).join(" ");
  }

  /** Midpoint of each segment — drag to insert a new waypoint */
  function segmentMids(edge: ErEdge): { x: number; y: number; insertAt: number }[] {
    const pts = edgePoints(edge);
    const mids: { x: number; y: number; insertAt: number }[] = [];
    for (let i = 0; i < pts.length - 1; i++) {
      mids.push({
        x: (pts[i].x + pts[i + 1].x) / 2,
        y: (pts[i].y + pts[i + 1].y) / 2,
        insertAt: i, // waypoint index to insert at (0 = first waypoint)
      });
    }
    return mids;
  }

  function isBroken(kind: ErEdgeKind) {
    return kind === "broken" || kind === "inferred" || kind === "logical";
  }

  function addTable() {
    if (!diagram) return;
    if (arrowTool) {
      linkFrom = null;
      draftWaypoints = [];
      cursorPt = null;
    }
    ctxMenu = null;
    const n = diagram.nodes.length;
    const node: ErNode = {
      id: uid("n"),
      kind: isSql ? "table" : family === "nosql" ? "collection" : "key_pattern",
      name: isSql
        ? `table_${n + 1}`
        : family === "nosql"
          ? `collection_${n + 1}`
          : `key:pattern_${n + 1}`,
      columns: [
        {
          id: uid("c"),
          name: isSql ? "id" : family === "nosql" ? "_id" : "type",
          dataType: isSql ? "UUID" : family === "nosql" ? "ObjectId" : "string",
          pk: isSql || family === "nosql",
        },
      ],
      x: snapToGrid(ER_GRID * 4 + (n % 4) * ER_GRID * 2),
      y: snapToGrid(ER_GRID * 4 + (n % 4) * ER_GRID * 2),
    };
    diagram = { ...diagram, nodes: [...diagram.nodes, node] };
    selectedTableId = node.id;
    selectedEdgeId = null;
    showAddCol = false;
    toolMsg = `Added “${node.name}”. Drag on the grid, or use an arrow tool to link columns. Right-click to delete.`;
    markDirty();
  }

  function onColumnClick(tableId: string, columnId: string, e: MouseEvent) {
    e.stopPropagation();
    e.preventDefault();
    ctxMenu = null;

    if (!arrowTool) {
      selectedTableId = tableId;
      selectedEdgeId = null;
      showAddCol = false;
      return;
    }

    if (!linkFrom) {
      linkFrom = { tableId, columnId };
      draftWaypoints = [];
      selectedTableId = tableId;
      selectedEdgeId = null;
      const node = diagram?.nodes.find((n) => n.id === tableId);
      const col = node?.columns.find((c) => c.id === columnId);
      toolMsg = `From ${node?.name ?? "?"}.${col?.name ?? "?"}. Click empty grid to bend the path, or click a column on another table to finish.`;
      return;
    }

    if (linkFrom.tableId === tableId) {
      linkFrom = { tableId, columnId };
      draftWaypoints = [];
      toolMsg = "Source column updated. Now click a column on another table.";
      return;
    }

    completeLink(tableId, columnId);
  }

  function completeLink(toTableId: string, toColumnId: string) {
    if (!diagram || !linkFrom || !arrowTool) return;

    const exists = diagram.edges.some(
      (ed) =>
        ed.from === linkFrom!.tableId &&
        ed.fromColumnId === linkFrom!.columnId &&
        ed.to === toTableId &&
        ed.toColumnId === toColumnId,
    );
    if (exists) {
      toolMsg = "That column link already exists.";
      linkFrom = null;
      draftWaypoints = [];
      cursorPt = null;
      return;
    }

    const fromNode = diagram.nodes.find((n) => n.id === linkFrom!.tableId);
    const toNode = diagram.nodes.find((n) => n.id === toTableId);
    const fromCol = fromNode?.columns.find((c) => c.id === linkFrom!.columnId);
    const toCol = toNode?.columns.find((c) => c.id === toColumnId);
    const label = `${fromNode?.name}.${fromCol?.name} → ${toNode?.name}.${toCol?.name}`;
    const kind: ErEdgeKind = arrowTool === "broken" ? "broken" : "normal";
    // Snap any draft bends to grid (already snapped on place, belt-and-suspenders)
    const wps = draftWaypoints.map((p) => ({
      x: snapToGrid(p.x),
      y: snapToGrid(p.y),
    }));

    const edge: ErEdge = {
      id: uid("e"),
      from: linkFrom.tableId,
      to: toTableId,
      fromColumnId: linkFrom.columnId,
      toColumnId: toColumnId,
      kind,
      label,
      waypoints: wps,
    };

    diagram = { ...diagram, edges: [...diagram.edges, edge] };
    toolMsg = `Linked: ${label}. Click the path to select and drag handles to reshape.`;
    linkFrom = null;
    draftWaypoints = [];
    cursorPt = null;
    selectedEdgeId = edge.id;
    selectedTableId = null;
    markDirty();
  }

  function onCanvasClick(e: MouseEvent) {
    const t = e.target as HTMLElement;
    if (
      t.closest("[data-er-table]") ||
      t.closest("[data-er-column]") ||
      t.closest("[data-er-edge-handle]") ||
      t.closest("[data-er-edge-hit]") ||
      t.closest("[data-ctx-menu]")
    ) {
      return;
    }

    ctxMenu = null;

    // While linking: place a grid-snapped bend point
    if (arrowTool && linkFrom) {
      const pt = canvasPoint(e);
      if (!pt) return;
      const snapped = { x: snapToGrid(pt.x), y: snapToGrid(pt.y) };
      draftWaypoints = [...draftWaypoints, snapped];
      toolMsg = `Bend point ${draftWaypoints.length} added. Click more points or click the target column.`;
      return;
    }

    // With a selected edge: empty-canvas click can add a new waypoint at the end of path
    // (more common: use mid handles). Deselect instead for clarity.
    selectedTableId = null;
    selectedEdgeId = null;
    showAddCol = false;
  }

  function onCanvasPointerMove(e: PointerEvent) {
    if (arrowTool && linkFrom) {
      cursorPt = canvasPoint(e);
    }

    // Dragging an edge waypoint
    if (dragWaypoint && diagram) {
      const pt = canvasPoint(e);
      if (!pt) return;
      const snapped = { x: snapToGrid(pt.x), y: snapToGrid(pt.y) };
      const { edgeId, index } = dragWaypoint;
      diagram = {
        ...diagram,
        edges: diagram.edges.map((ed) => {
          if (ed.id !== edgeId) return ed;
          const wps = [...(ed.waypoints ?? [])];
          if (index < 0 || index >= wps.length) return ed;
          wps[index] = snapped;
          return { ...ed, waypoints: wps };
        }),
      };
      return;
    }

    // Dragging a table
    if (!dragTableId || !diagram || !canvasEl) return;
    const rect = canvasEl.getBoundingClientRect();
    const x = snapToGrid(
      Math.max(0, e.clientX - rect.left + canvasEl.scrollLeft - dragOffX),
    );
    const y = snapToGrid(
      Math.max(0, e.clientY - rect.top + canvasEl.scrollTop - dragOffY),
    );
    const cur = diagram.nodes.find((n) => n.id === dragTableId);
    if (cur && (cur.x !== x || cur.y !== y)) didDrag = true;
    diagram = {
      ...diagram,
      nodes: diagram.nodes.map((n) => (n.id === dragTableId ? { ...n, x, y } : n)),
    };
  }

  function onWindowPointerUp() {
    if (dragWaypoint) {
      markDirty();
      dragWaypoint = null;
    }
  }

  function onTablePointerDown(tableId: string, e: PointerEvent) {
    if (arrowTool) return;
    if ((e.target as HTMLElement).closest("[data-no-drag]")) return;
    if (!diagram || !canvasEl) return;
    const node = diagram.nodes.find((n) => n.id === tableId);
    if (!node) return;
    const rect = canvasEl.getBoundingClientRect();
    dragTableId = tableId;
    didDrag = false;
    dragOffX = e.clientX - rect.left + canvasEl.scrollLeft - node.x;
    dragOffY = e.clientY - rect.top + canvasEl.scrollTop - node.y;
    selectedTableId = tableId;
    selectedEdgeId = null;
    ctxMenu = null;
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  }

  function onTablePointerUp(e: PointerEvent) {
    if (dragTableId) {
      if (didDrag) markDirty();
      try {
        (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);
      } catch {
        /* ignore */
      }
    }
    dragTableId = null;
  }

  function onTableContextMenu(tableId: string, e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    selectedTableId = tableId;
    selectedEdgeId = null;
    ctxMenu = { clientX: e.clientX, clientY: e.clientY, tableId };
  }

  function deleteTableById(id: string) {
    if (!diagram) return;
    diagram = {
      ...diagram,
      nodes: diagram.nodes.filter((n) => n.id !== id),
      edges: diagram.edges.filter((e) => e.from !== id && e.to !== id),
    };
    if (selectedTableId === id) selectedTableId = null;
    showAddCol = false;
    ctxMenu = null;
    toolMsg = "Table deleted.";
    markDirty();
  }

  function deleteSelectedTable() {
    if (selectedTableId) deleteTableById(selectedTableId);
  }

  function renameTable(name: string) {
    if (!diagram || !selectedTableId) return;
    diagram = {
      ...diagram,
      nodes: diagram.nodes.map((n) =>
        n.id === selectedTableId ? { ...n, name: name || n.name } : n,
      ),
    };
    markDirty();
  }

  function addColumn() {
    if (!diagram || !selectedTableId) return;
    const name = newColName.trim();
    if (!name) {
      toolMsg = "Column name is required.";
      return;
    }
    const col: ErColumn = {
      id: uid("c"),
      name,
      dataType: newColType || "TEXT",
      pk: newColPk,
    };
    diagram = {
      ...diagram,
      nodes: diagram.nodes.map((n) => {
        if (n.id !== selectedTableId) return n;
        let columns = [...n.columns, col];
        if (col.pk) {
          columns = columns.map((c) =>
            c.id === col.id ? c : { ...c, pk: false },
          );
        }
        return { ...n, columns };
      }),
    };
    newColName = "";
    newColType = "TEXT";
    newColPk = false;
    showAddCol = false;
    markDirty();
  }

  function deleteColumn(colId: string) {
    if (!diagram || !selectedTableId) return;
    diagram = {
      ...diagram,
      nodes: diagram.nodes.map((n) =>
        n.id === selectedTableId
          ? { ...n, columns: n.columns.filter((c) => c.id !== colId) }
          : n,
      ),
      edges: diagram.edges.filter(
        (e) => e.fromColumnId !== colId && e.toColumnId !== colId,
      ),
    };
    markDirty();
  }

  function deleteEdge(edgeId: string) {
    if (!diagram) return;
    diagram = {
      ...diagram,
      edges: diagram.edges.filter((e) => e.id !== edgeId),
    };
    if (selectedEdgeId === edgeId) selectedEdgeId = null;
    toolMsg = "Link deleted.";
    markDirty();
  }

  function selectEdge(edgeId: string, e?: Event) {
    e?.stopPropagation();
    e?.preventDefault();
    selectedEdgeId = edgeId;
    selectedTableId = null;
    showAddCol = false;
    ctxMenu = null;
    if (!arrowTool) {
      toolMsg =
        "Link selected. Drag blue handles to move path points on the grid. Drag diamonds on segments to add bends. Double-click a handle to remove it.";
    }
  }

  /** Start dragging an existing waypoint */
  function onWaypointDown(edgeId: string, index: number, e: PointerEvent) {
    e.stopPropagation();
    e.preventDefault();
    selectedEdgeId = edgeId;
    selectedTableId = null;
    dragWaypoint = { edgeId, index };
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  }

  function onWaypointUp(e: PointerEvent) {
    if (dragWaypoint) {
      markDirty();
      dragWaypoint = null;
    }
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);
    } catch {
      /* ignore */
    }
  }

  /** Drag a mid-segment handle: insert waypoint if needed, then drag it */
  function onMidHandleDown(edgeId: string, insertAt: number, e: PointerEvent) {
    e.stopPropagation();
    e.preventDefault();
    if (!diagram) return;
    const edge = diagram.edges.find((ed) => ed.id === edgeId);
    if (!edge) return;
    const pt = canvasPoint(e);
    if (!pt) return;
    const snapped = { x: snapToGrid(pt.x), y: snapToGrid(pt.y) };
    const wps = [...(edge.waypoints ?? [])];
    wps.splice(insertAt, 0, snapped);
    diagram = {
      ...diagram,
      edges: diagram.edges.map((ed) =>
        ed.id === edgeId ? { ...ed, waypoints: wps } : ed,
      ),
    };
    selectedEdgeId = edgeId;
    selectedTableId = null;
    dragWaypoint = { edgeId, index: insertAt };
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  }

  function removeWaypoint(edgeId: string, index: number, e: Event) {
    e.stopPropagation();
    e.preventDefault();
    if (!diagram) return;
    diagram = {
      ...diagram,
      edges: diagram.edges.map((ed) => {
        if (ed.id !== edgeId) return ed;
        const wps = [...(ed.waypoints ?? [])];
        wps.splice(index, 1);
        return { ...ed, waypoints: wps };
      }),
    };
    toolMsg = "Bend point removed.";
    markDirty();
  }

  function straightenSelectedEdge() {
    if (!diagram || !selectedEdgeId) return;
    diagram = {
      ...diagram,
      edges: diagram.edges.map((ed) =>
        ed.id === selectedEdgeId ? { ...ed, waypoints: [] } : ed,
      ),
    };
    toolMsg = "Edge path reset to straight.";
    markDirty();
  }

  function toggleGrid() {
    showGrid = !showGrid;
    toolMsg = showGrid ? "Grid shown." : "Grid hidden.";
  }

  async function handleSave() {
    if (!diagram) return;
    saving = true;
    try {
      if (!isTauri()) {
        const key = "mystique-er-diagrams";
        const raw = localStorage.getItem(key);
        const list: ErDiagramRecord[] = raw ? JSON.parse(raw) : [];
        const now = new Date().toISOString();
        const id = diagramId ?? uid("d");
        const rec: ErDiagramRecord = {
          id,
          name: diagramName.trim() || "Untitled",
          family,
          body: diagram,
          createdAt: list.find((r) => r.id === id)?.createdAt ?? now,
          updatedAt: now,
        };
        localStorage.setItem(
          key,
          JSON.stringify([rec, ...list.filter((r) => r.id !== id)]),
        );
        diagramId = id;
        dirty = false;
        toolMsg = "Saved locally (browser).";
        return;
      }
      const saved = await saveDiagram({
        id: diagramId,
        name: diagramName.trim() || "Untitled",
        family,
        body: diagram,
      });
      diagramId = saved.id;
      dirty = false;
      toolMsg = `Saved “${saved.name}”.`;
    } catch (e) {
      toolMsg = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }

  async function handleLoadLatest() {
    try {
      let list: ErDiagramRecord[] = [];
      if (isTauri()) {
        list = await listDiagrams(family);
      } else {
        const raw = localStorage.getItem("mystique-er-diagrams");
        list = (raw ? JSON.parse(raw) : []).filter(
          (r: ErDiagramRecord) => r.family === family,
        );
      }
      if (!list.length) {
        toolMsg = "No saved diagrams yet.";
        return;
      }
      const rec = list[0];
      diagram = rec.body;
      diagramId = rec.id;
      diagramName = rec.name;
      dirty = false;
      toolMsg = `Loaded “${rec.name}”.`;
    } catch (e) {
      toolMsg = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<svelte:window onpointerup={onWindowPointerUp} onpointercancel={onWindowPointerUp} />

<div class="flex h-full min-h-0 flex-col">
  <div
    class="flex h-topbar shrink-0 items-center justify-between gap-3 border-b border-line bg-panel px-4"
  >
    <div class="flex min-w-0 items-center gap-2">
      <Button variant="ghost" size="sm" onclick={onback}>← Back</Button>
      <span class="text-fg-muted">/</span>
      <span class="text-body font-semibold text-fg">ER · {meta.title}</span>
      {#if dirty}
        <Badge tone="warning" size="sm">Unsaved</Badge>
      {/if}
      {#if arrowTool}
        <Badge tone="accent" size="sm">
          {arrowTool === "broken" ? "Broken arrow" : "Normal arrow"}
          {#if linkFrom}
            · pick target
          {/if}
        </Badge>
      {/if}
    </div>
    <Inline gap="sm">
      <Input bind:value={diagramName} class="w-40" inputSize="sm" aria-label="Diagram name" />
      <Button variant="secondary" size="sm" onclick={handleLoadLatest}>Load last</Button>
      <Button variant="primary" size="sm" onclick={handleSave} disabled={saving}>
        {saving ? "Saving…" : "Save"}
      </Button>
      <Button variant="ghost" size="sm" onclick={onchangeFamily}>Change family</Button>
    </Inline>
  </div>

  <div class="flex min-h-0 flex-1">
    <aside class="flex w-56 shrink-0 flex-col gap-2 border-r border-line bg-panel p-3">
      <p class="text-label font-semibold tracking-wider text-fg-muted uppercase">Tools</p>
      <Button variant="primary" fullWidth onclick={addTable}>
        + Add {meta.nodeLabel.toLowerCase()}
      </Button>

      <button
        type="button"
        class={cn(
          "flex w-full items-center justify-between rounded-lg border px-3 py-2 text-left text-caption font-medium transition-colors",
          showGrid
            ? "border-accent bg-accent-muted text-fg"
            : "border-line bg-input text-fg-secondary hover:border-line-strong",
        )}
        onclick={toggleGrid}
        aria-pressed={showGrid}
      >
        <span>Grid</span>
        <span class="font-mono text-micro uppercase tracking-wide">
          {showGrid ? "ON" : "OFF"}
        </span>
      </button>

      <p class="mt-2 text-label font-semibold tracking-wider text-fg-muted uppercase">
        Arrows
      </p>
      <div class="grid grid-cols-2 gap-2">
        <button
          type="button"
          class={cn(
            "flex flex-col items-center gap-1 rounded-lg border px-2 py-3 transition-colors",
            arrowTool === "normal"
              ? "border-accent bg-accent-muted"
              : "border-line bg-input hover:border-line-strong",
          )}
          onclick={() => setArrowTool(arrowTool === "normal" ? null : "normal")}
        >
          <svg width="36" height="14" class="text-fg" aria-hidden="true">
            <defs>
              <marker id="tn" markerWidth="6" markerHeight="6" refX="5" refY="3" orient="auto">
                <path d="M0,0 L6,3 L0,6 Z" fill="currentColor" />
              </marker>
            </defs>
            <line
              x1="2"
              y1="7"
              x2="28"
              y2="7"
              stroke="currentColor"
              stroke-width="2"
              marker-end="url(#tn)"
            />
          </svg>
          <span class="text-caption font-medium text-fg">Normal</span>
        </button>
        <button
          type="button"
          class={cn(
            "flex flex-col items-center gap-1 rounded-lg border px-2 py-3 transition-colors",
            arrowTool === "broken"
              ? "border-accent bg-accent-muted"
              : "border-line bg-input hover:border-line-strong",
          )}
          onclick={() => setArrowTool(arrowTool === "broken" ? null : "broken")}
        >
          <svg width="36" height="14" class="text-fg" aria-hidden="true">
            <defs>
              <marker id="tb" markerWidth="6" markerHeight="6" refX="5" refY="3" orient="auto">
                <path d="M0,0 L6,3 L0,6 Z" fill="currentColor" />
              </marker>
            </defs>
            <line
              x1="2"
              y1="7"
              x2="28"
              y2="7"
              stroke="currentColor"
              stroke-width="2"
              stroke-dasharray="4 3"
              marker-end="url(#tb)"
            />
          </svg>
          <span class="text-caption font-medium text-fg">Broken</span>
        </button>
      </div>

      <ol class="m-0 list-decimal space-y-1 pl-4 text-caption text-fg-muted">
        <li>Select Normal or Broken</li>
        <li>Click <strong class="text-fg-secondary">column</strong> on table A</li>
        <li>Optional: click grid to draw path bends</li>
        <li>Click <strong class="text-fg-secondary">column</strong> on table B</li>
        <li>Click the path later to drag bends on the grid</li>
      </ol>

      {#if linkFrom}
        <Button variant="secondary" fullWidth onclick={cancelLink}>Cancel link</Button>
      {/if}
      {#if selectedTableId && !arrowTool}
        <Button variant="danger-ghost" fullWidth onclick={deleteSelectedTable}>
          Delete selected table
        </Button>
      {/if}
      {#if selectedEdgeId}
        <Button variant="ghost" fullWidth onclick={straightenSelectedEdge}>
          Straighten selected link
        </Button>
        <Button variant="danger-ghost" fullWidth onclick={() => deleteEdge(selectedEdgeId!)}>
          Delete selected link
        </Button>
      {/if}

      {#if toolMsg}
        <p
          class="rounded-md border border-line bg-input px-2 py-1.5 text-caption text-fg-secondary"
          role="status"
        >
          {toolMsg}
        </p>
      {/if}

      {#if diagram && diagram.edges.length > 0}
        <div class="border-t border-line pt-2">
          <p class="mb-1 text-label font-semibold text-fg-muted uppercase">Links</p>
          <ul class="m-0 max-h-36 list-none space-y-1 overflow-auto p-0">
            {#each diagram.edges as edge (edge.id)}
              <li>
                <button
                  type="button"
                  class={cn(
                    "flex w-full items-start justify-between gap-1 rounded-md border px-2 py-1 text-left text-caption",
                    selectedEdgeId === edge.id
                      ? "border-accent bg-accent-muted"
                      : "border-line bg-input",
                  )}
                  onclick={() => selectEdge(edge.id)}
                >
                  <span class="min-w-0 truncate text-fg-secondary">
                    {isBroken(edge.kind) ? "┄" : "─"}
                    {edge.label}
                  </span>
                  <span
                    class="shrink-0 text-danger"
                    role="button"
                    tabindex="0"
                    onclick={(e) => {
                      e.stopPropagation();
                      deleteEdge(edge.id);
                    }}
                  >
                    ×
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
    </aside>

    <!-- Canvas -->
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div
      class="relative min-w-0 flex-1 overflow-auto"
      style="
        background-color: var(--bg-app);
        background-image: {showGrid
        ? `linear-gradient(to right, color-mix(in srgb, var(--border-subtle) 70%, transparent) 1px, transparent 1px),
          linear-gradient(to bottom, color-mix(in srgb, var(--border-subtle) 70%, transparent) 1px, transparent 1px)`
        : "none"};
        background-size: {ER_GRID}px {ER_GRID}px;
        cursor: {arrowTool ? "crosshair" : "default"};
      "
      bind:this={canvasEl}
      role="application"
      aria-label="ER grid canvas"
      onclick={onCanvasClick}
      onpointermove={onCanvasPointerMove}
      oncontextmenu={(e) => {
        // prevent browser menu on empty canvas
        if (!(e.target as HTMLElement).closest("[data-er-table]")) {
          e.preventDefault();
          ctxMenu = null;
        }
      }}
    >
      {#if diagram}
        <!-- Edges SVG: pointer events on hit paths + handles only -->
        <svg
          class="absolute inset-0 h-full min-h-[800px] w-full min-w-[1400px]"
          style="pointer-events: none;"
          aria-hidden="true"
        >
          <defs>
            <marker
              id="er-arrow-head"
              markerWidth="10"
              markerHeight="10"
              refX="9"
              refY="3"
              orient="auto"
            >
              <path d="M0,0 L9,3 L0,6 Z" fill="var(--accent)" />
            </marker>
            <marker
              id="er-arrow-head-sel"
              markerWidth="10"
              markerHeight="10"
              refX="9"
              refY="3"
              orient="auto"
            >
              <path d="M0,0 L9,3 L0,6 Z" fill="var(--accent)" />
            </marker>
          </defs>

          {#each diagram.edges as edge (edge.id)}
            {@const selected = selectedEdgeId === edge.id}
            <!-- Wide invisible hit stroke for click-to-select -->
            <path
              data-er-edge-hit
              d={edgePolyline(edge)}
              fill="none"
              stroke="transparent"
              stroke-width="14"
              style="pointer-events: stroke; cursor: pointer;"
              onclick={(e) => selectEdge(edge.id, e)}
            />
            <!-- Visible path -->
            <path
              d={edgePolyline(edge)}
              fill="none"
              stroke="var(--accent)"
              stroke-width={selected ? 3 : 2}
              stroke-dasharray={isBroken(edge.kind) ? "7 5" : "0"}
              marker-end="url(#er-arrow-head)"
              opacity="0.95"
              style="pointer-events: none;"
            />

            {#if selected && !arrowTool}
              <!-- Mid-segment insert handles (diamonds) -->
              {#each segmentMids(edge) as mid (mid.insertAt + "-" + mid.x + "-" + mid.y)}
                <g
                  data-er-edge-handle
                  style="pointer-events: all; cursor: move;"
                  onpointerdown={(e) => onMidHandleDown(edge.id, mid.insertAt, e)}
                  onpointerup={onWaypointUp}
                  onpointercancel={onWaypointUp}
                >
                  <rect
                    x={mid.x - 5}
                    y={mid.y - 5}
                    width="10"
                    height="10"
                    transform="rotate(45 {mid.x} {mid.y})"
                    fill="var(--bg-panel, #1a1a1a)"
                    stroke="var(--accent)"
                    stroke-width="1.5"
                  />
                </g>
              {/each}

              <!-- Existing waypoint handles (circles) -->
              {#each edge.waypoints ?? [] as wp, wi (edge.id + "-wp-" + wi)}
                <g
                  data-er-edge-handle
                  style="pointer-events: all; cursor: grab;"
                  onpointerdown={(e) => onWaypointDown(edge.id, wi, e)}
                  onpointerup={onWaypointUp}
                  onpointercancel={onWaypointUp}
                  ondblclick={(e) => removeWaypoint(edge.id, wi, e)}
                >
                  <circle
                    cx={wp.x}
                    cy={wp.y}
                    r="7"
                    fill="var(--accent)"
                    stroke="var(--bg-panel, #fff)"
                    stroke-width="2"
                  />
                </g>
              {/each}
            {/if}
          {/each}

          <!-- live draft path while linking -->
          {#if linkFrom && cursorPt}
            <path
              d={previewPolyline()}
              fill="none"
              stroke="var(--accent)"
              stroke-width="2"
              stroke-dasharray="5 4"
              opacity="0.75"
              marker-end="url(#er-arrow-head)"
              style="pointer-events: none;"
            />
            {#each draftWaypoints as wp, i}
              <circle cx={wp.x} cy={wp.y} r="4" fill="var(--accent)" style="pointer-events: none;" />
              <text
                x={wp.x + 6}
                y={wp.y - 6}
                fill="var(--text-muted)"
                font-size="10"
                style="pointer-events: none;">{i + 1}</text
              >
            {/each}
          {/if}
        </svg>

        {#each diagram.nodes as node (node.id)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            data-er-table
            class={cn(
              "absolute rounded-lg border bg-panel shadow-sm",
              selectedTableId === node.id
                ? "border-accent ring-2 ring-accent/30"
                : "border-line",
              arrowTool ? "cursor-default" : "cursor-grab",
              dragTableId === node.id && "cursor-grabbing",
            )}
            style="left: {node.x}px; top: {node.y}px; width: {TABLE_W}px; z-index: 2;"
            onpointerdown={(e) => onTablePointerDown(node.id, e)}
            onpointermove={onCanvasPointerMove}
            onpointerup={onTablePointerUp}
            onpointercancel={onTablePointerUp}
            oncontextmenu={(e) => onTableContextMenu(node.id, e)}
          >
            <div
              class="flex items-center justify-between gap-2 border-b border-line bg-elevated px-2.5"
              style="height: {HEADER_H}px"
            >
              {#if selectedTableId === node.id && !arrowTool}
                <input
                  class="min-w-0 flex-1 rounded border border-line bg-input px-1 py-0.5 text-body font-semibold text-fg"
                  value={node.name}
                  data-no-drag
                  oninput={(e) => renameTable((e.currentTarget as HTMLInputElement).value)}
                  onclick={(e) => e.stopPropagation()}
                />
              {:else}
                <span class="truncate text-body font-semibold text-fg">{node.name}</span>
              {/if}
            </div>

            <ul class="m-0 list-none p-0">
              {#each node.columns as col (col.id)}
                <li class="m-0 p-0" style="height: {ROW_H}px">
                  <button
                    type="button"
                    data-er-column
                    data-no-drag
                    class={cn(
                      "flex h-full w-full items-center gap-1 border-b border-line/50 px-2.5 text-left last:border-b-0",
                      arrowTool && "hover:bg-accent-muted",
                      linkFrom?.tableId === node.id &&
                        linkFrom?.columnId === col.id &&
                        "bg-accent-muted ring-1 ring-inset ring-accent",
                    )}
                    onclick={(e) => onColumnClick(node.id, col.id, e)}
                  >
                    {#if col.pk}
                      <span class="shrink-0 text-micro font-semibold text-accent">PK</span>
                    {/if}
                    <span class="min-w-0 flex-1 truncate font-mono text-caption text-fg">
                      {col.name}
                    </span>
                    <span class="shrink-0 font-mono text-caption text-fg-muted"
                      >{col.dataType}</span
                    >
                    {#if selectedTableId === node.id && !arrowTool}
                      <span
                        class="shrink-0 px-1 text-caption text-danger"
                        role="button"
                        tabindex="0"
                        onclick={(e) => {
                          e.stopPropagation();
                          deleteColumn(col.id);
                        }}
                      >
                        ×
                      </span>
                    {/if}
                  </button>
                </li>
              {/each}
            </ul>

            {#if selectedTableId === node.id && !arrowTool}
              <div class="border-t border-line p-2" data-no-drag>
                {#if !showAddCol}
                  <button
                    type="button"
                    class="w-full rounded-md border border-dashed border-line py-1.5 text-caption font-medium text-accent hover:bg-accent-muted"
                    onclick={(e) => {
                      e.stopPropagation();
                      showAddCol = true;
                    }}
                  >
                    + Add column
                  </button>
                {:else}
                  <div class="flex flex-col gap-1.5">
                    <Input bind:value={newColName} placeholder="column_name" inputSize="sm" />
                    {#if isSql}
                      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
                      <div role="presentation" onclick={(e) => e.stopPropagation()}>
                        <Select
                          bind:value={newColType}
                          options={SQL_TYPES.map((t) => ({ value: t, label: t }))}
                          inputSize="sm"
                        />
                      </div>
                    {:else}
                      <Input bind:value={newColType} placeholder="type" inputSize="sm" />
                    {/if}
                    <label class="flex items-center gap-1.5 text-caption text-fg-secondary">
                      <input type="checkbox" bind:checked={newColPk} />
                      Primary key
                    </label>
                    <div class="flex gap-1">
                      <Button
                        size="sm"
                        variant="primary"
                        onclick={(e: MouseEvent) => {
                          e.stopPropagation();
                          addColumn();
                        }}
                      >
                        Add
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        onclick={(e: MouseEvent) => {
                          e.stopPropagation();
                          showAddCol = false;
                        }}
                      >
                        Cancel
                      </Button>
                    </div>
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>

<!-- Right-click context menu for tables -->
{#if ctxMenu}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-40"
    role="presentation"
    onclick={() => (ctxMenu = null)}
    oncontextmenu={(e) => {
      e.preventDefault();
      ctxMenu = null;
    }}
  ></div>
  <div
    data-ctx-menu
    class="fixed z-50 min-w-[160px] overflow-hidden rounded-lg border border-line bg-panel py-1 shadow-lg"
    style="left: {ctxMenu.clientX}px; top: {ctxMenu.clientY}px;"
    role="menu"
  >
    <button
      type="button"
      role="menuitem"
      class="flex w-full items-center gap-2 px-3 py-2 text-left text-body text-fg hover:bg-accent-muted"
      onclick={() => {
        selectedTableId = ctxMenu!.tableId;
        selectedEdgeId = null;
        ctxMenu = null;
      }}
    >
      Select table
    </button>
    <button
      type="button"
      role="menuitem"
      class="flex w-full items-center gap-2 px-3 py-2 text-left text-body text-danger hover:bg-danger/10"
      onclick={() => deleteTableById(ctxMenu!.tableId)}
    >
      Delete table
    </button>
  </div>
{/if}
