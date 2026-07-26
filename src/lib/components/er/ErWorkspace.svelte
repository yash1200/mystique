<script lang="ts">
  /**
   * ER canvas — DBeaver-like column links on a grid.
   * - Multi-select tables (Ctrl/Cmd+click, Ctrl+A)
   * - Copy/paste tables (Ctrl+C / Ctrl+V) without relationships
   * - Right-click delete (table or canvas selection)
   * - Family-specific column type dropdowns
   * - Path reshape on grid + grid toggle
   * - Undo / redo (Ctrl+Z / Ctrl+Y)
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
    columnTypesForFamily,
    defaultColumnType,
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
  const PASTE_OFFSET = ER_GRID * 2;
  const MAX_HISTORY = 80;

  let diagram = $state<ErDiagramState | null>(null);
  let diagramId = $state<string | null>(null);
  let diagramName = $state("Untitled diagram");
  /** Multi-select table ids (order = selection order; last is primary for editing). */
  let selectedTableIds = $state<string[]>([]);
  let selectedEdgeId = $state<string | null>(null);
  let toolMsg = $state<string | null>(null);
  let saving = $state(false);
  let dirty = $state(false);
  let showGrid = $state(true);

  /** Undo / redo stacks (diagram snapshots taken before each change). */
  let undoStack = $state<ErDiagramState[]>([]);
  let redoStack = $state<ErDiagramState[]>([]);
  /** When true, mutations must not push new history (undo/redo apply). */
  let historyLocked = false;
  /** Snapshot once per continuous drag (tables / waypoints). */
  let dragHistoryPushed = false;
  /** Snapshot once per rename typing session. */
  let renameHistoryPushed = false;

  let arrowTool = $state<"normal" | "broken" | null>(null);
  let linkFrom = $state<{ tableId: string; columnId: string } | null>(null);
  let draftWaypoints = $state<ErPoint[]>([]);
  let cursorPt = $state<ErPoint | null>(null);

  // table drag (supports multi-drag)
  let dragTableId = $state<string | null>(null);
  let dragOffX = 0;
  let dragOffY = 0;
  let didDrag = false;
  let multiDragOrigins: Record<string, { x: number; y: number }> | null = null;
  let canvasEl: HTMLDivElement | null = $state(null);

  let dragWaypoint = $state<{ edgeId: string; index: number } | null>(null);

  /** Context menu: table or empty canvas */
  let ctxMenu = $state<{
    clientX: number;
    clientY: number;
    kind: "table" | "canvas";
    tableId?: string;
  } | null>(null);

  /** Internal clipboard of tables only (no relationships). */
  let tableClipboard = $state<ErNode[] | null>(null);

  let showAddCol = $state(false);
  let newColName = $state("");
  let newColType = $state(defaultColumnType("sql"));
  let newColPk = $state(false);

  const meta = $derived(FAMILY_META[family]);
  const isSql = $derived(family === "sql");
  const typeOptions = $derived(
    columnTypesForFamily(family).map((t) => ({ value: t, label: t })),
  );
  /** Primary table for rename / columns UI when exactly one selected. */
  const primaryTableId = $derived(
    selectedTableIds.length === 1 ? selectedTableIds[0]! : null,
  );
  const selectionCount = $derived(selectedTableIds.length);
  const canUndo = $derived(undoStack.length > 0);
  const canRedo = $derived(redoStack.length > 0);

  $effect(() => {
    diagram = emptyDiagram(family);
    diagramId = null;
    diagramName = `Untitled ${FAMILY_META[family].short} diagram`;
    selectedTableIds = [];
    selectedEdgeId = null;
    arrowTool = null;
    linkFrom = null;
    draftWaypoints = [];
    cursorPt = null;
    showAddCol = false;
    dirty = false;
    ctxMenu = null;
    dragWaypoint = null;
    tableClipboard = null;
    newColType = defaultColumnType(family);
    clearHistory();
    renameHistoryPushed = false;
    toolMsg =
      "Ctrl+Z undo · Ctrl+Y redo · Ctrl+A select all · Ctrl+C/V copy tables · Right-click Delete.";
  });

  function uid(p: string) {
    return `${p}_${Math.random().toString(36).slice(2, 9)}`;
  }

  function cloneDiagram(d: ErDiagramState): ErDiagramState {
    return structuredClone(d);
  }

  function clearHistory() {
    undoStack = [];
    redoStack = [];
    historyLocked = false;
    dragHistoryPushed = false;
  }

  /** Call immediately before mutating `diagram` so the prior state is undoable. */
  function snapshotBeforeChange() {
    if (!diagram || historyLocked) return;
    undoStack = [...undoStack, cloneDiagram(diagram)].slice(-MAX_HISTORY);
    redoStack = [];
  }

  function markDirty() {
    dirty = true;
  }

  function pruneSelectionToDiagram() {
    if (!diagram) {
      selectedTableIds = [];
      selectedEdgeId = null;
      return;
    }
    const nodeIds = new Set(diagram.nodes.map((n) => n.id));
    selectedTableIds = selectedTableIds.filter((id) => nodeIds.has(id));
    if (selectedEdgeId && !diagram.edges.some((e) => e.id === selectedEdgeId)) {
      selectedEdgeId = null;
    }
  }

  function undo() {
    if (!diagram || undoStack.length === 0) {
      toolMsg = "Nothing to undo.";
      return;
    }
    historyLocked = true;
    redoStack = [cloneDiagram(diagram), ...redoStack].slice(0, MAX_HISTORY);
    diagram = undoStack[undoStack.length - 1]!;
    undoStack = undoStack.slice(0, -1);
    linkFrom = null;
    draftWaypoints = [];
    cursorPt = null;
    showAddCol = false;
    ctxMenu = null;
    dragWaypoint = null;
    pruneSelectionToDiagram();
    dirty = true;
    historyLocked = false;
    toolMsg = "Undid last change.";
  }

  function redo() {
    if (!diagram || redoStack.length === 0) {
      toolMsg = "Nothing to redo.";
      return;
    }
    historyLocked = true;
    undoStack = [...undoStack, cloneDiagram(diagram)].slice(-MAX_HISTORY);
    diagram = redoStack[0]!;
    redoStack = redoStack.slice(1);
    linkFrom = null;
    draftWaypoints = [];
    cursorPt = null;
    showAddCol = false;
    ctxMenu = null;
    dragWaypoint = null;
    pruneSelectionToDiagram();
    dirty = true;
    historyLocked = false;
    toolMsg = "Redid last change.";
  }

  function isEditableTarget(t: EventTarget | null): boolean {
    if (!(t instanceof HTMLElement)) return false;
    const tag = t.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
    if (t.isContentEditable) return true;
    return Boolean(t.closest("input, textarea, select, [contenteditable='true']"));
  }

  function canvasPoint(e: { clientX: number; clientY: number }): ErPoint | null {
    if (!canvasEl) return null;
    const rect = canvasEl.getBoundingClientRect();
    return {
      x: e.clientX - rect.left + canvasEl.scrollLeft,
      y: e.clientY - rect.top + canvasEl.scrollTop,
    };
  }

  function setSelection(ids: string[]) {
    selectedTableIds = [...new Set(ids)];
    selectedEdgeId = null;
    if (selectedTableIds.length !== 1) showAddCol = false;
  }

  function selectOnly(tableId: string) {
    setSelection([tableId]);
  }

  function toggleSelect(tableId: string) {
    if (selectedTableIds.includes(tableId)) {
      selectedTableIds = selectedTableIds.filter((id) => id !== tableId);
    } else {
      selectedTableIds = [...selectedTableIds, tableId];
    }
    selectedEdgeId = null;
    showAddCol = false;
  }

  function selectAllTables() {
    if (!diagram) return;
    setSelection(diagram.nodes.map((n) => n.id));
    toolMsg = `Selected ${diagram.nodes.length} table(s). Right-click → Delete, or Ctrl+C to copy.`;
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
        "Move mode: drag tables · Ctrl+click multi-select · Ctrl+A all · Ctrl+C/V copy/paste · Right-click Delete.";
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

  function segmentMids(edge: ErEdge): { x: number; y: number; insertAt: number }[] {
    const pts = edgePoints(edge);
    const mids: { x: number; y: number; insertAt: number }[] = [];
    for (let i = 0; i < pts.length - 1; i++) {
      mids.push({
        x: (pts[i].x + pts[i + 1].x) / 2,
        y: (pts[i].y + pts[i + 1].y) / 2,
        insertAt: i,
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
    snapshotBeforeChange();
    diagram = { ...diagram, nodes: [...diagram.nodes, node] };
    selectOnly(node.id);
    showAddCol = false;
    toolMsg = `Added “${node.name}”.`;
    markDirty();
  }

  function onColumnClick(tableId: string, columnId: string, e: MouseEvent) {
    e.stopPropagation();
    e.preventDefault();
    ctxMenu = null;

    if (!arrowTool) {
      if (e.ctrlKey || e.metaKey) toggleSelect(tableId);
      else selectOnly(tableId);
      showAddCol = false;
      return;
    }

    if (!linkFrom) {
      linkFrom = { tableId, columnId };
      draftWaypoints = [];
      selectOnly(tableId);
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

    snapshotBeforeChange();
    diagram = { ...diagram, edges: [...diagram.edges, edge] };
    toolMsg = `Linked: ${label}. Click the path later to reshape (Ctrl+Z to undo).`;
    linkFrom = null;
    draftWaypoints = [];
    cursorPt = null;
    // Leave the new arrow unselected so the canvas stays ready for the next action
    selectedEdgeId = null;
    selectedTableIds = [];
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

    if (arrowTool && linkFrom) {
      const pt = canvasPoint(e);
      if (!pt) return;
      const snapped = { x: snapToGrid(pt.x), y: snapToGrid(pt.y) };
      draftWaypoints = [...draftWaypoints, snapped];
      toolMsg = `Bend point ${draftWaypoints.length} added. Click more points or click the target column.`;
      return;
    }

    selectedTableIds = [];
    selectedEdgeId = null;
    showAddCol = false;
  }

  function onCanvasPointerMove(e: PointerEvent) {
    if (arrowTool && linkFrom) {
      cursorPt = canvasPoint(e);
    }

    if (dragWaypoint && diagram) {
      const pt = canvasPoint(e);
      if (!pt) return;
      const snapped = { x: snapToGrid(pt.x), y: snapToGrid(pt.y) };
      const { edgeId, index } = dragWaypoint;
      if (!dragHistoryPushed) {
        snapshotBeforeChange();
        dragHistoryPushed = true;
      }
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

    if (!dragTableId || !diagram || !canvasEl) return;
    const rect = canvasEl.getBoundingClientRect();
    const x = snapToGrid(
      Math.max(0, e.clientX - rect.left + canvasEl.scrollLeft - dragOffX),
    );
    const y = snapToGrid(
      Math.max(0, e.clientY - rect.top + canvasEl.scrollTop - dragOffY),
    );

    if (multiDragOrigins && selectedTableIds.includes(dragTableId) && selectedTableIds.length > 1) {
      const origin = multiDragOrigins[dragTableId];
      if (!origin) return;
      const dx = x - origin.x;
      const dy = y - origin.y;
      if (dx !== 0 || dy !== 0) {
        if (!dragHistoryPushed) {
          snapshotBeforeChange();
          dragHistoryPushed = true;
        }
        didDrag = true;
      }
      diagram = {
        ...diagram,
        nodes: diagram.nodes.map((n) => {
          if (!selectedTableIds.includes(n.id)) return n;
          const o = multiDragOrigins![n.id];
          if (!o) return n;
          return {
            ...n,
            x: snapToGrid(Math.max(0, o.x + dx)),
            y: snapToGrid(Math.max(0, o.y + dy)),
          };
        }),
      };
      return;
    }

    const cur = diagram.nodes.find((n) => n.id === dragTableId);
    if (cur && (cur.x !== x || cur.y !== y)) {
      if (!dragHistoryPushed) {
        snapshotBeforeChange();
        dragHistoryPushed = true;
      }
      didDrag = true;
    }
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

    // Selection: Ctrl/Cmd toggles; plain click selects only (unless already multi-selected for group drag)
    if (e.ctrlKey || e.metaKey) {
      toggleSelect(tableId);
    } else if (!selectedTableIds.includes(tableId)) {
      selectOnly(tableId);
    }
    // if already in multi-selection, keep it for group drag

    const rect = canvasEl.getBoundingClientRect();
    dragTableId = tableId;
    didDrag = false;
    dragHistoryPushed = false;
    dragOffX = e.clientX - rect.left + canvasEl.scrollLeft - node.x;
    dragOffY = e.clientY - rect.top + canvasEl.scrollTop - node.y;
    selectedEdgeId = null;
    ctxMenu = null;
    renameHistoryPushed = false;

    if (selectedTableIds.includes(tableId) && selectedTableIds.length > 1) {
      multiDragOrigins = {};
      for (const n of diagram.nodes) {
        if (selectedTableIds.includes(n.id)) {
          multiDragOrigins[n.id] = { x: n.x, y: n.y };
        }
      }
    } else {
      multiDragOrigins = null;
    }

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
    multiDragOrigins = null;
  }

  function onTableContextMenu(tableId: string, e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    // If right-clicked table is not in selection, select only it; else keep multi-selection
    if (!selectedTableIds.includes(tableId)) {
      selectOnly(tableId);
    }
    selectedEdgeId = null;
    ctxMenu = { clientX: e.clientX, clientY: e.clientY, kind: "table", tableId };
  }

  function onCanvasContextMenu(e: MouseEvent) {
    const t = e.target as HTMLElement;
    if (t.closest("[data-er-table]") || t.closest("[data-ctx-menu]")) return;
    e.preventDefault();
    ctxMenu = { clientX: e.clientX, clientY: e.clientY, kind: "canvas" };
  }

  function deleteTablesByIds(ids: string[]) {
    if (!diagram || !ids.length) return;
    const set = new Set(ids);
    snapshotBeforeChange();
    diagram = {
      ...diagram,
      nodes: diagram.nodes.filter((n) => !set.has(n.id)),
      edges: diagram.edges.filter((e) => !set.has(e.from) && !set.has(e.to)),
    };
    selectedTableIds = selectedTableIds.filter((id) => !set.has(id));
    showAddCol = false;
    ctxMenu = null;
    toolMsg =
      ids.length === 1 ? "Table deleted." : `Deleted ${ids.length} tables.`;
    markDirty();
  }

  function deleteSelection() {
    if (selectedEdgeId && !selectedTableIds.length) {
      deleteEdge(selectedEdgeId);
      return;
    }
    if (selectedTableIds.length) {
      deleteTablesByIds([...selectedTableIds]);
    }
  }

  function copySelectedTables() {
    if (!diagram || !selectedTableIds.length) {
      toolMsg = "Select one or more tables to copy (Ctrl+C).";
      return;
    }
    // Deep clone tables only — no relationships
    const nodes = diagram.nodes
      .filter((n) => selectedTableIds.includes(n.id))
      .map((n) => ({
        ...n,
        columns: n.columns.map((c) => ({ ...c })),
      }));
    tableClipboard = nodes;
    toolMsg = `Copied ${nodes.length} table(s) without relationships. Ctrl+V to paste.`;
  }

  function pasteTables() {
    if (!diagram || !tableClipboard?.length) {
      toolMsg = "Clipboard empty — copy tables with Ctrl+C first.";
      return;
    }
    const newIds: string[] = [];
    const pasted: ErNode[] = tableClipboard.map((src) => {
      const colIdMap = new Map<string, string>();
      const columns = src.columns.map((c) => {
        const nid = uid("c");
        colIdMap.set(c.id, nid);
        return { ...c, id: nid };
      });
      const id = uid("n");
      newIds.push(id);
      return {
        ...src,
        id,
        name: `${src.name}_copy`,
        columns,
        x: snapToGrid(src.x + PASTE_OFFSET),
        y: snapToGrid(src.y + PASTE_OFFSET),
      };
    });
    // Shift clipboard so successive pastes cascade
    tableClipboard = tableClipboard.map((n) => ({
      ...n,
      columns: n.columns.map((c) => ({ ...c })),
      x: n.x + PASTE_OFFSET,
      y: n.y + PASTE_OFFSET,
    }));
    snapshotBeforeChange();
    diagram = { ...diagram, nodes: [...diagram.nodes, ...pasted] };
    setSelection(newIds);
    toolMsg = `Pasted ${pasted.length} table(s) (no relationships).`;
    markDirty();
  }

  function onKeyDown(e: KeyboardEvent) {
    if (isEditableTarget(e.target)) {
      // Still allow undo/redo while typing? Prefer not — let OS handle text undo in inputs.
      return;
    }
    const mod = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();

    if (mod && key === "z" && !e.shiftKey) {
      e.preventDefault();
      undo();
      return;
    }
    if (mod && (key === "y" || (key === "z" && e.shiftKey))) {
      e.preventDefault();
      redo();
      return;
    }
    if (mod && key === "a") {
      e.preventDefault();
      if (!arrowTool) selectAllTables();
      return;
    }
    if (mod && key === "c") {
      e.preventDefault();
      if (!arrowTool) copySelectedTables();
      return;
    }
    if (mod && key === "v") {
      e.preventDefault();
      if (!arrowTool) pasteTables();
      return;
    }
    if (e.key === "Delete" || e.key === "Backspace") {
      if (selectedTableIds.length || selectedEdgeId) {
        e.preventDefault();
        deleteSelection();
      }
      return;
    }
    if (e.key === "Escape") {
      ctxMenu = null;
      if (linkFrom) cancelLink();
      else {
        selectedTableIds = [];
        selectedEdgeId = null;
      }
    }
  }

  function renameTable(name: string) {
    if (!diagram || !primaryTableId) return;
    if (!renameHistoryPushed) {
      snapshotBeforeChange();
      renameHistoryPushed = true;
    }
    diagram = {
      ...diagram,
      nodes: diagram.nodes.map((n) =>
        n.id === primaryTableId ? { ...n, name: name || n.name } : n,
      ),
    };
    markDirty();
  }

  function updateColumnType(tableId: string, colId: string, dataType: string) {
    if (!diagram) return;
    snapshotBeforeChange();
    diagram = {
      ...diagram,
      nodes: diagram.nodes.map((n) =>
        n.id === tableId
          ? {
              ...n,
              columns: n.columns.map((c) =>
                c.id === colId ? { ...c, dataType } : c,
              ),
            }
          : n,
      ),
    };
    markDirty();
  }

  function addColumn() {
    if (!diagram || !primaryTableId) return;
    const name = newColName.trim();
    if (!name) {
      toolMsg = "Column name is required.";
      return;
    }
    const col: ErColumn = {
      id: uid("c"),
      name,
      dataType: newColType || defaultColumnType(family),
      pk: newColPk,
    };
    snapshotBeforeChange();
    diagram = {
      ...diagram,
      nodes: diagram.nodes.map((n) => {
        if (n.id !== primaryTableId) return n;
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
    newColType = defaultColumnType(family);
    newColPk = false;
    showAddCol = false;
    markDirty();
  }

  function deleteColumn(colId: string) {
    if (!diagram || !primaryTableId) return;
    snapshotBeforeChange();
    diagram = {
      ...diagram,
      nodes: diagram.nodes.map((n) =>
        n.id === primaryTableId
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
    snapshotBeforeChange();
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
    selectedTableIds = [];
    showAddCol = false;
    ctxMenu = null;
    if (!arrowTool) {
      toolMsg =
        "Link selected. Drag handles to reshape. Delete key removes the link.";
    }
  }

  function onWaypointDown(edgeId: string, index: number, e: PointerEvent) {
    e.stopPropagation();
    e.preventDefault();
    selectedEdgeId = edgeId;
    selectedTableIds = [];
    dragHistoryPushed = false;
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
    snapshotBeforeChange();
    dragHistoryPushed = true; // already snapshotted for this gesture
    diagram = {
      ...diagram,
      edges: diagram.edges.map((ed) =>
        ed.id === edgeId ? { ...ed, waypoints: wps } : ed,
      ),
    };
    selectedEdgeId = edgeId;
    selectedTableIds = [];
    dragWaypoint = { edgeId, index: insertAt };
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  }

  function removeWaypoint(edgeId: string, index: number, e: Event) {
    e.stopPropagation();
    e.preventDefault();
    if (!diagram) return;
    snapshotBeforeChange();
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
    snapshotBeforeChange();
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
      selectedTableIds = [];
      selectedEdgeId = null;
      clearHistory();
      toolMsg = `Loaded “${rec.name}”.`;
    } catch (e) {
      toolMsg = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<svelte:window
  onpointerup={onWindowPointerUp}
  onpointercancel={onWindowPointerUp}
  onkeydown={onKeyDown}
/>

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
      {#if selectionCount > 0}
        <Badge tone="neutral" size="sm">{selectionCount} selected</Badge>
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

      <div class="grid grid-cols-2 gap-2">
        <Button variant="secondary" size="sm" fullWidth onclick={undo} disabled={!canUndo}>
          Undo
        </Button>
        <Button variant="secondary" size="sm" fullWidth onclick={redo} disabled={!canRedo}>
          Redo
        </Button>
      </div>

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
        <li>Normal/Broken → columns to link</li>
        <li>Ctrl+Z / Ctrl+Y undo · redo</li>
        <li>Ctrl+A select all tables</li>
        <li>Ctrl+C / Ctrl+V copy-paste (no links)</li>
        <li>Right-click → Delete</li>
        <li>Click path to reshape on grid</li>
      </ol>

      {#if linkFrom}
        <Button variant="secondary" fullWidth onclick={cancelLink}>Cancel link</Button>
      {/if}
      {#if selectedEdgeId}
        <Button variant="ghost" fullWidth onclick={straightenSelectedEdge}>
          Straighten selected link
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
      tabindex="0"
      onclick={onCanvasClick}
      onpointermove={onCanvasPointerMove}
      oncontextmenu={onCanvasContextMenu}
    >
      {#if diagram}
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
          </defs>

          {#each diagram.edges as edge (edge.id)}
            {@const selected = selectedEdgeId === edge.id}
            <path
              data-er-edge-hit
              d={edgePolyline(edge)}
              fill="none"
              stroke="transparent"
              stroke-width="14"
              style="pointer-events: stroke; cursor: pointer;"
              onclick={(e) => selectEdge(edge.id, e)}
            />
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
          {@const isSelected = selectedTableIds.includes(node.id)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            data-er-table
            class={cn(
              "absolute rounded-lg border bg-panel shadow-sm",
              isSelected ? "border-accent ring-2 ring-accent/30" : "border-line",
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
              {#if primaryTableId === node.id && !arrowTool}
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
                  {#if primaryTableId === node.id && !arrowTool}
                    <!-- Edit row: name fixed, type dropdown, delete -->
                    <div
                      data-er-column
                      data-no-drag
                      class="flex h-full w-full items-center gap-1 border-b border-line/50 px-2 last:border-b-0"
                    >
                      {#if col.pk}
                        <span class="shrink-0 text-micro font-semibold text-accent">PK</span>
                      {/if}
                      <span class="min-w-0 flex-1 truncate font-mono text-caption text-fg">
                        {col.name}
                      </span>
                      <div
                        class="w-[7.5rem] shrink-0"
                        role="presentation"
                        onclick={(e) => e.stopPropagation()}
                        onpointerdown={(e) => e.stopPropagation()}
                      >
                        <Select
                          value={col.dataType}
                          options={typeOptions.some((o) => o.value === col.dataType)
                            ? typeOptions
                            : [{ value: col.dataType, label: col.dataType }, ...typeOptions]}
                          inputSize="sm"
                          onchange={(v) => updateColumnType(node.id, col.id, v)}
                        />
                      </div>
                      <button
                        type="button"
                        class="shrink-0 px-1 text-caption text-danger"
                        onclick={(e) => {
                          e.stopPropagation();
                          deleteColumn(col.id);
                        }}
                      >
                        ×
                      </button>
                    </div>
                  {:else}
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
                    </button>
                  {/if}
                </li>
              {/each}
            </ul>

            {#if primaryTableId === node.id && !arrowTool}
              <div class="border-t border-line p-2" data-no-drag>
                {#if !showAddCol}
                  <button
                    type="button"
                    class="w-full rounded-md border border-dashed border-line py-1.5 text-caption font-medium text-accent hover:bg-accent-muted"
                    onclick={(e) => {
                      e.stopPropagation();
                      newColType = defaultColumnType(family);
                      showAddCol = true;
                    }}
                  >
                    + Add column
                  </button>
                {:else}
                  <div class="flex flex-col gap-1.5">
                    <Input bind:value={newColName} placeholder="column_name" inputSize="sm" />
                    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
                    <div role="presentation" onclick={(e) => e.stopPropagation()}>
                      <Select
                        bind:value={newColType}
                        options={typeOptions}
                        inputSize="sm"
                        placeholder="Column type"
                      />
                    </div>
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

<!-- Context menu: table or canvas -->
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
    class="fixed z-50 min-w-[180px] overflow-hidden rounded-lg border border-line bg-panel py-1 shadow-lg"
    style="left: {ctxMenu.clientX}px; top: {ctxMenu.clientY}px;"
    role="menu"
  >
    {#if ctxMenu.kind === "canvas"}
      <button
        type="button"
        role="menuitem"
        class="flex w-full items-center justify-between gap-4 px-3 py-2 text-left text-body text-fg hover:bg-accent-muted"
        onclick={() => {
          selectAllTables();
          ctxMenu = null;
        }}
      >
        <span>Select all</span>
        <span class="text-caption text-fg-muted">Ctrl+A</span>
      </button>
      <button
        type="button"
        role="menuitem"
        class={cn(
          "flex w-full items-center justify-between gap-4 px-3 py-2 text-left text-body hover:bg-accent-muted",
          tableClipboard?.length ? "text-fg" : "text-fg-muted",
        )}
        disabled={!tableClipboard?.length}
        onclick={() => {
          pasteTables();
          ctxMenu = null;
        }}
      >
        <span>Paste tables</span>
        <span class="text-caption text-fg-muted">Ctrl+V</span>
      </button>
      {#if selectionCount > 0}
        <div class="my-1 border-t border-line"></div>
        <button
          type="button"
          role="menuitem"
          class="flex w-full items-center justify-between gap-4 px-3 py-2 text-left text-body text-danger hover:bg-danger/10"
          onclick={() => {
            deleteSelection();
            ctxMenu = null;
          }}
        >
          <span>
            Delete {selectionCount === 1 ? "table" : `${selectionCount} tables`}
          </span>
          <span class="text-caption opacity-70">Del</span>
        </button>
      {/if}
    {:else}
      <button
        type="button"
        role="menuitem"
        class="flex w-full items-center justify-between gap-4 px-3 py-2 text-left text-body text-fg hover:bg-accent-muted"
        onclick={() => {
          copySelectedTables();
          ctxMenu = null;
        }}
      >
        <span>Copy {selectionCount > 1 ? `${selectionCount} tables` : "table"}</span>
        <span class="text-caption text-fg-muted">Ctrl+C</span>
      </button>
      <button
        type="button"
        role="menuitem"
        class={cn(
          "flex w-full items-center justify-between gap-4 px-3 py-2 text-left text-body hover:bg-accent-muted",
          tableClipboard?.length ? "text-fg" : "text-fg-muted",
        )}
        disabled={!tableClipboard?.length}
        onclick={() => {
          pasteTables();
          ctxMenu = null;
        }}
      >
        <span>Paste</span>
        <span class="text-caption text-fg-muted">Ctrl+V</span>
      </button>
      <div class="my-1 border-t border-line"></div>
      <button
        type="button"
        role="menuitem"
        class="flex w-full items-center justify-between gap-4 px-3 py-2 text-left text-body text-danger hover:bg-danger/10"
        onclick={() => {
          deleteSelection();
          ctxMenu = null;
        }}
      >
        <span>
          Delete {selectionCount > 1 ? `${selectionCount} tables` : "table"}
        </span>
        <span class="text-caption opacity-70">Del</span>
      </button>
    {/if}
  </div>
{/if}
