<script lang="ts">
  import type { DataFamily, ErDiagramState, ErNode } from "$lib/types/er";
  import { FAMILY_META, emptyDiagram } from "$lib/types/er";
  import { Button, Badge, Inline } from "$lib/components/ui";
  import { cn } from "$lib/utils/cn";

  interface Props {
    family: DataFamily;
    onback: () => void;
    onchangeFamily: () => void;
  }

  let { family, onback, onchangeFamily }: Props = $props();

  let diagram = $state<ErDiagramState | null>(null);
  let selectedId = $state<string | null>(null);
  let linkFrom = $state<string | null>(null);
  let toolMsg = $state<string | null>(null);

  const meta = $derived(FAMILY_META[family]);

  $effect(() => {
    // Reset when family prop changes
    diagram = emptyDiagram(family);
    selectedId = null;
    linkFrom = null;
    toolMsg = null;
  });

  function uid(prefix: string) {
    return `${prefix}_${Math.random().toString(36).slice(2, 8)}`;
  }

  function selectNode(id: string) {
    selectedId = id;
    if (!diagram) return;
    if (linkFrom && linkFrom !== id) {
      // complete edge
      const kind =
        family === "sql" ? "fk" : family === "nosql" ? "inferred" : "logical";
      const label =
        family === "sql"
          ? "FK"
          : family === "nosql"
            ? "inferred ref"
            : "logical";
      diagram = {
        ...diagram,
        edges: [
          ...diagram.edges,
          { id: uid("e"), from: linkFrom, to: id, kind, label },
        ],
      };
      toolMsg = `Linked ${linkFrom} → ${id} (${label})`;
      linkFrom = null;
    }
  }

  function runTool(toolId: string) {
    if (!diagram) return;
    toolMsg = null;
    const n = diagram.nodes.length;
    const x = 60 + (n % 3) * 280;
    const y = 80 + Math.floor(n / 3) * 200;

    if (toolId === "add-table" || toolId === "add-collection" || toolId === "add-pattern") {
      const kind =
        family === "sql"
          ? "table"
          : family === "nosql"
            ? "collection"
            : "key_pattern";
      const name =
        family === "sql"
          ? `table_${n + 1}`
          : family === "nosql"
            ? `collection_${n + 1}`
            : `key:pattern_${n + 1}`;
      const fields =
        family === "sql"
          ? ["id PK"]
          : family === "nosql"
            ? ["_id"]
            : ["type: string"];
      const node: ErNode = {
        id: uid("n"),
        kind,
        name,
        fields,
        x,
        y,
      };
      diagram = { ...diagram, nodes: [...diagram.nodes, node] };
      selectedId = node.id;
      toolMsg = `Added ${meta.nodeLabel.toLowerCase()} “${name}”`;
      return;
    }

    if (toolId === "add-fk" || toolId === "add-inferred" || toolId === "add-group") {
      if (!selectedId) {
        toolMsg = "Select a source node first, then the target.";
        return;
      }
      linkFrom = selectedId;
      toolMsg = "Click another node to create the relationship.";
      return;
    }

    if (toolId === "add-column" || toolId === "add-field" || toolId === "add-type" || toolId === "add-ttl" || toolId === "sample-shape" || toolId === "mark-pk") {
      if (!selectedId) {
        toolMsg = "Select a node first.";
        return;
      }
      diagram = {
        ...diagram,
        nodes: diagram.nodes.map((node) => {
          if (node.id !== selectedId) return node;
          let fields = [...node.fields];
          if (toolId === "add-column") fields.push(`col_${fields.length}`);
          if (toolId === "add-field") fields.push(`field_${fields.length}`);
          if (toolId === "mark-pk" && fields[0] && !fields[0].includes("PK")) {
            fields[0] = `${fields[0]} PK`;
          }
          if (toolId === "sample-shape") {
            fields = ["_id", "name", "meta.nested", "tags[]"];
          }
          if (toolId === "add-type") {
            const types = ["string", "hash", "list", "set", "zset"];
            const next = types[fields.length % types.length];
            fields = fields.map((f) =>
              f.startsWith("type:") ? `type: ${next}` : f,
            );
            if (!fields.some((f) => f.startsWith("type:"))) {
              fields = [`type: ${next}`, ...fields];
            }
          }
          if (toolId === "add-ttl") {
            if (!fields.some((f) => f.startsWith("TTL"))) {
              fields = [...fields, "TTL: 3600s"];
            }
          }
          return { ...node, fields };
        }),
      };
      toolMsg = "Updated selected node.";
    }
  }

  function edgePath(fromId: string, toId: string): string {
    if (!diagram) return "";
    const a = diagram.nodes.find((n) => n.id === fromId);
    const b = diagram.nodes.find((n) => n.id === toId);
    if (!a || !b) return "";
    const x1 = a.x + 110;
    const y1 = a.y + 40;
    const x2 = b.x + 10;
    const y2 = b.y + 40;
    const mx = (x1 + x2) / 2;
    return `M ${x1} ${y1} C ${mx} ${y1}, ${mx} ${y2}, ${x2} ${y2}`;
  }
</script>

<div class="flex h-full min-h-0 flex-col">
  <!-- ER chrome bar -->
  <div
    class="flex h-topbar shrink-0 items-center justify-between gap-3 border-b border-line bg-panel px-4"
  >
    <div class="flex min-w-0 items-center gap-2">
      <Button variant="ghost" size="sm" onclick={onback}>← Back</Button>
      <span class="text-fg-muted">/</span>
      <span class="text-body font-medium text-fg">ER · {meta.title}</span>
      <Badge tone="accent" size="sm">FE only</Badge>
    </div>
    <Inline gap="sm">
      <Button variant="secondary" size="sm" onclick={onchangeFamily}>
        Change DB family
      </Button>
    </Inline>
  </div>

  <div class="flex min-h-0 flex-1">
    <!-- Tools -->
    <aside
      class="flex w-52 shrink-0 flex-col gap-2 border-r border-line bg-panel p-3"
      aria-label="ER tools"
    >
      <p class="text-label font-semibold tracking-wider text-fg-muted uppercase">
        Tools
      </p>
      {#each meta.tools as tool (tool.id)}
        <button
          type="button"
          class="rounded-md border border-line bg-input px-2.5 py-2 text-left text-label font-medium text-fg transition-colors hover:border-line-strong hover:bg-hover"
          title={tool.hint}
          onclick={() => runTool(tool.id)}
        >
          {tool.label}
          <span class="mt-0.5 block text-caption font-normal text-fg-muted">
            {tool.hint}
          </span>
        </button>
      {/each}
      {#if toolMsg}
        <p class="mt-2 text-caption text-fg-secondary" role="status">{toolMsg}</p>
      {/if}
      {#if linkFrom}
        <p class="text-caption text-accent">Linking from selected node…</p>
      {/if}
    </aside>

    <!-- Canvas -->
    <div class="relative min-w-0 flex-1 overflow-auto bg-app">
      {#if diagram}
        <svg
          class="pointer-events-none absolute inset-0 h-full min-h-[480px] w-full"
          aria-hidden="true"
        >
          {#each diagram.edges as edge (edge.id)}
            <path
              d={edgePath(edge.from, edge.to)}
              fill="none"
              stroke="var(--accent)"
              stroke-width="1.5"
              stroke-dasharray={edge.kind === "inferred" || edge.kind === "logical"
                ? "5 4"
                : "0"}
              opacity="0.85"
            />
          {/each}
        </svg>

        {#each diagram.nodes as node (node.id)}
          <button
            type="button"
            class={cn(
              "absolute w-[220px] rounded-xl border bg-panel p-3 text-left shadow-sm transition-colors",
              selectedId === node.id
                ? "border-accent ring-2 ring-accent/30"
                : "border-line hover:border-line-strong",
            )}
            style="left: {node.x}px; top: {node.y}px;"
            onclick={() => selectNode(node.id)}
          >
            <div class="mb-2 flex items-center justify-between gap-2">
              <span class="truncate text-body font-semibold text-fg">{node.name}</span>
              <span class="shrink-0 font-mono text-micro text-fg-muted uppercase">
                {node.kind.replace("_", " ")}
              </span>
            </div>
            <ul class="m-0 list-none space-y-0.5 p-0">
              {#each node.fields as field}
                <li class="truncate font-mono text-caption text-fg-secondary">
                  {field}
                </li>
              {/each}
            </ul>
          </button>
        {/each}
      {/if}

      <p
        class="pointer-events-none absolute right-3 bottom-3 max-w-sm text-right text-caption text-fg-muted"
      >
        FE canvas · relations are local only · live DB introspection comes with engine plugins
      </p>
    </div>
  </div>
</div>
