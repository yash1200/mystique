/** High-level data family for live ER / model views. */
export type DataFamily = "sql" | "nosql" | "redis";

export type ErNodeKind = "table" | "collection" | "key_pattern" | "entity";

/** Normal = solid FK-style; broken = dashed / optional link. */
export type ErEdgeKind = "normal" | "broken" | "fk" | "inferred" | "logical" | "key_ref";

export interface ErColumn {
  id: string;
  name: string;
  dataType: string;
  pk?: boolean;
  nullable?: boolean;
}

export interface ErNode {
  id: string;
  kind: ErNodeKind;
  name: string;
  columns: ErColumn[];
  x: number;
  y: number;
}

export interface ErPoint {
  x: number;
  y: number;
}

export interface ErEdge {
  id: string;
  from: string;
  to: string;
  fromColumnId: string;
  toColumnId: string;
  kind: ErEdgeKind;
  label?: string;
  waypoints?: ErPoint[];
}

export interface ErDiagramState {
  family: DataFamily;
  nodes: ErNode[];
  edges: ErEdge[];
}

export const ER_GRID = 24;

export const FAMILY_META: Record<
  DataFamily,
  { title: string; short: string; description: string; nodeLabel: string; edgeLabel: string }
> = {
  sql: {
    title: "SQL databases",
    short: "SQL",
    description: "Tables and foreign-key links from the live catalog.",
    nodeLabel: "Table",
    edgeLabel: "Column → column",
  },
  nosql: {
    title: "NoSQL / document",
    short: "NoSQL",
    description: "Collections on a grid; field-to-field references.",
    nodeLabel: "Collection",
    edgeLabel: "Field → field",
  },
  redis: {
    title: "Redis / key-value",
    short: "Redis",
    description: "Key patterns on a grid; attribute-level links.",
    nodeLabel: "Key pattern",
    edgeLabel: "Attr → attr",
  },
};

export function snapToGrid(v: number, grid = ER_GRID): number {
  return Math.round(v / grid) * grid;
}
