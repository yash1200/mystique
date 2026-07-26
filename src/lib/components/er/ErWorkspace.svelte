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
  import { diagramToDbml, dbmlToDiagram, sampleDbml } from "$lib/utils/dbml";

  interface Props {
    family: DataFamily;
    onback: () => void;
    onchangeFamily: () => void;
  }

  let { family, onback, onchangeFamily }: Props = $props();

  const TABLE_W = 280;
  const HEADER_H = 40;
  /** Row height must fit inline type Select (h-control-sm = 28px) with breathing room. */
  const ROW_H = 34;
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
  /**
   * Right-hand live DBML panel (read-only export).
   * Auto-hidden when left DBML editor (source mode) is open.
   */
  let showDbml = $state(true);
  let dbmlCopied = $state(false);
  /**
   * Left-hand DBML source editor — user writes text → Apply builds the canvas.
   * When true, the right DBML panel is forced closed.
   */
  let dbmlSourceMode = $state(false);
  let dbmlEditorText = $state("");
  let dbmlParseError = $state<string | null>(null);

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
  /** Live DBML text generated from the canvas diagram (right panel). */
  const dbmlText = $derived(diagramToDbml(diagram, diagramName));
  /** Right panel only when export view is on AND source editor is off. */
  const showRightDbml = $derived(showDbml && !dbmlSourceMode);

  /** Non-reactive: only reset workspace when the family prop actually changes. */
  let lastInitFamily: DataFamily | null = null;

  $effect(() => {
    const f = family;
    if (lastInitFamily === f) return;
    lastInitFamily = f;

    diagram = emptyDiagram(f);
    diagramId = null;
    diagramName = `Untitled ${FAMILY_META[f].short} diagram`;
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
    newColType = defaultColumnType(f);
    dbmlSourceMode = false;
    dbmlEditorText = "";
    dbmlParseError = null;
    showDbml = true;
    clearHistory();
    renameHistoryPushed = false;
    toolMsg =
      "DBML editor (left) builds the diagram from text · export panel (right) shows live DBML.";
  });

  function uid(p: string) {
    return `${p}_${Math.random().toString(36).slice(2, 9)}`;
  }

  /**
   * Deep-clone diagram for undo history.
   * Must not use structuredClone — it throws DataCloneError on Svelte 5 $state
   * proxies, which aborted add/delete before the diagram was updated.
   * JSON works with $state proxies (enumerable plain data).
   */
  function cloneDiagram(d: ErDiagramState): ErDiagramState {
    return JSON.parse(JSON.stringify(d)) as ErDiagramState;
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
    try {
      undoStack = [...undoStack, cloneDiagram(diagram)].slice(-MAX_HISTORY);
      redoStack = [];
    } catch (err) {
      console.warn("ER history snapshot failed", err);
    }
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

  function columnRowY(node: ErNode, columnId: string): number {
    const idx = Math.max(0, node.columns.findIndex((c) => c.id === columnId));
    return node.y + HEADER_H + idx * ROW_H + ROW_H / 2;
  }

  function columnCenter(node: ErNode, columnId: string): ErPoint {
    return {
      x: node.x + TABLE_W / 2,
      y: columnRowY(node, columnId),
    };
  }

  /**
   * Anchor on the left or right edge of the table at the column row.
   * Slightly outside the border so the arrowhead is not covered by the card.
   */
  function columnSideAnchor(
    node: ErNode,
    columnId: string,
    side: "left" | "right",
  ): ErPoint {
    const pad = 4;
    return {
      x: side === "left" ? node.x - pad : node.x + TABLE_W + pad,
      y: columnRowY(node, columnId),
    };
  }

  type TableSide = "left" | "right";

  /** Pick left/right side facing the next/prev path point. */
  function sideToward(node: ErNode, towardX: number): TableSide {
    const mid = node.x + TABLE_W / 2;
    return towardX >= mid ? "right" : "left";
  }

  function tableMidY(node: ErNode): number {
    return node.y + HEADER_H + (node.columns.length * ROW_H) / 2;
  }

  /**
   * Tables stacked one above the other (or mostly vertical): prefer same-side
   * left→left or right→right routing instead of a mid-X cut through the cards.
   */
  function isVerticalStack(a: ErNode, b: ErNode): boolean {
    const aMidX = a.x + TABLE_W / 2;
    const bMidX = b.x + TABLE_W / 2;
    const dx = Math.abs(aMidX - bMidX);
    const dy = Math.abs(tableMidY(a) - tableMidY(b));
    const hOverlap =
      Math.min(a.x + TABLE_W, b.x + TABLE_W) - Math.max(a.x, b.x);
    // Overlapping horizontally, or vertical separation dominates
    return hOverlap > TABLE_W * 0.2 || (dy > dx && dy > ER_GRID);
  }

  /**
   * Outer side for a vertical stack: same side on both tables.
   * Prefer the side of the wider "overhang" so the corridor clears both cards.
   */
  function pickStackedOuterSide(a: ErNode, b: ErNode): TableSide {
    const maxRight = Math.max(a.x + TABLE_W, b.x + TABLE_W);
    const minLeft = Math.min(a.x, b.x);
    // How much each side sticks out beyond the other table
    const rightClearance =
      maxRight - Math.min(a.x + TABLE_W, b.x + TABLE_W);
    const leftClearance = Math.max(a.x, b.x) - minLeft;
    // Prefer right (self-loop convention); use left only if left overhang is clearly larger
    if (leftClearance > rightClearance + ER_GRID * 2) return "left";
    return "right";
  }

  /**
   * Choose exit side on source and entry side on target.
   * - Custom path: face the first/last waypoint
   * - Vertical stack: SAME side (left+left or right+right)
   * - Side-by-side: face each other (right→left or left→right)
   */
  function pickLinkSides(
    a: ErNode,
    b: ErNode,
    wps: ErPoint[],
  ): { fromSide: TableSide; toSide: TableSide } {
    if (a.id === b.id) {
      return { fromSide: "right", toSide: "right" };
    }
    if (wps.length > 0) {
      return {
        fromSide: sideToward(a, wps[0]!.x),
        toSide: sideToward(b, wps[wps.length - 1]!.x),
      };
    }
    if (isVerticalStack(a, b)) {
      const side = pickStackedOuterSide(a, b);
      return { fromSide: side, toSide: side };
    }
    // Side-by-side: leave the facing edges
    const aMid = a.x + TABLE_W / 2;
    const bMid = b.x + TABLE_W / 2;
    if (bMid >= aMid) {
      return { fromSide: "right", toSide: "left" };
    }
    return { fromSide: "left", toSide: "right" };
  }

  function samePoint(a: ErPoint, b: ErPoint, eps = 0.5): boolean {
    return Math.abs(a.x - b.x) < eps && Math.abs(a.y - b.y) < eps;
  }

  /**
   * Force a polyline to axis-aligned (Manhattan / 90°) segments only.
   * Diagonal spans get a horizontal-then-vertical elbow.
   */
  function toOrthogonal(pts: ErPoint[]): ErPoint[] {
    if (pts.length === 0) return [];
    const out: ErPoint[] = [{ x: pts[0].x, y: pts[0].y }];
    for (let i = 1; i < pts.length; i++) {
      const prev = out[out.length - 1]!;
      const cur = pts[i]!;
      if (samePoint(prev, cur)) continue;
      if (prev.x !== cur.x && prev.y !== cur.y) {
        // Perfect square corner: horizontal first, then vertical
        const elbow = { x: cur.x, y: prev.y };
        if (!samePoint(prev, elbow)) out.push(elbow);
      }
      const last = out[out.length - 1]!;
      if (!samePoint(last, cur)) out.push({ x: cur.x, y: cur.y });
    }
    return out;
  }

  /**
   * Snap a free pointer position so the segment from `prev` is only
   * horizontal OR vertical (no diagonal freehand). Grid-snaps the free axis.
   */
  function orthoGridSnapFrom(prev: ErPoint, raw: ErPoint): ErPoint {
    const gx = snapToGrid(raw.x);
    const gy = snapToGrid(raw.y);
    const dx = Math.abs(gx - prev.x);
    const dy = Math.abs(gy - prev.y);
    if (dx === 0 && dy === 0) return { x: prev.x, y: prev.y };
    // Dominant axis wins — lock the other coordinate to prev (exact, for column Y)
    if (dx >= dy) return { x: gx, y: prev.y };
    return { x: prev.x, y: gy };
  }

  /**
   * Stable lane index among edges that share the same table pair direction
   * (A→B). Used so parallel column links get separate corridors.
   */
  function edgeLaneIndex(edge: ErEdge): number {
    if (!diagram) return 0;
    const peers = diagram.edges
      .filter((e) => e.from === edge.from && e.to === edge.to)
      .slice()
      .sort((a, b) => a.id.localeCompare(b.id));
    const idx = peers.findIndex((e) => e.id === edge.id);
    return idx < 0 ? 0 : idx;
  }

  /**
   * Default path shape is always:
   *   exit horizontally → vertical run → enter horizontally
   * so the arrow head points left/right (──▶ or ◀──), never up/down.
   *
   * `lane` offsets the vertical corridor so multiple A→B links stay separate.
   */
  function defaultManhattanWaypoints(
    start: ErPoint,
    end: ErPoint,
    fromSide: TableSide,
    toSide: TableSide,
    lane = 0,
  ): ErPoint[] {
    const lanePad = lane * ER_GRID;

    // Pure horizontal (same row): already ──▶ / ◀──, no elbows needed
    if (Math.abs(start.y - end.y) < 0.5) {
      // Still separate parallel same-row links slightly in Y? skip — rare
      return [];
    }

    // NEVER pure vertical (same X): that makes the arrow tip point up/down.
    // Always detour: ──┐ then │ then └──▶

    // Same side (stacked tables or self-loop style): outer vertical corridor
    if (fromSide === toSide) {
      const basePad = ER_GRID * 2 + lanePad;
      const corridorX =
        fromSide === "right"
          ? snapToGrid(Math.max(start.x, end.x) + basePad)
          : snapToGrid(Math.min(start.x, end.x) - basePad);
      return [
        { x: corridorX, y: start.y },
        { x: corridorX, y: end.y },
      ];
    }

    // Opposite sides (side-by-side tables): mid corridor with per-lane offset
    let midX = (start.x + end.x) / 2;
    // Spread parallel links above/below the midpoint along X
    const spread =
      lane === 0
        ? 0
        : (lane % 2 === 1 ? 1 : -1) * Math.ceil(lane / 2) * ER_GRID;
    midX = snapToGrid(midX + spread);

    // Ensure we actually leave horizontally from start (mid ≠ start.x)
    if (Math.abs(midX - start.x) < ER_GRID) {
      midX = snapToGrid(
        start.x + (fromSide === "right" ? ER_GRID * 2 : -ER_GRID * 2) + lanePad,
      );
    }
    // Ensure we approach end horizontally (mid ≠ end.x)
    if (Math.abs(midX - end.x) < ER_GRID) {
      midX = snapToGrid(
        end.x + (toSide === "left" ? -ER_GRID * 2 : ER_GRID * 2) - lanePad,
      );
    }

    return [
      { x: midX, y: start.y },
      { x: midX, y: end.y },
    ];
  }

  /**
   * Guarantee the first and last path segments are horizontal so markers
   * point left/right into the column sides.
   */
  function ensureHorizontalPorts(
    pts: ErPoint[],
    fromSide: TableSide,
    toSide: TableSide,
    lane = 0,
  ): ErPoint[] {
    if (pts.length < 2) return pts;
    let out = pts.map((p) => ({ x: p.x, y: p.y }));
    const pad = ER_GRID * 2 + lane * ER_GRID;

    // Fix first segment if vertical
    const s0 = out[0]!;
    const s1 = out[1]!;
    if (Math.abs(s0.x - s1.x) < 0.5 && Math.abs(s0.y - s1.y) > 0.5) {
      const cx =
        fromSide === "right"
          ? snapToGrid(s0.x + pad)
          : snapToGrid(s0.x - pad);
      out = [s0, { x: cx, y: s0.y }, { x: cx, y: s1.y }, ...out.slice(2)];
    }

    // Fix last segment if vertical
    if (out.length >= 2) {
      const e0 = out[out.length - 2]!;
      const e1 = out[out.length - 1]!;
      if (Math.abs(e0.x - e1.x) < 0.5 && Math.abs(e0.y - e1.y) > 0.5) {
        const cx =
          toSide === "right"
            ? snapToGrid(e1.x + pad)
            : snapToGrid(e1.x - pad);
        out = [
          ...out.slice(0, -1),
          { x: cx, y: e0.y },
          { x: cx, y: e1.y },
          e1,
        ];
      }
    }

    return toOrthogonal(out);
  }

  /** Raw anchors + stored waypoints (may still contain diagonals). */
  function edgeAnchorsAndWaypoints(edge: ErEdge): {
    start: ErPoint;
    end: ErPoint;
    wps: ErPoint[];
    fromSide: TableSide;
    toSide: TableSide;
  } | null {
    if (!diagram) return null;
    const a = diagram.nodes.find((n) => n.id === edge.from);
    const b = diagram.nodes.find((n) => n.id === edge.to);
    if (!a || !b) return null;
    const wps = [...(edge.waypoints ?? [])];
    // For auto layout (no custom path), pick sides from topology only
    const { fromSide, toSide } = pickLinkSides(a, b, wps);
    const start = columnSideAnchor(a, edge.fromColumnId, fromSide);
    const end = columnSideAnchor(b, edge.toColumnId, toSide);
    return { start, end, wps, fromSide, toSide };
  }

  /**
   * Full path for rendering — always orthogonal, H-exit / V-run / H-enter,
   * with parallel edges on separate lanes.
   */
  function edgePoints(edge: ErEdge): ErPoint[] {
    const raw = edgeAnchorsAndWaypoints(edge);
    if (!raw) return [];
    const lane = edgeLaneIndex(edge);
    let { start, end, wps, fromSide, toSide } = raw;

    // Auto route (empty or only trivial) — always rebuild with lane separation
    if (wps.length === 0) {
      wps = defaultManhattanWaypoints(start, end, fromSide, toSide, lane);
    } else if (wps.length <= 2) {
      // Likely auto-generated elbows: rebuild so parallel links don't share a spine
      wps = defaultManhattanWaypoints(start, end, fromSide, toSide, lane);
    } else {
      // User multi-bend path: keep shape but nudge by lane so overlaps ease
      if (lane > 0) {
        const sign =
          fromSide === "left" && toSide === "left"
            ? -1
            : fromSide === "right" || toSide === "right"
              ? 1
              : 1;
        const dx = sign * lane * ER_GRID;
        wps = wps.map((p) => ({ x: snapToGrid(p.x + dx), y: p.y }));
      }
    }

    const orth = toOrthogonal([start, ...wps, end]);
    return ensureHorizontalPorts(orth, fromSide, toSide, lane);
  }

  /**
   * Recompute stored intermediate corners as pure Manhattan waypoints
   * (drop endpoints). Used after drag/insert so handles sit on 90° corners.
   */
  function normalizeEdgeWaypoints(
    edge: ErEdge,
    wps: ErPoint[],
  ): ErPoint[] {
    if (!diagram) return wps;
    const a = diagram.nodes.find((n) => n.id === edge.from);
    const b = diagram.nodes.find((n) => n.id === edge.to);
    if (!a || !b) return wps;
    const { fromSide, toSide } = pickLinkSides(a, b, wps);
    const start = columnSideAnchor(a, edge.fromColumnId, fromSide);
    const end = columnSideAnchor(b, edge.toColumnId, toSide);
    const lane = edgeLaneIndex(edge);
    const full = ensureHorizontalPorts(
      toOrthogonal([
        start,
        ...wps.map((p) => ({ x: snapToGrid(p.x), y: snapToGrid(p.y) })),
        end,
      ]),
      fromSide,
      toSide,
      lane,
    );
    if (full.length <= 2) return [];
    return full.slice(1, -1);
  }

  function edgePolyline(edge: ErEdge): string {
    const pts = edgePoints(edge);
    if (!pts.length) return "";
    return pts.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x} ${p.y}`).join(" ");
  }

  function linkDraftStart(): ErPoint | null {
    if (!diagram || !linkFrom) return null;
    const a = diagram.nodes.find((n) => n.id === linkFrom!.tableId);
    if (!a) return null;
    const towardX = draftWaypoints[0]?.x
      ?? cursorPt?.x
      ?? a.x + TABLE_W + ER_GRID;
    return columnSideAnchor(a, linkFrom.columnId, sideToward(a, towardX));
  }

  function linkDraftLast(): ErPoint | null {
    if (draftWaypoints.length) return draftWaypoints[draftWaypoints.length - 1]!;
    return linkDraftStart();
  }

  function previewPolyline(): string {
    const from = linkFrom;
    if (!diagram || !from || !cursorPt) return "";
    const p0 = linkDraftStart();
    if (!p0) return "";
    const last = linkDraftLast() ?? p0;
    const tip = orthoGridSnapFrom(last, cursorPt);
    const pts = toOrthogonal([p0, ...draftWaypoints, tip]);
    return pts.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x} ${p.y}`).join(" ");
  }

  function segmentMids(edge: ErEdge): { x: number; y: number; insertAt: number }[] {
    const pts = edgePoints(edge);
    const mids: { x: number; y: number; insertAt: number }[] = [];
    // Only midpoints of real axis-aligned segments; insertAt indexes stored waypoints
    // insertAt = number of waypoints before this segment in the orth path middle
    for (let i = 0; i < pts.length - 1; i++) {
      const a = pts[i]!;
      const b = pts[i + 1]!;
      // skip zero-length
      if (samePoint(a, b)) continue;
      mids.push({
        x: (a.x + b.x) / 2,
        y: (a.y + b.y) / 2,
        // waypoint index to insert at: segment i in orth path → insert before orth[i+1]
        // relative to stored waypoints ≈ i (between start and first wp is 0)
        insertAt: Math.max(0, i),
      });
    }
    return mids;
  }

  function isBroken(kind: ErEdgeKind) {
    return kind === "broken" || kind === "inferred" || kind === "logical";
  }

  function addTable() {
    if (!diagram) {
      // Ensure we always have a diagram (effect may not have run yet)
      diagram = emptyDiagram(family);
    }
    if (arrowTool) {
      linkFrom = null;
      draftWaypoints = [];
      cursorPt = null;
    }
    ctxMenu = null;
    const current = diagram;
    const n = current.nodes.length;
    const kind: ErNode["kind"] =
      family === "sql" ? "table" : family === "nosql" ? "collection" : "key_pattern";
    const node: ErNode = {
      id: uid("n"),
      kind,
      name:
        family === "sql"
          ? `table_${n + 1}`
          : family === "nosql"
            ? `collection_${n + 1}`
            : `key:pattern_${n + 1}`,
      columns: [
        {
          id: uid("c"),
          name: family === "sql" ? "id" : family === "nosql" ? "_id" : "type",
          dataType:
            family === "sql" ? "UUID" : family === "nosql" ? "ObjectId" : "string",
          pk: family === "sql" || family === "nosql",
        },
      ],
      x: snapToGrid(ER_GRID * 4 + (n % 4) * ER_GRID * 2),
      y: snapToGrid(ER_GRID * 4 + (n % 4) * ER_GRID * 2),
    };
    snapshotBeforeChange();
    diagram = { ...current, nodes: [...current.nodes, node] };
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
      toolMsg = `From ${node?.name ?? "?"}.${col?.name ?? "?"}. Click target column (or same table for self-loop). Path is always 90° grid corridors — click empty grid for square bends.`;
      return;
    }

    // Same column as source → ignore / re-prompt
    if (linkFrom.tableId === tableId && linkFrom.columnId === columnId) {
      toolMsg = "Pick a different target column (self-loop needs two columns on this table).";
      return;
    }

    // Same table, different column → self-relationship; other table → normal FK
    completeLink(tableId, columnId);
  }

  /** Route a self-loop outside the table as a 90° U: out → vertical → in. */
  function autoSelfLoopWaypoints(
    node: ErNode,
    fromColumnId: string,
    toColumnId: string,
  ): ErPoint[] {
    const y0 = columnRowY(node, fromColumnId);
    const y1 = columnRowY(node, toColumnId);
    const loopX = snapToGrid(node.x + TABLE_W + ER_GRID * 3);
    return [
      { x: loopX, y: y0 },
      { x: loopX, y: y1 },
    ];
  }

  function completeLink(toTableId: string, toColumnId: string) {
    if (!diagram || !linkFrom || !arrowTool) return;

    // Cannot link a column to itself
    if (
      linkFrom.tableId === toTableId &&
      linkFrom.columnId === toColumnId
    ) {
      toolMsg = "Cannot link a column to itself — choose another column.";
      return;
    }

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
      // Still exit arrow mode so UI doesn't stay stuck
      arrowTool = null;
      return;
    }

    const fromNode = diagram.nodes.find((n) => n.id === linkFrom!.tableId);
    const toNode = diagram.nodes.find((n) => n.id === toTableId);
    const fromCol = fromNode?.columns.find((c) => c.id === linkFrom!.columnId);
    const toCol = toNode?.columns.find((c) => c.id === toColumnId);
    const isSelfLoop = linkFrom.tableId === toTableId;
    const label = `${fromNode?.name}.${fromCol?.name} → ${toNode?.name}.${toCol?.name}`;
    const kind: ErEdgeKind = arrowTool === "broken" ? "broken" : "normal";

    let wps = draftWaypoints.map((p) => ({
      x: snapToGrid(p.x),
      y: snapToGrid(p.y),
    }));

    // Lane among existing A→B links (this edge not in list yet)
    const lane = diagram.edges.filter(
      (e) => e.from === linkFrom!.tableId && e.to === toTableId,
    ).length;

    // Auto H-V-H route when user did not draw bends (arrow always enters L/R)
    if (wps.length === 0 && fromNode && toNode) {
      const { fromSide, toSide } = pickLinkSides(fromNode, toNode, []);
      const start = columnSideAnchor(fromNode, linkFrom.columnId, fromSide);
      const end = columnSideAnchor(toNode, toColumnId, toSide);
      if (isSelfLoop) {
        wps = autoSelfLoopWaypoints(fromNode, linkFrom.columnId, toColumnId).map(
          (p) => ({
            x: snapToGrid(p.x + (fromSide === "right" ? lane : -lane) * ER_GRID),
            y: p.y,
          }),
        );
      } else {
        wps = defaultManhattanWaypoints(start, end, fromSide, toSide, lane);
      }
    }

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
    toolMsg = isSelfLoop
      ? `Self-loop: ${label}. 90° path. Arrow tool cleared.`
      : `Linked: ${label}. 90° grid path. Arrow tool cleared.`;

    // Clear linking state AND deselect Normal/Broken tool
    linkFrom = null;
    draftWaypoints = [];
    cursorPt = null;
    arrowTool = null;
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
      const last = linkDraftLast();
      // Only horizontal/vertical steps on the grid (no free diagonal traces)
      const snapped = last
        ? orthoGridSnapFrom(last, pt)
        : { x: snapToGrid(pt.x), y: snapToGrid(pt.y) };
      if (last && samePoint(last, snapped)) return;
      draftWaypoints = [...draftWaypoints, snapped];
      toolMsg = `Square bend ${draftWaypoints.length} (90°). Click more grid points or the target column.`;
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
      const { edgeId, index } = dragWaypoint;
      if (!dragHistoryPushed) {
        snapshotBeforeChange();
        dragHistoryPushed = true;
      }
      let nextIndex = index;
      const nextEdges = diagram.edges.map((ed) => {
        if (ed.id !== edgeId) return ed;
        const wps = [...(ed.waypoints ?? [])];
        if (index < 0 || index >= wps.length) return ed;
        // Free move on grid, then re-force 90° corridors
        wps[index] = { x: snapToGrid(pt.x), y: snapToGrid(pt.y) };
        const normalized = normalizeEdgeWaypoints(ed, wps);
        nextIndex = Math.min(index, Math.max(0, normalized.length - 1));
        return { ...ed, waypoints: normalized };
      });
      diagram = { ...diagram, edges: nextEdges };
      if (nextIndex !== index) {
        dragWaypoint = { edgeId, index: nextIndex };
      }
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
    // Capture plain array before any state changes
    const idList = ids.filter(Boolean);
    if (!idList.length) return;
    snapshotBeforeChange();
    const nextNodes = diagram.nodes.filter((n) => !set.has(n.id));
    const nextEdges = diagram.edges.filter(
      (e) => !set.has(e.from) && !set.has(e.to),
    );
    diagram = {
      ...diagram,
      nodes: nextNodes,
      edges: nextEdges,
    };
    selectedTableIds = selectedTableIds.filter((id) => !set.has(id));
    selectedEdgeId = null;
    showAddCol = false;
    ctxMenu = null;
    toolMsg =
      idList.length === 1 ? "Table deleted." : `Deleted ${idList.length} tables.`;
    markDirty();
  }

  function deleteSelection() {
    // Snapshot selection first — menu close / focus changes must not clear it mid-action
    const tableIds = [...selectedTableIds];
    const edgeId = selectedEdgeId;
    if (tableIds.length > 0) {
      deleteTablesByIds(tableIds);
      return;
    }
    if (edgeId) {
      deleteEdge(edgeId);
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
    const mod = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();
    const isDeleteKey =
      e.key === "Delete" ||
      e.key === "Backspace" ||
      e.code === "Delete" ||
      e.code === "Backspace";

    // Allow Delete even when focus is in a non-text control; block only real text fields
    // (except we still want Delete to remove tables when canvas has a selection).
    if (isEditableTarget(e.target) && !isDeleteKey) {
      return;
    }
    // In text fields, let Backspace edit text; only trap Delete if empty selection of tables
    if (isEditableTarget(e.target) && isDeleteKey) {
      // Don't steal Backspace from inputs
      if (e.key === "Backspace" || e.code === "Backspace") return;
    }

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
      if (isEditableTarget(e.target)) return;
      e.preventDefault();
      if (!arrowTool) copySelectedTables();
      return;
    }
    if (mod && key === "v") {
      if (isEditableTarget(e.target)) return;
      e.preventDefault();
      if (!arrowTool) pasteTables();
      return;
    }
    if (isDeleteKey) {
      if (selectedTableIds.length || selectedEdgeId) {
        e.preventDefault();
        e.stopPropagation();
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
    // insertAt is segment index along the orthogonal display path
    const pts = edgePoints(edge);
    if (insertAt < 0 || insertAt >= pts.length - 1) return;
    const a = pts[insertAt]!;
    const b = pts[insertAt + 1]!;
    const mid = {
      x: snapToGrid((a.x + b.x) / 2),
      y: snapToGrid((a.y + b.y) / 2),
    };
    // Seed waypoints from the full orth path so synthetic elbows become editable
    const seeded = [...pts.slice(0, insertAt + 1), mid, ...pts.slice(insertAt + 1)];
    const wps = normalizeEdgeWaypoints(edge, seeded.slice(1, -1));
    const dragIndex = Math.min(insertAt, Math.max(0, wps.length - 1));
    snapshotBeforeChange();
    dragHistoryPushed = true;
    diagram = {
      ...diagram,
      edges: diagram.edges.map((ed) =>
        ed.id === edgeId ? { ...ed, waypoints: wps } : ed,
      ),
    };
    selectedEdgeId = edgeId;
    selectedTableIds = [];
    dragWaypoint = { edgeId, index: dragIndex };
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
        return { ...ed, waypoints: normalizeEdgeWaypoints(ed, wps) };
      }),
    };
    toolMsg = "Bend point removed (path stays 90°).";
    markDirty();
  }

  function straightenSelectedEdge() {
    if (!diagram || !selectedEdgeId) return;
    snapshotBeforeChange();
    diagram = {
      ...diagram,
      edges: diagram.edges.map((ed) => {
        if (ed.id !== selectedEdgeId) return ed;
        // Reset to default H-V-H corridor (separate lane for parallel links)
        const raw = edgeAnchorsAndWaypoints({ ...ed, waypoints: [] });
        if (!raw) return { ...ed, waypoints: [] };
        const lane = edgeLaneIndex(ed);
        const wps = defaultManhattanWaypoints(
          raw.start,
          raw.end,
          raw.fromSide,
          raw.toSide,
          lane,
        );
        return { ...ed, waypoints: wps };
      }),
    };
    toolMsg = "Path reset to default 90° corridor.";
    markDirty();
  }

  function toggleGrid() {
    showGrid = !showGrid;
    toolMsg = showGrid ? "Grid shown." : "Grid hidden.";
  }

  function toggleDbml() {
    // Right export panel — not available while left source editor is open
    if (dbmlSourceMode) {
      toolMsg = "Close left DBML editor first to show the right export panel.";
      return;
    }
    showDbml = !showDbml;
    toolMsg = showDbml ? "DBML export panel shown." : "DBML export panel hidden.";
  }

  function openDbmlSourceEditor() {
    dbmlSourceMode = true;
    showDbml = false; // auto-hide right panel
    dbmlParseError = null;
    // Seed editor from canvas, or sample if empty-ish
    const fromCanvas = diagramToDbml(diagram, diagramName);
    const hasTables = (diagram?.nodes.length ?? 0) > 0;
    dbmlEditorText = hasTables ? fromCanvas : sampleDbml(family);
    toolMsg =
      "DBML editor open — edit text, then Apply to rebuild the diagram. Right export panel hidden.";
  }

  function closeDbmlSourceEditor() {
    dbmlSourceMode = false;
    dbmlParseError = null;
    toolMsg = "DBML editor closed. Use tools to edit the canvas, or Show DBML for export view.";
  }

  function toggleDbmlSourceEditor() {
    if (dbmlSourceMode) closeDbmlSourceEditor();
    else openDbmlSourceEditor();
  }

  function pullCanvasIntoEditor() {
    dbmlEditorText = diagramToDbml(diagram, diagramName);
    dbmlParseError = null;
    toolMsg = "Editor loaded from current canvas.";
  }

  function loadSampleDbml() {
    dbmlEditorText = sampleDbml(family);
    dbmlParseError = null;
    toolMsg = "Sample DBML loaded — click Apply to generate the diagram.";
  }

  function applyDbmlToCanvas() {
    const result = dbmlToDiagram(dbmlEditorText, family, diagram);
    if (!result.ok) {
      dbmlParseError = result.error;
      toolMsg = result.error;
      return;
    }
    snapshotBeforeChange();
    diagram = result.diagram;
    selectedTableIds = [];
    selectedEdgeId = null;
    arrowTool = null;
    linkFrom = null;
    draftWaypoints = [];
    cursorPt = null;
    showAddCol = false;
    dbmlParseError = null;
    markDirty();
    toolMsg = `Diagram generated from DBML (${result.diagram.nodes.length} tables, ${result.diagram.edges.length} refs).`;
  }

  async function copyDbml() {
    const text = dbmlSourceMode ? dbmlEditorText : dbmlText;
    try {
      await navigator.clipboard.writeText(text);
      dbmlCopied = true;
      toolMsg = "DBML copied to clipboard.";
      setTimeout(() => {
        dbmlCopied = false;
      }, 1600);
    } catch {
      toolMsg = "Could not copy DBML — select text manually.";
    }
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
      <Button
        variant={dbmlSourceMode ? "secondary" : "ghost"}
        size="sm"
        onclick={toggleDbmlSourceEditor}
        aria-pressed={dbmlSourceMode}
      >
        {dbmlSourceMode ? "Close DBML editor" : "DBML editor"}
      </Button>
      {#if !dbmlSourceMode}
        <Button
          variant={showDbml ? "secondary" : "ghost"}
          size="sm"
          onclick={toggleDbml}
          aria-pressed={showDbml}
        >
          {showDbml ? "Hide export" : "Show export"}
        </Button>
      {/if}
      <Button variant="secondary" size="sm" onclick={handleLoadLatest}>Load last</Button>
      <Button variant="primary" size="sm" onclick={handleSave} disabled={saving}>
        {saving ? "Saving…" : "Save"}
      </Button>
      <Button variant="ghost" size="sm" onclick={onchangeFamily}>Change family</Button>
    </Inline>
  </div>

  <div class="flex min-h-0 flex-1">
    <aside
      class={cn(
        "flex shrink-0 flex-col border-r border-line bg-panel",
        dbmlSourceMode ? "w-[min(22rem,38vw)]" : "w-56",
      )}
    >
      {#if dbmlSourceMode}
        <!-- Left DBML source editor (text → diagram); right export is hidden -->
        <div
          class="flex h-control shrink-0 items-center justify-between gap-2 border-b border-line px-3"
        >
          <div class="min-w-0">
            <p class="text-label font-semibold tracking-wider text-fg-muted uppercase">
              DBML editor
            </p>
            <p class="truncate text-caption text-fg-muted">Text → diagram</p>
          </div>
          <Button variant="ghost" size="sm" onclick={closeDbmlSourceEditor}>Close</Button>
        </div>
        <div class="flex min-h-0 flex-1 flex-col gap-2 p-3">
          <div class="flex flex-wrap gap-1.5">
            <Button variant="primary" size="sm" onclick={applyDbmlToCanvas}>
              Apply to canvas
            </Button>
            <Button variant="secondary" size="sm" onclick={pullCanvasIntoEditor}>
              From canvas
            </Button>
            <Button variant="ghost" size="sm" onclick={loadSampleDbml}>Sample</Button>
            <Button variant="ghost" size="sm" onclick={copyDbml}>
              {dbmlCopied ? "Copied" : "Copy"}
            </Button>
          </div>
          <textarea
            class="min-h-0 w-full flex-1 resize-none rounded-md border border-line bg-input p-2.5 font-mono text-caption leading-relaxed text-fg placeholder:text-fg-muted focus:border-focus focus:outline-none focus:ring-2 focus:ring-accent-muted"
            bind:value={dbmlEditorText}
            spellcheck="false"
            aria-label="DBML source"
            placeholder="Table users &#123;&#10;  id integer [primary key]&#10;&#125;"
          ></textarea>
          {#if dbmlParseError}
            <p class="rounded-md border border-danger/40 bg-danger/10 px-2 py-1.5 text-caption text-danger" role="alert">
              {dbmlParseError}
            </p>
          {/if}
          <p class="text-caption text-fg-muted">
            Write DBML, then <strong class="text-fg-secondary">Apply</strong>. Right export panel stays hidden in this mode.
            <a
              class="text-accent hover:underline"
              href="https://dbml.dbdiagram.io/docs"
              target="_blank"
              rel="noreferrer">Docs</a
            >
          </p>
        </div>
      {:else}
        <div class="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto p-3">
          <p class="text-label font-semibold tracking-wider text-fg-muted uppercase">Tools</p>
          <Button
            variant="primary"
            fullWidth
            onclick={(e: MouseEvent) => {
              e.preventDefault();
              e.stopPropagation();
              addTable();
            }}
          >
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

          <button
            type="button"
            class={cn(
              "flex w-full items-center justify-between rounded-lg border px-3 py-2 text-left text-caption font-medium transition-colors",
              "border-line bg-input text-fg-secondary hover:border-line-strong",
            )}
            onclick={openDbmlSourceEditor}
          >
            <span>DBML editor</span>
            <span class="font-mono text-micro uppercase tracking-wide">Text</span>
          </button>

          <button
            type="button"
            class={cn(
              "flex w-full items-center justify-between rounded-lg border px-3 py-2 text-left text-caption font-medium transition-colors",
              showDbml
                ? "border-accent bg-accent-muted text-fg"
                : "border-line bg-input text-fg-secondary hover:border-line-strong",
            )}
            onclick={toggleDbml}
            aria-pressed={showDbml}
          >
            <span>DBML export</span>
            <span class="font-mono text-micro uppercase tracking-wide">
              {showDbml ? "ON" : "OFF"}
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
            <li>DBML editor → write text → Apply</li>
            <li>Or draw tables / 90° links on canvas</li>
            <li>Export panel shows live DBML (right)</li>
            <li>Ctrl+Z undo · Ctrl+A select all</li>
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
            <!-- Destination: solid ▶ chevron (source → destination) -->
            <marker
              id="er-arrow-head"
              viewBox="0 0 14 14"
              markerWidth="14"
              markerHeight="14"
              refX="12"
              refY="7"
              orient="auto"
              markerUnits="userSpaceOnUse"
            >
              <path d="M1,1 L13,7 L1,13 L4,7 Z" fill="#5b8def" stroke="#5b8def" stroke-width="0.5" />
            </marker>
            <marker
              id="er-arrow-head-sel"
              viewBox="0 0 16 16"
              markerWidth="16"
              markerHeight="16"
              refX="14"
              refY="8"
              orient="auto"
              markerUnits="userSpaceOnUse"
            >
              <path d="M1,1 L15,8 L1,15 L4.5,8 Z" fill="#5b8def" stroke="#8eb6ff" stroke-width="1" />
            </marker>
            <!-- Source: small filled circle so origin is obvious -->
            <marker
              id="er-arrow-tail"
              viewBox="0 0 10 10"
              markerWidth="8"
              markerHeight="8"
              refX="5"
              refY="5"
              orient="auto"
              markerUnits="userSpaceOnUse"
            >
              <circle cx="5" cy="5" r="3.5" fill="#5b8def" stroke="#8eb6ff" stroke-width="1" />
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
              stroke="#5b8def"
              stroke-width={selected ? 3 : 2.25}
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-dasharray={isBroken(edge.kind) ? "8 5" : "0"}
              marker-start="url(#er-arrow-tail)"
              marker-end={selected ? "url(#er-arrow-head-sel)" : "url(#er-arrow-head)"}
              opacity="0.98"
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
              stroke="#5b8def"
              stroke-width="2.25"
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-dasharray="6 4"
              opacity="0.85"
              marker-start="url(#er-arrow-tail)"
              marker-end="url(#er-arrow-head)"
              style="pointer-events: none;"
            />
            {#each draftWaypoints as wp, i}
              <circle cx={wp.x} cy={wp.y} r="4" fill="#5b8def" style="pointer-events: none;" />
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
              // Grab only on header — body uses default so inputs get a normal text caret
              arrowTool ? "cursor-default" : "",
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
              class={cn(
                "flex items-center justify-between gap-2 border-b border-line bg-elevated px-2.5",
                !arrowTool && "cursor-grab",
                dragTableId === node.id && "cursor-grabbing",
              )}
              style="height: {HEADER_H}px"
            >
              {#if primaryTableId === node.id && !arrowTool}
                <input
                  class="min-w-0 flex-1 cursor-text rounded-md border border-line bg-input px-2 py-1 text-body font-semibold text-fg"
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
                      class="flex h-full w-full items-center gap-1.5 border-b border-line/50 px-2 last:border-b-0"
                    >
                      {#if col.pk}
                        <span class="shrink-0 text-micro font-semibold text-accent">PK</span>
                      {/if}
                      <span class="min-w-0 flex-1 truncate font-mono text-caption text-fg">
                        {col.name}
                      </span>
                      <div
                        class="w-28 shrink-0 cursor-pointer [&_button]:h-7 [&_button]:px-2 [&_button]:text-caption [&_svg]:size-3"
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
                        class="flex size-7 shrink-0 cursor-pointer items-center justify-center rounded text-body text-danger hover:bg-danger/10"
                        onclick={(e) => {
                          e.stopPropagation();
                          deleteColumn(col.id);
                        }}
                        aria-label="Delete column"
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
                        "flex h-full w-full cursor-default items-center gap-1.5 border-b border-line/50 px-2.5 text-left last:border-b-0",
                        arrowTool && "cursor-pointer hover:bg-accent-muted",
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
              <div class="cursor-default border-t border-line p-2.5" data-no-drag>
                {#if !showAddCol}
                  <button
                    type="button"
                    class="h-control-sm w-full cursor-pointer rounded-md border border-dashed border-line text-caption font-medium text-accent hover:bg-accent-muted"
                    onclick={(e) => {
                      e.stopPropagation();
                      newColType = defaultColumnType(family);
                      showAddCol = true;
                    }}
                  >
                    + Add column
                  </button>
                {:else}
                  <div class="flex flex-col gap-2">
                    <Input
                      bind:value={newColName}
                      placeholder="column_name"
                      inputSize="md"
                      class="cursor-text font-mono text-body"
                    />
                    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
                    <div
                      class="cursor-pointer"
                      role="presentation"
                      onclick={(e) => e.stopPropagation()}
                      onpointerdown={(e) => e.stopPropagation()}
                    >
                      <Select
                        bind:value={newColType}
                        options={typeOptions}
                        inputSize="md"
                        placeholder="Column type"
                      />
                    </div>
                    <label
                      class="flex cursor-pointer items-center gap-2 text-body text-fg-secondary"
                    >
                      <input
                        type="checkbox"
                        class="size-3.5 shrink-0 cursor-pointer accent-[var(--accent)]"
                        bind:checked={newColPk}
                      />
                      Primary key
                    </label>
                    <div class="flex gap-2 pt-0.5">
                      <Button
                        size="md"
                        variant="primary"
                        onclick={(e: MouseEvent) => {
                          e.stopPropagation();
                          addColumn();
                        }}
                      >
                        Add
                      </Button>
                      <Button
                        size="md"
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

    <!-- Right DBML export (hidden automatically when left DBML editor is open) -->
    {#if showRightDbml}
      <aside
        class="flex w-[min(22rem,36vw)] shrink-0 flex-col border-l border-line bg-panel"
        aria-label="DBML text view"
      >
        <div
          class="flex h-control shrink-0 items-center justify-between gap-2 border-b border-line px-3"
        >
          <div class="min-w-0">
            <p class="text-label font-semibold tracking-wider text-fg-muted uppercase">
              DBML
            </p>
            <p class="truncate text-caption text-fg-muted">Live from canvas</p>
          </div>
          <Inline gap="sm">
            <Button variant="ghost" size="sm" onclick={copyDbml}>
              {dbmlCopied ? "Copied" : "Copy"}
            </Button>
            <Button variant="ghost" size="sm" onclick={toggleDbml} aria-label="Hide DBML">
              Hide
            </Button>
          </Inline>
        </div>
        <div class="min-h-0 flex-1 overflow-auto bg-input/40 p-3">
          <pre
            class="m-0 whitespace-pre-wrap break-words font-mono text-caption leading-relaxed text-fg"
            >{dbmlText}</pre
          >
        </div>
        <p class="shrink-0 border-t border-line px-3 py-2 text-caption text-fg-muted">
          Updates as you edit tables, columns, and links. Format:
          <a
            class="text-accent hover:underline"
            href="https://dbml.dbdiagram.io/docs"
            target="_blank"
            rel="noreferrer"
          >DBML docs</a
          >.
        </p>
      </aside>
    {/if}
  </div>
</div>

<!-- Context menu: table or canvas -->
{#if ctxMenu}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-40"
    role="presentation"
    onmousedown={(e) => {
      e.preventDefault();
      ctxMenu = null;
    }}
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
        onmousedown={(e) => {
          e.preventDefault();
          e.stopPropagation();
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
        onmousedown={(e) => {
          e.preventDefault();
          e.stopPropagation();
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
          onmousedown={(e) => {
            e.preventDefault();
            e.stopPropagation();
            const ids = [...selectedTableIds];
            ctxMenu = null;
            deleteTablesByIds(ids);
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
        onmousedown={(e) => {
          e.preventDefault();
          e.stopPropagation();
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
        onmousedown={(e) => {
          e.preventDefault();
          e.stopPropagation();
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
        onmousedown={(e) => {
          e.preventDefault();
          e.stopPropagation();
          // Capture selection before menu tears down
          const ids =
            selectedTableIds.length > 0
              ? [...selectedTableIds]
              : ctxMenu?.tableId
                ? [ctxMenu.tableId]
                : [];
          ctxMenu = null;
          if (ids.length) deleteTablesByIds(ids);
          else deleteSelection();
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
