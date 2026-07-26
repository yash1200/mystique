/** High-level data family for ER / model designer. */
export type DataFamily = "sql" | "nosql" | "redis";

export type ErNodeKind =
  | "table"
  | "collection"
  | "key_pattern"
  | "entity";

export type ErEdgeKind = "fk" | "inferred" | "logical" | "key_ref";

/** Column / field on a node (SQL tables use name + type). */
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
  from: string;
  to: string;
  kind: ErEdgeKind;
  label?: string;
  /** Optional column-level endpoints for FK labels */
  fromColumn?: string;
  toColumn?: string;
}

export interface ErDiagramState {
  family: DataFamily;
  nodes: ErNode[];
  edges: ErEdge[];
}

/** Persisted diagram metadata + body (matches BE). */
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
      "Tables you can drag, columns with types, and foreign-key arrows between tables.",
    nodeLabel: "Table",
    edgeLabel: "Foreign key",
  },
  nosql: {
    title: "NoSQL / document",
    short: "NoSQL",
    description:
      "Collections and fields with inferred references (document-oriented).",
    nodeLabel: "Collection",
    edgeLabel: "Inferred ref",
  },
  redis: {
    title: "Redis / key-value",
    short: "Redis",
    description: "Key patterns, value types, and logical links between patterns.",
    nodeLabel: "Key pattern",
    edgeLabel: "Logical link",
  },
};

function col(
  name: string,
  dataType: string,
  opts?: { pk?: boolean; nullable?: boolean },
): ErColumn {
  return {
    id: `c_${Math.random().toString(36).slice(2, 9)}`,
    name,
    dataType,
    pk: opts?.pk,
    nullable: opts?.nullable,
  };
}

export function emptyDiagram(family: DataFamily): ErDiagramState {
  if (family === "sql") {
    const usersId = "n_users";
    const ordersId = "n_orders";
    return {
      family: "sql",
      nodes: [
        {
          id: usersId,
          kind: "table",
          name: "users",
          columns: [
            col("id", "UUID", { pk: true }),
            col("email", "VARCHAR(255)"),
            col("created_at", "TIMESTAMPTZ"),
          ],
          x: 80,
          y: 100,
        },
        {
          id: ordersId,
          kind: "table",
          name: "orders",
          columns: [
            col("id", "UUID", { pk: true }),
            col("user_id", "UUID"),
            col("total", "NUMERIC"),
          ],
          x: 400,
          y: 140,
        },
      ],
      edges: [
        {
          id: "e1",
          from: ordersId,
          to: usersId,
          kind: "fk",
          label: "user_id → users.id",
          fromColumn: "user_id",
          toColumn: "id",
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
            col("_id", "ObjectId", { pk: true }),
            col("email", "string"),
            col("profile.name", "string"),
          ],
          x: 80,
          y: 100,
        },
        {
          id: "n2",
          kind: "collection",
          name: "orders",
          columns: [
            col("_id", "ObjectId", { pk: true }),
            col("userId", "ObjectId"),
            col("items", "array"),
          ],
          x: 400,
          y: 140,
        },
      ],
      edges: [
        {
          id: "e1",
          from: "n2",
          to: "n1",
          kind: "inferred",
          label: "userId → users._id",
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
        columns: [col("type", "hash"), col("fields", "name, email")],
        x: 80,
        y: 100,
      },
      {
        id: "n2",
        kind: "key_pattern",
        name: "session:{token}",
        columns: [col("type", "string"), col("TTL", "3600s")],
        x: 400,
        y: 140,
      },
    ],
    edges: [
      {
        id: "e1",
        from: "n2",
        to: "n1",
        kind: "logical",
        label: "session → user",
      },
    ],
  };
}
