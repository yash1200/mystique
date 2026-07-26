/** High-level data family for ER / model designer. */
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
  /** Optional intermediate points so the user can draw a custom path. */
  waypoints?: ErPoint[];
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

/** Mongo / document field types for the ER designer. */
export const NOSQL_TYPES = [
  "ObjectId",
  "string",
  "number",
  "boolean",
  "date",
  "array",
  "object",
  "null",
  "Decimal128",
  "Binary",
  "mixed",
] as const;

/** Redis value / attribute types for the ER designer. */
export const REDIS_TYPES = [
  "string",
  "hash",
  "list",
  "set",
  "zset",
  "stream",
  "bitmap",
  "geo",
  "JSON",
  "number",
  "TTL",
] as const;

export function columnTypesForFamily(family: DataFamily): readonly string[] {
  if (family === "sql") return SQL_TYPES;
  if (family === "nosql") return NOSQL_TYPES;
  return REDIS_TYPES;
}

export function defaultColumnType(family: DataFamily): string {
  if (family === "sql") return "TEXT";
  if (family === "nosql") return "string";
  return "string";
}

export const ER_GRID = 24;

export const FAMILY_META: Record<
  DataFamily,
  { title: string; short: string; description: string; nodeLabel: string; edgeLabel: string }
> = {
  sql: {
    title: "SQL databases",
    short: "SQL",
    description:
      "Tables on a grid. Link a column on table A to a column on table B with normal or broken arrows.",
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

function col(id: string, name: string, dataType: string, opts?: { pk?: boolean }): ErColumn {
  return { id, name, dataType, pk: opts?.pk };
}

export function emptyDiagram(family: DataFamily): ErDiagramState {
  if (family === "sql") {
    const cUserId = "c_users_id";
    const cOrderUserId = "c_orders_user_id";
    return {
      family: "sql",
      nodes: [
        {
          id: "n_users",
          kind: "table",
          name: "users",
          columns: [
            col(cUserId, "id", "UUID", { pk: true }),
            col("c_users_email", "email", "VARCHAR(255)"),
            col("c_users_created", "created_at", "TIMESTAMPTZ"),
          ],
          x: ER_GRID * 4,
          y: ER_GRID * 4,
        },
        {
          id: "n_orders",
          kind: "table",
          name: "orders",
          columns: [
            col("c_orders_id", "id", "UUID", { pk: true }),
            col(cOrderUserId, "user_id", "UUID"),
            col("c_orders_total", "total", "NUMERIC"),
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
          label: "orders.user_id → users.id",
          waypoints: [],
        },
      ],
    };
  }
  if (family === "nosql") {
    return {
      family: "nosql",
      nodes: [
        {
          id: "n1",
          kind: "collection",
          name: "users",
          columns: [
            col("c_u_id", "_id", "ObjectId", { pk: true }),
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
            col("c_o_uid", "userId", "ObjectId"),
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
          fromColumnId: "c_o_uid",
          toColumnId: "c_u_id",
          kind: "broken",
          label: "orders.userId → users._id",
          waypoints: [],
        },
      ],
    };
  }
  return {
    family: "redis",
    nodes: [
      {
        id: "n1",
        kind: "key_pattern",
        name: "user:{id}",
        columns: [col("c_r_type", "type", "hash"), col("c_r_f", "fields", "name,email")],
        x: ER_GRID * 4,
        y: ER_GRID * 4,
      },
      {
        id: "n2",
        kind: "key_pattern",
        name: "session:{token}",
        columns: [col("c_s_type", "type", "string"), col("c_s_ttl", "TTL", "3600s")],
        x: ER_GRID * 18,
        y: ER_GRID * 6,
      },
    ],
    edges: [
      {
        id: "e1",
        from: "n2",
        to: "n1",
        fromColumnId: "c_s_type",
        toColumnId: "c_r_type",
        kind: "broken",
        label: "session → user",
        waypoints: [],
      },
    ],
  };
}

export function snapToGrid(v: number, grid = ER_GRID): number {
  return Math.round(v / grid) * grid;
}
