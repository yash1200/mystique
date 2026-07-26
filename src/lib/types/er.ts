/** High-level data family for ER / model designer. */
export type DataFamily = "sql" | "nosql" | "redis";

export type ErNodeKind = "table" | "collection" | "key_pattern" | "entity";

/** Normal = solid FK-style; broken = dashed / optional link (like DBeaver). */
export type ErEdgeKind = "normal" | "broken" | "fk" | "inferred" | "logical" | "key_ref";

/** Column / field on a node. Relationships attach to columns, not whole tables. */
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

export interface ErEdge {
  id: string;
  /** Source table / node id */
  from: string;
  /** Target table / node id */
  to: string;
  /** Source column id (required for proper FK links) */
  fromColumnId: string;
  /** Target column id */
  toColumnId: string;
  kind: ErEdgeKind;
  label?: string;
}

export interface ErDiagramState {
  family: DataFamily;
  nodes: ErNode[];
  edges: ErEdge[];
}

export interface ErDiagramRecord {
  id: string;
  name: string;
  family: DataFamily;
  body: ErDiagramState;
  createdAt: string;
  updatedAt: string;
}

export const SQL_TYPES = [
  "INTEGER",
  "BIGINT",
  "SERIAL",
  "UUID",
  "TEXT",
  "VARCHAR(255)",
  "BOOLEAN",
  "TIMESTAMP",
  "TIMESTAMPTZ",
  "DATE",
  "NUMERIC",
  "JSONB",
  "BYTEA",
] as const;

/** Canvas grid size (px) — tables snap to this lattice. */
export const ER_GRID = 24;

export const FAMILY_META: Record<
  DataFamily,
  {
    title: string;
    short: string;
    description: string;
    nodeLabel: string;
    edgeLabel: string;
  }
> = {
  sql: {
    title: "SQL databases",
    short: "SQL",
    description:
      "Tables on a grid. Link a column on table A to a column on table B (FK-style arrows).",
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
    description: "Key patterns on a grid; attribute-level logical links.",
    nodeLabel: "Key pattern",
    edgeLabel: "Attr → attr",
  },
};

function col(
  id: string,
  name: string,
  dataType: string,
  opts?: { pk?: boolean },
): ErColumn {
  return { id, name, dataType, pk: opts?.pk };
}

export function emptyDiagram(family: DataFamily): ErDiagramState {
  if (family === "sql") {
    const cUserId = "c_users_id";
    const cUserEmail = "c_users_email";
    const cUserCreated = "c_users_created";
    const cOrderId = "c_orders_id";
    const cOrderUserId = "c_orders_user_id";
    const cOrderTotal = "c_orders_total";
    return {
      family: "sql",
      nodes: [
        {
          id: "n_users",
          kind: "table",
          name: "users",
          columns: [
            col(cUserId, "id", "UUID", { pk: true }),
            col(cUserEmail, "email", "VARCHAR(255)"),
            col(cUserCreated, "created_at", "TIMESTAMPTZ"),
          ],
          x: ER_GRID * 4,
          y: ER_GRID * 4,
        },
        {
          id: "n_orders",
          kind: "table",
          name: "orders",
          columns: [
            col(cOrderId, "id", "UUID", { pk: true }),
            col(cOrderUserId, "user_id", "UUID"),
            col(cOrderTotal, "total", "NUMERIC"),
          ],
          x: ER_GRID * 18,
          y: ER_GRID * 6,
        },
      ],
      edges: [
        {
          id: "e1",
          from: "n_orders",
          to: "n_users",
          fromColumnId: cOrderUserId,
          toColumnId: cUserId,
          kind: "normal",
          label: "user_id → users.id",
        },
      ],
    };
  }
  if (family === "nosql") {
    const a = "c_u_id";
    const b = "c_o_uid";
    return {
      family: "nosql",
      nodes: [
        {
          id: "n1",
          kind: "collection",
          name: "users",
          columns: [
            col(a, "_id", "ObjectId", { pk: true }),
            col("c_u_email", "email", "string"),
          ],
          x: ER_GRID * 4,
          y: ER_GRID * 4,
        },
        {
          id: "n2",
          kind: "collection",
          name: "orders",
          columns: [
            col("c_o_id", "_id", "ObjectId", { pk: true }),
            col(b, "userId", "ObjectId"),
          ],
          x: ER_GRID * 18,
          y: ER_GRID * 6,
        },
      ],
      edges: [
        {
          id: "e1",
          from: "n2",
          to: "n1",
          fromColumnId: b,
          toColumnId: a,
          kind: "broken",
          label: "userId → _id",
        },
      ],
    };
  }
  const k1 = "c_r_type";
  const k2 = "c_s_type";
  return {
    family: "redis",
    nodes: [
      {
        id: "n1",
        kind: "key_pattern",
        name: "user:{id}",
        columns: [col(k1, "type", "hash"), col("c_r_f", "fields", "name,email")],
        x: ER_GRID * 4,
        y: ER_GRID * 4,
      },
      {
        id: "n2",
        kind: "key_pattern",
        name: "session:{token}",
        columns: [col(k2, "type", "string"), col("c_s_ttl", "TTL", "3600s")],
        x: ER_GRID * 18,
        y: ER_GRID * 6,
      },
    ],
    edges: [
      {
        id: "e1",
        from: "n2",
        to: "n1",
        fromColumnId: k2,
        toColumnId: k1,
        kind: "broken",
        label: "session → user",
      },
    ],
  };
}

export function snapToGrid(v: number, grid = ER_GRID): number {
  return Math.round(v / grid) * grid;
}
