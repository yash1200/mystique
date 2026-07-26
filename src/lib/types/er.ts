/** High-level data family for FE-only ER / model designer. */
export type DataFamily = "sql" | "nosql" | "redis";

export type ErNodeKind =
  | "table"
  | "collection"
  | "keyspace"
  | "key_pattern"
  | "entity";

export type ErEdgeKind = "fk" | "inferred" | "logical" | "key_ref";

export interface ErNode {
  id: string;
  kind: ErNodeKind;
  name: string;
  /** Fields / columns / keys shown on the card */
  fields: string[];
  x: number;
  y: number;
}

export interface ErEdge {
  id: string;
  from: string;
  to: string;
  kind: ErEdgeKind;
  label?: string;
}

export interface ErDiagramState {
  family: DataFamily;
  nodes: ErNode[];
  edges: ErEdge[];
}

export const FAMILY_META: Record<
  DataFamily,
  {
    title: string;
    short: string;
    description: string;
    nodeLabel: string;
    edgeLabel: string;
    tools: { id: string; label: string; hint: string }[];
  }
> = {
  sql: {
    title: "SQL databases",
    short: "SQL",
    description:
      "Relational model — tables, columns, primary keys, and foreign-key relationships (Postgres, MySQL, SQLite, …).",
    nodeLabel: "Table",
    edgeLabel: "Foreign key",
    tools: [
      { id: "add-table", label: "Add table", hint: "Entity with columns" },
      { id: "add-fk", label: "Add FK", hint: "Link two tables" },
      { id: "add-column", label: "Add column", hint: "Field on selected table" },
      { id: "mark-pk", label: "Mark PK", hint: "Primary key badge" },
    ],
  },
  nosql: {
    title: "NoSQL / document",
    short: "NoSQL",
    description:
      "Document model — collections, nested fields, and inferred references (MongoDB, DocumentDB, …).",
    nodeLabel: "Collection",
    edgeLabel: "Inferred ref",
    tools: [
      { id: "add-collection", label: "Add collection", hint: "Document set" },
      {
        id: "add-inferred",
        label: "Add reference",
        hint: "Soft link between collections",
      },
      { id: "add-field", label: "Add field", hint: "Path on selected collection" },
      { id: "sample-shape", label: "Sample shape", hint: "Demo nested fields" },
    ],
  },
  redis: {
    title: "Redis / key-value",
    short: "Redis",
    description:
      "Key-oriented model — key patterns, types (string, hash, list, set), and logical groupings.",
    nodeLabel: "Key pattern",
    edgeLabel: "Logical link",
    tools: [
      { id: "add-pattern", label: "Add key pattern", hint: "e.g. user:*" },
      { id: "add-type", label: "Set type", hint: "string / hash / list / set" },
      { id: "add-group", label: "Link patterns", hint: "Logical relationship" },
      { id: "add-ttl", label: "Note TTL", hint: "Expiry hint on pattern" },
    ],
  },
};

export function emptyDiagram(family: DataFamily): ErDiagramState {
  const seed: Record<DataFamily, ErDiagramState> = {
    sql: {
      family: "sql",
      nodes: [
        {
          id: "n1",
          kind: "table",
          name: "users",
          fields: ["id PK", "email", "created_at"],
          x: 80,
          y: 100,
        },
        {
          id: "n2",
          kind: "table",
          name: "orders",
          fields: ["id PK", "user_id FK", "total"],
          x: 380,
          y: 140,
        },
      ],
      edges: [
        { id: "e1", from: "n2", to: "n1", kind: "fk", label: "user_id → users.id" },
      ],
    },
    nosql: {
      family: "nosql",
      nodes: [
        {
          id: "n1",
          kind: "collection",
          name: "users",
          fields: ["_id", "email", "profile.name"],
          x: 80,
          y: 100,
        },
        {
          id: "n2",
          kind: "collection",
          name: "orders",
          fields: ["_id", "userId", "items[]"],
          x: 380,
          y: 140,
        },
      ],
      edges: [
        {
          id: "e1",
          from: "n2",
          to: "n1",
          kind: "inferred",
          label: "userId → users._id (inferred)",
        },
      ],
    },
    redis: {
      family: "redis",
      nodes: [
        {
          id: "n1",
          kind: "key_pattern",
          name: "user:{id}",
          fields: ["type: hash", "fields: name, email"],
          x: 80,
          y: 100,
        },
        {
          id: "n2",
          kind: "key_pattern",
          name: "session:{token}",
          fields: ["type: string", "TTL: 3600s"],
          x: 380,
          y: 140,
        },
      ],
      edges: [
        {
          id: "e1",
          from: "n2",
          to: "n1",
          kind: "logical",
          label: "session → user id",
        },
      ],
    },
  };
  return structuredClone(seed[family]);
}
