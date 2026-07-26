<script lang="ts">
  import type {
    DataFamily,
    ErColumn,
    ErDiagramState,
    ErEdge,
    ErEdgeKind,
    ErNode,
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

  const TABLE_W = 240;
  const HEADER_H = 40;
  const ROW_H = 28;

  let diagram = $state<ErDiagramState | null>(null);
  let diagramId = $state<string | null>(null);
  let diagramName = $state("Untitled diagram");
  let selectedTableId = $state<string | null>(null);
  let toolMsg = $state<string | null>(null);
  let saving = $state(false);
  let dirty = $state(false);

  /** null = move/select · normal | broken = draw column→column */
  let arrowTool = $state<"normal" | "broken" | null>(null);
  /** Pending link start: table + column */
  let linkFrom = $state<{ tableId: string; columnId: string } | null>(null);

  // table drag
  let dragTableId = $state<string | null>(null);
  let dragOffX = 0;
  let dragOffY = 0;
  let canvasEl: HTMLDivElement | null = $state(null);

  // rubber-band while linking
  let rubber = $state<{ x1: number; y1: number; x2: number; y2: number } | null>(
    null,
  );

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
    arrowTool = null;
    linkFrom = null;
    rubber = null;
    showAddCol = false;
    dirty = false;
    toolMsg = null;
  });

  function uid(p: string) {
    return `${p}_${Math.random().toString(36).slice(2, 9)}`;
  }

  function markDirty() {
    dirty = true;
  }

  function addTable() {
    if (!diagram) return;
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
      x: snapToGrid(ER_GRID * 3 + (n % 5) * ER_GRID),
      y: snapToGrid(ER_GRID * 3 + (n % 5) * ER_GRID),
    };
    diagram = { ...diagram, nodes: [...diagram.nodes, node] };
    selectedTableId = node.id;
    showAddCol = false;
    toolMsg = `Added “${node.name}”. Drag on the grid. Use arrow tools on columns to link.`;
    markDirty();
  }

  function setArrowTool(kind: "normal" | "broken" | null) {
    arrowTool = kind;
    linkFrom = null;
    rubber = null;
    if (kind === "normal") {
      toolMsg = "Normal arrow: click a column, then the target column on another table.";
    } else if (kind === "broken") {
      toolMsg = "Broken arrow: click source column, then target column (dashed link).";
    } else {
      toolMsg = "Move mode: drag tables on the grid.";
    }
  }

  function columnAnchor(
    node: ErNode,
    columnId: string,
    side: "left" | "right",
  ): { x: number; y: number } {
    const idx = Math.max(
      0,
      node.columns.findIndex((c) => c.id === columnId),
    );
    const y = node.y + HEADER_H + idx * ROW_H + ROW_H / 2;
    const x = side === "right" ? node.x + TABLE_W : node.x;
    return { x, y };
  }

  function edgePath(edge: ErEdge): string {
    if (!diagram) return "";
    const a = diagram.nodes.find((n) => n.id === edge.from);
    const b = diagram.nodes.find((n) => n.id === edge.to);
    if (!a || !b) return "";
    // Leave source from right, enter target from left
    const p1 = columnAnchor(a, edge.fromColumnId, "right");
    const p2 = columnAnchor(b, edge.toColumnId, "left");
    const dx = Math.max(40, Math.abs(p2.x - p1.x) * 0.4);
    return `M ${p1.x} ${p1.y} C ${p1.x + dx} ${p1.y}, ${p2.x - dx} ${p2.y}, ${p2.x} ${p2.y}`;
  }

  function onColumnPointerDown(
    tableId: string,
    columnId: string,
    e: PointerEvent,
  ) {
    e.stopPropagation();
    if (!arrowTool || !canvasEl) return;
    e.preventDefault();
    linkFrom = { tableId, columnId };
    const node = diagram?.nodes.find((n) => n.id === tableId);
    if (!node) return;
    const p = columnAnchor(node, columnId, "right");
    rubber = { x1: p.x, y1: p.y, x2: p.x, y2: p.y };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onColumnPointerMove(e: PointerEvent) {
    if (!linkFrom || !canvasEl || !rubber) return;
    const rect = canvasEl.getBoundingClientRect();
    rubber = {
      ...rubber,
      x2: e.clientX - rect.left + canvasEl.scrollLeft,
      y2: e.clientY - rect.top + canvasEl.scrollTop,
    };
  }

  function completeLink(toTableId: string, toColumnId: string) {
    if (!diagram || !linkFrom || !arrowTool) return;
    if (linkFrom.tableId === toTableId) {
      toolMsg = "Pick a column on a different table.";
      linkFrom = null;
      rubber = null;
      return;
    }
    // avoid duplicate
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
      rubber = null;
      return;
    }
    const fromNode = diagram.nodes.find((n) => n.id === linkFrom!.tableId);
    const toNode = diagram.nodes.find((n) => n.id === toTableId);
    const fromCol = fromNode?.columns.find((c) => c.id === linkFrom!.columnId);
    const toCol = toNode?.columns.find((c) => c.id === toColumnId);
    const label = `${fromNode?.name}.${fromCol?.name} → ${toNode?.name}.${toCol?.name}`;
    const kind: ErEdgeKind = arrowTool === "broken" ? "broken" : "normal";
    const edge: ErEdge = {
      id: uid("e"),
      from: linkFrom.tableId,
      to: toTableId,
      fromColumnId: linkFrom.columnId,
      toColumnId: toColumnId,
      kind,
      label,
    };
    diagram = { ...diagram, edges: [...diagram.edges, edge] };
    toolMsg = `${kind === "broken" ? "Broken" : "Normal"} link: ${label}`;
    linkFrom = null;
    rubber = null;
    markDirty();
  }

  function onColumnPointerUp(toTableId: string, toColumnId: string, e: PointerEvent) {
    e.stopPropagation();
    if (!linkFrom || !arrowTool) return;
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }
    if (
      linkFrom.tableId === toTableId &&
      linkFrom.columnId === toColumnId
    ) {
      // click without drag — keep start, wait for second column click
      rubber = null;
      toolMsg = "Now click the target column on another table.";
      return;
    }
    completeLink(toTableId, toColumnId);
  }

  function onColumnClick(tableId: string, columnId: string, e: MouseEvent) {
    e.stopPropagation();
    if (!arrowTool) {
      selectedTableId = tableId;
      return;
    }
    if (!linkFrom) {
      linkFrom = { tableId, columnId };
      toolMsg = "Now click the target column.";
      return;
    }
    completeLink(tableId, columnId);
  }

  function onTablePointerDown(tableId: string, e: PointerEvent) {
    if (arrowTool) return; // don't drag tables while drawing relations
    if ((e.target as HTMLElement).closest("[data-col-handle]")) return;
    if ((e.target as HTMLElement).closest("[data-no-drag]")) return;
    if (!diagram || !canvasEl) return;
    const node = diagram.nodes.find((n) => n.id === tableId);
    if (!node) return;
    const rect = canvasEl.getBoundingClientRect();
    dragTableId = tableId;
    dragOffX = e.clientX - rect.left + canvasEl.scrollLeft - node.x;
    dragOffY = e.clientY - rect.top + canvasEl.scrollTop - node.y;
    selectedTableId = tableId;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    e.preventDefault();
  }

  function onTablePointerMove(e: PointerEvent) {
    if (linkFrom && arrowTool && canvasEl && rubber) {
      onColumnPointerMove(e);
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
    diagram = {
      ...diagram,
      nodes: diagram.nodes.map((n) =>
        n.id === dragTableId ? { ...n, x, y } : n,
      ),
    };
  }

  function onTablePointerUp(e: PointerEvent) {
    if (dragTableId) {
      markDirty();
      try {
        (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
      } catch {
        /* ignore */
      }
    }
    dragTableId = null;
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

  function deleteSelectedTable() {
    if (!diagram || !selectedTableId) return;
    const id = selectedTableId;
    diagram = {
      ...diagram,
      nodes: diagram.nodes.filter((n) => n.id !== id),
      edges: diagram.edges.filter((e) => e.from !== id && e.to !== id),
    };
    selectedTableId = null;
    showAddCol = false;
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
    markDirty();
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
      // migrate old edges without column ids if needed
      const body = rec.body;
      diagram = body;
      diagramId = rec.id;
      diagramName = rec.name;
      dirty = false;
      toolMsg = `Loaded “${rec.name}”.`;
    } catch (e) {
      toolMsg = e instanceof Error ? e.message : String(e);
    }
  }

  function edgeKindIsBroken(kind: ErEdgeKind) {
    return kind === "broken" || kind === "inferred" || kind === "logical";
  }
</script>

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
    </div>
    <Inline gap="sm">
      <Input bind:value={diagramName} class="w-44" inputSize="sm" aria-label="Diagram name" />
      <Button variant="secondary" size="sm" onclick={handleLoadLatest}>Load last</Button>
      <Button variant="primary" size="sm" onclick={handleSave} disabled={saving}>
        {saving ? "Saving…" : "Save"}
      </Button>
      <Button variant="ghost" size="sm" onclick={onchangeFamily}>Change family</Button>
    </Inline>
  </div>

  <div class="flex min-h-0 flex-1">
    <!-- Tools -->
    <aside class="flex w-56 shrink-0 flex-col gap-2 border-r border-line bg-panel p-3">
      <p class="text-label font-semibold tracking-wider text-fg-muted uppercase">Tools</p>

      <Button variant="primary" fullWidth onclick={addTable}>
        + Add {meta.nodeLabel.toLowerCase()}
      </Button>

      <p class="mt-2 text-label font-semibold tracking-wider text-fg-muted uppercase">
        Arrows (column → column)
      </p>
      <div class="grid grid-cols-2 gap-2">
        <button
          type="button"
          class={cn(
            "flex flex-col items-center gap-1.5 rounded-lg border px-2 py-3 transition-colors",
            arrowTool === "normal"
              ? "border-accent bg-accent-muted"
              : "border-line bg-input hover:border-line-strong",
          )}
          onclick={() => setArrowTool(arrowTool === "normal" ? null : "normal")}
          title="Solid FK-style link between two columns"
        >
          <svg width="40" height="16" aria-hidden="true">
            <defs>
              <marker id="m-n" markerWidth="6" markerHeight="6" refX="5" refY="3" orient="auto">
                <path d="M0,0 L6,3 L0,6 Z" fill="currentColor" class="text-fg" />
              </marker>
            </defs>
            <line
              x1="2"
              y1="8"
              x2="32"
              y2="8"
              stroke="currentColor"
              stroke-width="2"
              marker-end="url(#m-n)"
              class="text-fg"
            />
          </svg>
          <span class="text-caption font-medium text-fg">Normal</span>
        </button>
        <button
          type="button"
          class={cn(
            "flex flex-col items-center gap-1.5 rounded-lg border px-2 py-3 transition-colors",
            arrowTool === "broken"
              ? "border-accent bg-accent-muted"
              : "border-line bg-input hover:border-line-strong",
          )}
          onclick={() => setArrowTool(arrowTool === "broken" ? null : "broken")}
          title="Dashed / optional link between two columns"
        >
          <svg width="40" height="16" aria-hidden="true">
            <defs>
              <marker id="m-b" markerWidth="6" markerHeight="6" refX="5" refY="3" orient="auto">
                <path d="M0,0 L6,3 L0,6 Z" fill="currentColor" class="text-fg" />
              </marker>
            </defs>
            <line
              x1="2"
              y1="8"
              x2="32"
              y2="8"
              stroke="currentColor"
              stroke-width="2"
              stroke-dasharray="4 3"
              marker-end="url(#m-b)"
              class="text-fg"
            />
          </svg>
          <span class="text-caption font-medium text-fg">Broken</span>
        </button>
      </div>
      <p class="text-caption text-fg-muted">
        Select an arrow type, then click a <strong class="font-medium text-fg-secondary">column</strong>
        on table A and a <strong class="font-medium text-fg-secondary">column</strong> on table B.
        Not whole tables.
      </p>

      {#if selectedTableId}
        <Button variant="danger-ghost" fullWidth onclick={deleteSelectedTable}>
          Delete selected table
        </Button>
      {/if}

      {#if toolMsg}
        <p class="text-caption text-fg-secondary" role="status">{toolMsg}</p>
      {/if}

      {#if diagram && diagram.edges.length > 0}
        <div class="mt-1 border-t border-line pt-2">
          <p class="mb-1 text-label font-semibold text-fg-muted uppercase">Links</p>
          <ul class="m-0 max-h-40 list-none space-y-1 overflow-auto p-0">
            {#each diagram.edges as edge (edge.id)}
              <li
                class="flex items-start justify-between gap-1 rounded-md border border-line bg-input px-2 py-1 text-caption"
              >
                <span class="min-w-0 truncate text-fg-secondary" title={edge.label}>
                  {edge.kind === "broken" || edgeKindIsBroken(edge.kind) ? "┄ " : "─ "}
                  {edge.label ?? edge.id}
                </span>
                <button
                  type="button"
                  class="shrink-0 border-none bg-transparent p-0 text-danger hover:underline"
                  onclick={() => deleteEdge(edge.id)}
                >
                  ×
                </button>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
    </aside>

    <!-- Grid canvas -->
    <div
      class="relative min-w-0 flex-1 overflow-auto"
      style="
        background-color: var(--bg-app);
        background-image:
          linear-gradient(to right, var(--border-subtle) 1px, transparent 1px),
          linear-gradient(to bottom, var(--border-subtle) 1px, transparent 1px);
        background-size: {ER_GRID}px {ER_GRID}px;
      "
      bind:this={canvasEl}
      role="application"
      aria-label="ER grid canvas"
      onpointermove={onTablePointerMove}
    >
      {#if diagram}
        <svg
          class="pointer-events-none absolute inset-0 h-full min-h-[720px] w-full min-w-[1200px]"
          aria-hidden="true"
        >
          <defs>
            <marker
              id="er-arrow"
              markerWidth="8"
              markerHeight="8"
              refX="7"
              refY="3"
              orient="auto"
            >
              <path d="M0,0 L8,3 L0,6 Z" fill="var(--accent)" />
            </marker>
          </defs>
          {#each diagram.edges as edge (edge.id)}
            <path
              d={edgePath(edge)}
              fill="none"
              stroke="var(--accent)"
              stroke-width="2"
              stroke-dasharray={edgeKindIsBroken(edge.kind) ? "6 4" : "0"}
              marker-end="url(#er-arrow)"
            />
          {/each}
          {#if rubber}
            <line
              x1={rubber.x1}
              y1={rubber.y1}
              x2={rubber.x2}
              y2={rubber.y2}
              stroke="var(--accent)"
              stroke-width="1.5"
              stroke-dasharray="4 3"
              opacity="0.8"
            />
          {/if}
        </svg>

        {#each diagram.nodes as node (node.id)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class={cn(
              "absolute rounded-lg border bg-panel shadow-sm",
              selectedTableId === node.id
                ? "border-accent ring-2 ring-accent/30"
                : "border-line",
              arrowTool ? "cursor-default" : "cursor-grab",
              dragTableId === node.id && "cursor-grabbing",
            )}
            style="left: {node.x}px; top: {node.y}px; width: {TABLE_W}px;"
            onpointerdown={(e) => onTablePointerDown(node.id, e)}
            onpointermove={onTablePointerMove}
            onpointerup={onTablePointerUp}
            onpointercancel={onTablePointerUp}
            onclick={() => {
              if (!arrowTool) selectedTableId = node.id;
            }}
          >
            <!-- header -->
            <div
              class="flex h-10 items-center justify-between gap-2 border-b border-line bg-elevated px-2.5"
              style="height: {HEADER_H}px"
            >
              {#if selectedTableId === node.id && !arrowTool}
                <input
                  class="min-w-0 flex-1 rounded border border-line bg-input px-1 py-0.5 text-body font-semibold text-fg"
                  value={node.name}
                  data-no-drag
                  oninput={(e) =>
                    renameTable((e.currentTarget as HTMLInputElement).value)}
                  onclick={(e) => e.stopPropagation()}
                />
              {:else}
                <span class="truncate text-body font-semibold text-fg">{node.name}</span>
              {/if}
              <span class="font-mono text-micro text-fg-muted">TBL</span>
            </div>

            <!-- columns: each row is a relationship endpoint -->
            <ul class="m-0 list-none p-0">
              {#each node.columns as col, idx (col.id)}
                <li
                  class={cn(
                    "group relative flex items-center border-b border-line/50 last:border-b-0",
                    linkFrom?.tableId === node.id &&
                      linkFrom?.columnId === col.id &&
                      "bg-accent-muted",
                    arrowTool && "hover:bg-hover",
                  )}
                  style="height: {ROW_H}px"
                  data-no-drag
                >
                  <!-- left handle (incoming) -->
                  <button
                    type="button"
                    data-col-handle
                    class={cn(
                      "absolute top-1/2 left-0 z-10 size-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-accent bg-panel",
                      arrowTool ? "opacity-100" : "opacity-0 group-hover:opacity-70",
                    )}
                    title="Drop relationship here"
                    onpointerup={(e) => onColumnPointerUp(node.id, col.id, e)}
                    onclick={(e) => onColumnClick(node.id, col.id, e)}
                  ></button>

                  <div class="flex min-w-0 flex-1 items-center gap-1 px-3">
                    {#if col.pk}
                      <span class="text-micro text-accent" title="PK">PK</span>
                    {/if}
                    <span class="truncate font-mono text-caption text-fg">{col.name}</span>
                    <span class="shrink-0 font-mono text-caption text-fg-muted">
                      {col.dataType}
                    </span>
                  </div>

                  {#if selectedTableId === node.id && !arrowTool}
                    <button
                      type="button"
                      class="mr-1 border-none bg-transparent px-1 text-caption text-danger opacity-0 group-hover:opacity-100"
                      data-no-drag
                      title="Delete column"
                      onclick={(e) => {
                        e.stopPropagation();
                        deleteColumn(col.id);
                      }}
                    >
                      ×
                    </button>
                  {/if}

                  <!-- right handle (outgoing) -->
                  <button
                    type="button"
                    data-col-handle
                    class={cn(
                      "absolute top-1/2 right-0 z-10 size-2.5 translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-accent bg-panel",
                      arrowTool ? "cursor-crosshair opacity-100" : "opacity-0 group-hover:opacity-70",
                    )}
                    title={arrowTool ? "Start link from this column" : "Column"}
                    onpointerdown={(e) => onColumnPointerDown(node.id, col.id, e)}
                    onpointermove={onColumnPointerMove}
                    onpointerup={(e) => onColumnPointerUp(node.id, col.id, e)}
                    onclick={(e) => onColumnClick(node.id, col.id, e)}
                  ></button>
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
                    <Input
                      bind:value={newColName}
                      placeholder="column_name"
                      inputSize="sm"
                    />
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
