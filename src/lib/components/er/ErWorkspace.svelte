<script lang="ts">
  import type {
    DataFamily,
    ErColumn,
    ErDiagramState,
    ErNode,
  } from "$lib/types/er";
  import { FAMILY_META, SQL_TYPES, emptyDiagram } from "$lib/types/er";
  import { Button, Badge, Inline, Input, Select } from "$lib/components/ui";
  import { cn } from "$lib/utils/cn";
  import {
    listDiagrams,
    saveDiagram,
    type ErDiagramRecord,
  } from "$lib/api/diagrams";
  import { isTauri } from "$lib/api/tauri";

  interface Props {
    family: DataFamily;
    onback: () => void;
    onchangeFamily: () => void;
  }

  let { family, onback, onchangeFamily }: Props = $props();

  let diagram = $state<ErDiagramState | null>(null);
  let diagramId = $state<string | null>(null);
  let diagramName = $state("Untitled diagram");
  let selectedId = $state<string | null>(null);
  let linkMode = $state(false);
  let linkFrom = $state<string | null>(null);
  let toolMsg = $state<string | null>(null);
  let saving = $state(false);
  let dirty = $state(false);

  // drag state
  let dragId = $state<string | null>(null);
  let dragOffsetX = 0;
  let dragOffsetY = 0;
  let canvasEl: HTMLDivElement | null = $state(null);

  // inline column form on selected table
  let showAddCol = $state(false);
  let newColName = $state("");
  let newColType = $state<string>("TEXT");
  let newColPk = $state(false);

  const meta = $derived(FAMILY_META[family]);
  const selected = $derived(
    diagram?.nodes.find((n) => n.id === selectedId) ?? null,
  );
  const isSql = $derived(family === "sql");

  $effect(() => {
    diagram = emptyDiagram(family);
    diagramId = null;
    diagramName = `Untitled ${FAMILY_META[family].short} diagram`;
    selectedId = null;
    linkMode = false;
    linkFrom = null;
    toolMsg = null;
    showAddCol = false;
    dirty = false;
  });

  function uid(prefix: string) {
    return `${prefix}_${Math.random().toString(36).slice(2, 9)}`;
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
      columns: isSql
        ? [
            {
              id: uid("c"),
              name: "id",
              dataType: "UUID",
              pk: true,
            },
          ]
        : [
            {
              id: uid("c"),
              name: family === "nosql" ? "_id" : "type",
              dataType: family === "nosql" ? "ObjectId" : "string",
              pk: family === "nosql",
            },
          ],
      x: 80 + (n % 4) * 40,
      y: 80 + (n % 4) * 30,
    };
    diagram = { ...diagram, nodes: [...diagram.nodes, node] };
    selectedId = node.id;
    showAddCol = false;
    toolMsg = `Added “${node.name}” — drag it anywhere on the canvas.`;
    markDirty();
  }

  function startLinkMode() {
    if (!selectedId) {
      toolMsg = "Select a table first, then click Add relationship, then the target table.";
      return;
    }
    linkMode = true;
    linkFrom = selectedId;
    toolMsg = "Click another table to draw a relationship arrow.";
  }

  function onNodeClick(id: string, e: MouseEvent) {
    // Don't steal click after drag
    if ((e.target as HTMLElement).closest("[data-no-select]")) return;
    if (linkMode && linkFrom) {
      if (linkFrom === id) {
        toolMsg = "Pick a different table as the target.";
        return;
      }
      if (!diagram) return;
      const fromNode = diagram.nodes.find((n) => n.id === linkFrom);
      const toNode = diagram.nodes.find((n) => n.id === id);
      const kind =
        family === "sql" ? "fk" : family === "nosql" ? "inferred" : "logical";
      const fromCol =
        fromNode?.columns.find((c) => !c.pk)?.name ??
        fromNode?.columns[0]?.name ??
        "id";
      const toCol =
        toNode?.columns.find((c) => c.pk)?.name ??
        toNode?.columns[0]?.name ??
        "id";
      const label =
        family === "sql"
          ? `${fromCol} → ${toNode?.name ?? ""}.${toCol}`
          : family === "nosql"
            ? "inferred ref"
            : "logical link";
      diagram = {
        ...diagram,
        edges: [
          ...diagram.edges,
          {
            id: uid("e"),
            from: linkFrom,
            to: id,
            kind,
            label,
            fromColumn: fromCol,
            toColumn: toCol,
          },
        ],
      };
      toolMsg = `Relationship: ${label}`;
      linkMode = false;
      linkFrom = null;
      markDirty();
      return;
    }
    selectedId = id;
    showAddCol = false;
  }

  function onPointerDown(nodeId: string, e: PointerEvent) {
    if ((e.target as HTMLElement).closest("[data-no-drag]")) return;
    if (!diagram || !canvasEl) return;
    const node = diagram.nodes.find((n) => n.id === nodeId);
    if (!node) return;
    const rect = canvasEl.getBoundingClientRect();
    dragId = nodeId;
    dragOffsetX = e.clientX - rect.left - node.x + canvasEl.scrollLeft;
    dragOffsetY = e.clientY - rect.top - node.y + canvasEl.scrollTop;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    e.preventDefault();
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragId || !diagram || !canvasEl) return;
    const rect = canvasEl.getBoundingClientRect();
    const x = Math.max(8, e.clientX - rect.left + canvasEl.scrollLeft - dragOffsetX);
    const y = Math.max(8, e.clientY - rect.top + canvasEl.scrollTop - dragOffsetY);
    diagram = {
      ...diagram,
      nodes: diagram.nodes.map((n) =>
        n.id === dragId ? { ...n, x, y } : n,
      ),
    };
  }

  function onPointerUp(e: PointerEvent) {
    if (dragId) {
      markDirty();
      try {
        (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
      } catch {
        /* ignore */
      }
    }
    dragId = null;
  }

  function renameTable(name: string) {
    if (!diagram || !selectedId) return;
    diagram = {
      ...diagram,
      nodes: diagram.nodes.map((n) =>
        n.id === selectedId ? { ...n, name: name || n.name } : n,
      ),
    };
    markDirty();
  }

  function deleteSelectedTable() {
    if (!diagram || !selectedId) return;
    const id = selectedId;
    diagram = {
      ...diagram,
      nodes: diagram.nodes.filter((n) => n.id !== id),
      edges: diagram.edges.filter((e) => e.from !== id && e.to !== id),
    };
    selectedId = null;
    showAddCol = false;
    toolMsg = "Table removed.";
    markDirty();
  }

  function addColumn() {
    if (!diagram || !selectedId) return;
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
        if (n.id !== selectedId) return n;
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
    toolMsg = `Column “${name}” added.`;
    markDirty();
  }

  function deleteColumn(colId: string) {
    if (!diagram || !selectedId) return;
    diagram = {
      ...diagram,
      nodes: diagram.nodes.map((n) =>
        n.id === selectedId
          ? { ...n, columns: n.columns.filter((c) => c.id !== colId) }
          : n,
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

  function nodeCenter(n: ErNode) {
    const h = 48 + n.columns.length * 22;
    return { x: n.x + 110, y: n.y + h / 2 };
  }

  function edgePath(fromId: string, toId: string): string {
    if (!diagram) return "";
    const a = diagram.nodes.find((n) => n.id === fromId);
    const b = diagram.nodes.find((n) => n.id === toId);
    if (!a || !b) return "";
    const p1 = nodeCenter(a);
    const p2 = nodeCenter(b);
    const mx = (p1.x + p2.x) / 2;
    return `M ${p1.x} ${p1.y} C ${mx} ${p1.y}, ${mx} ${p2.y}, ${p2.x} ${p2.y}`;
  }

  async function handleSave() {
    if (!diagram) return;
    saving = true;
    toolMsg = null;
    try {
      if (!isTauri()) {
        // Browser fallback
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
        const next = [rec, ...list.filter((r) => r.id !== id)];
        localStorage.setItem(key, JSON.stringify(next));
        diagramId = id;
        dirty = false;
        toolMsg = "Saved locally (browser). Run Tauri for app DB storage.";
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
      toolMsg = `Saved “${saved.name}” to app database.`;
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
        list = raw ? JSON.parse(raw) : [];
        list = list.filter((r) => r.family === family);
      }
      if (list.length === 0) {
        toolMsg = "No saved diagrams for this family yet.";
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
      <Input
        bind:value={diagramName}
        class="w-44"
        inputSize="sm"
        aria-label="Diagram name"
      />
      <Button variant="secondary" size="sm" onclick={handleLoadLatest}>
        Load last
      </Button>
      <Button variant="primary" size="sm" onclick={handleSave} disabled={saving}>
        {saving ? "Saving…" : "Save"}
      </Button>
      <Button variant="ghost" size="sm" onclick={onchangeFamily}>
        Change family
      </Button>
    </Inline>
  </div>

  <div class="flex min-h-0 flex-1">
    <!-- Tools: table-centric -->
    <aside
      class="flex w-56 shrink-0 flex-col gap-2 border-r border-line bg-panel p-3"
      aria-label="ER tools"
    >
      <p class="text-label font-semibold tracking-wider text-fg-muted uppercase">
        Tools
      </p>
      <Button variant="primary" fullWidth onclick={addTable}>
        + Add {meta.nodeLabel.toLowerCase()}
      </Button>
      <Button
        variant={linkMode ? "secondary" : "ghost"}
        fullWidth
        onclick={startLinkMode}
      >
        {linkMode ? "Click target table…" : "Add relationship"}
      </Button>
      {#if selectedId}
        <Button variant="danger-ghost" fullWidth onclick={deleteSelectedTable}>
          Delete selected
        </Button>
      {/if}

      <div class="mt-2 border-t border-line pt-3">
        <p class="mb-1 text-caption text-fg-muted">
          Drag tables freely. Click a table to edit columns. Use relationship
          mode for arrows.
        </p>
        {#if toolMsg}
          <p class="text-caption text-fg-secondary" role="status">{toolMsg}</p>
        {/if}
        {#if linkMode}
          <p class="text-caption text-accent">Relationship mode on</p>
        {/if}
      </div>

      {#if diagram && diagram.edges.length > 0}
        <div class="mt-2 border-t border-line pt-3">
          <p class="mb-1 text-label font-semibold text-fg-muted uppercase">
            Relationships
          </p>
          <ul class="m-0 flex list-none flex-col gap-1 p-0">
            {#each diagram.edges as edge (edge.id)}
              <li
                class="flex items-start justify-between gap-1 rounded-md border border-line bg-input px-2 py-1.5 text-caption"
              >
                <span class="min-w-0 truncate text-fg-secondary">
                  {edge.label ?? edge.id}
                </span>
                <button
                  type="button"
                  class="shrink-0 border-none bg-transparent p-0 text-caption text-danger hover:underline"
                  data-no-drag
                  data-no-select
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

    <!-- Canvas -->
    <div
      class="relative min-w-0 flex-1 overflow-auto bg-app"
      bind:this={canvasEl}
      role="application"
      aria-label="ER canvas"
    >
      {#if diagram}
        <svg
          class="pointer-events-none absolute inset-0 h-full min-h-[640px] w-full min-w-[960px]"
          aria-hidden="true"
        >
          <defs>
            <marker
              id="arrowhead"
              markerWidth="8"
              markerHeight="8"
              refX="7"
              refY="3"
              orient="auto"
            >
              <path d="M0,0 L7,3 L0,6 Z" fill="var(--accent)" />
            </marker>
          </defs>
          {#each diagram.edges as edge (edge.id)}
            <path
              d={edgePath(edge.from, edge.to)}
              fill="none"
              stroke="var(--accent)"
              stroke-width="2"
              stroke-dasharray={edge.kind === "fk" ? "0" : "6 4"}
              marker-end="url(#arrowhead)"
              opacity="0.9"
            />
          {/each}
        </svg>

        {#each diagram.nodes as node (node.id)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class={cn(
              "absolute w-[230px] select-none rounded-xl border bg-panel shadow-sm",
              selectedId === node.id
                ? "border-accent ring-2 ring-accent/35"
                : "border-line",
              dragId === node.id ? "cursor-grabbing opacity-95" : "cursor-grab",
            )}
            style="left: {node.x}px; top: {node.y}px;"
            onpointerdown={(e) => onPointerDown(node.id, e)}
            onpointermove={onPointerMove}
            onpointerup={onPointerUp}
            onpointercancel={onPointerUp}
            onclick={(e) => onNodeClick(node.id, e)}
          >
            <div
              class="flex items-center justify-between gap-2 border-b border-line bg-elevated px-3 py-2"
            >
              {#if selectedId === node.id}
                <input
                  class="min-w-0 flex-1 rounded-md border border-line bg-input px-1.5 py-0.5 text-body font-semibold text-fg"
                  value={node.name}
                  data-no-drag
                  data-no-select
                  oninput={(e) =>
                    renameTable((e.currentTarget as HTMLInputElement).value)}
                  onclick={(e) => e.stopPropagation()}
                />
              {:else}
                <span class="truncate text-body font-semibold text-fg">
                  {node.name}
                </span>
              {/if}
              <span class="shrink-0 font-mono text-micro text-fg-muted uppercase">
                {node.kind === "table" ? "TBL" : node.kind.slice(0, 3)}
              </span>
            </div>

            <ul class="m-0 list-none p-0">
              {#each node.columns as col (col.id)}
                <li
                  class="group flex items-center justify-between gap-1 border-b border-line/60 px-3 py-1.5 last:border-b-0"
                  data-no-drag
                >
                  <span class="min-w-0 truncate font-mono text-caption text-fg">
                    {#if col.pk}
                      <span class="text-accent">🔑 </span>
                    {/if}
                    {col.name}
                    <span class="text-fg-muted"> · {col.dataType}</span>
                  </span>
                  {#if selectedId === node.id}
                    <button
                      type="button"
                      class="shrink-0 border-none bg-transparent p-0 text-caption text-danger opacity-0 group-hover:opacity-100 hover:underline"
                      data-no-drag
                      data-no-select
                      title="Delete column"
                      onclick={(e) => {
                        e.stopPropagation();
                        deleteColumn(col.id);
                      }}
                    >
                      ×
                    </button>
                  {/if}
                </li>
              {/each}
            </ul>

            {#if selectedId === node.id}
              <div class="border-t border-line p-2" data-no-drag data-no-select>
                {#if !showAddCol}
                  <button
                    type="button"
                    class="w-full rounded-md border border-dashed border-line px-2 py-1.5 text-caption font-medium text-accent hover:bg-accent-muted"
                    onclick={(e) => {
                      e.stopPropagation();
                      showAddCol = true;
                      newColName = "";
                      newColType = isSql ? "TEXT" : "string";
                      newColPk = false;
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
                      onclick={(e: MouseEvent) => e.stopPropagation()}
                    />
                    {#if isSql}
                      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
                      <div
                        data-no-drag
                        data-no-select
                        role="presentation"
                        onclick={(e) => e.stopPropagation()}
                      >
                        <Select
                          bind:value={newColType}
                          options={SQL_TYPES.map((t) => ({ value: t, label: t }))}
                          inputSize="sm"
                        />
                      </div>
                    {:else}
                      <Input
                        bind:value={newColType}
                        placeholder="type"
                        inputSize="sm"
                      />
                    {/if}
                    <label
                      class="flex items-center gap-1.5 text-caption text-fg-secondary"
                    >
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
