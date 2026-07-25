import type {
  CatalogObjectKind,
  DatabaseMeta,
  TableMeta,
} from "$lib/types/workspace";

/** Server catalog (databases → objects) for an engine. Dummy only. */
export function mockCatalogForEngine(engine: string): DatabaseMeta[] {
  const e = String(engine).toLowerCase();

  if (e === "mongodb") {
    return [
      db("app", [
        leaf("app", "", "users", "collection", mongoUsersCols(), mongoUsersRows()),
        leaf("app", "", "orders", "collection", mongoOrdersCols(), mongoOrdersRows()),
        leaf("app", "", "events", "collection", mongoEventsCols(), mongoEventsRows()),
      ]),
      db("analytics", [
        leaf("analytics", "", "page_views", "collection", [
          col("_id", "ObjectId", { pk: true }),
          col("path", "string"),
          col("count", "number"),
        ], [
          { _id: "aa1…", path: "/home", count: 1204 },
          { _id: "aa2…", path: "/pricing", count: 388 },
        ]),
        leaf("analytics", "", "funnels", "collection", [
          col("_id", "ObjectId", { pk: true }),
          col("name", "string"),
          col("steps", "number"),
        ], [
          { _id: "ff1…", name: "signup", steps: 4 },
        ]),
      ]),
      db("admin", [
        leaf("admin", "", "system.users", "collection", [
          col("_id", "ObjectId", { pk: true }),
          col("user", "string"),
        ], [
          { _id: "su1…", user: "root" },
        ]),
      ]),
    ];
  }

  if (e === "mysql" || e === "mariadb") {
    return [
      db("mystique_dev", [
        leaf("mystique_dev", "", "users", "table", pgUsersCols(), pgUsersRows()),
        leaf("mystique_dev", "", "projects", "table", pgProjectsCols(), pgProjectsRows()),
        leaf("mystique_dev", "", "orders", "table", pgOrdersCols(), pgOrdersRows()),
        leaf("mystique_dev", "", "v_active_users", "view", [
          col("id", "int", { pk: true }),
          col("email", "varchar"),
          col("display_name", "varchar"),
        ], [
          { id: 1, email: "ada@example.com", display_name: "Ada Lovelace" },
          { id: 2, email: "grace@example.com", display_name: "Grace Hopper" },
        ]),
      ]),
      db("mystique_staging", [
        leaf("mystique_staging", "", "users", "table", pgUsersCols(), pgUsersRows().slice(0, 2)),
        leaf("mystique_staging", "", "orders", "table", pgOrdersCols(), pgOrdersRows().slice(0, 2)),
      ]),
      db("information_schema", [
        leaf("information_schema", "", "TABLES", "view", [
          col("TABLE_SCHEMA", "varchar"),
          col("TABLE_NAME", "varchar"),
          col("TABLE_TYPE", "varchar"),
        ], [
          { TABLE_SCHEMA: "mystique_dev", TABLE_NAME: "users", TABLE_TYPE: "BASE TABLE" },
        ]),
      ]),
    ];
  }

  if (e === "sqlite") {
    return [
      db("main", [
        leaf("main", "", "users", "table", pgUsersCols(), pgUsersRows()),
        leaf("main", "", "projects", "table", pgProjectsCols(), pgProjectsRows()),
        leaf("main", "", "orders", "table", pgOrdersCols(), pgOrdersRows()),
        leaf("main", "", "sqlite_sequence", "table", [
          col("name", "text", { pk: true }),
          col("seq", "integer"),
        ], [
          { name: "users", seq: 5 },
          { name: "orders", seq: 1005 },
        ]),
      ]),
    ];
  }

  // PostgreSQL — multiple databases; schemas inside each
  return [
    db("postgres", [
      leaf("postgres", "public", "users", "table", pgUsersCols(), pgUsersRows()),
      leaf("postgres", "public", "projects", "table", pgProjectsCols(), pgProjectsRows()),
      leaf("postgres", "public", "orders", "table", pgOrdersCols(), pgOrdersRows()),
      leaf("postgres", "public", "schema_migrations", "table", [
        col("version", "bigint", { pk: true }),
        col("dirty", "boolean"),
      ], [
        { version: 20260101000001, dirty: false },
        { version: 20260315000002, dirty: false },
      ]),
      leaf("postgres", "public", "v_active_users", "view", [
        col("id", "integer", { pk: true }),
        col("email", "text"),
        col("display_name", "text"),
      ], [
        { id: 1, email: "ada@example.com", display_name: "Ada Lovelace" },
      ]),
      leaf("postgres", "auth", "sessions", "table", [
        col("id", "uuid", { pk: true }),
        col("user_id", "integer"),
        col("ip", "inet", { nullable: true }),
        col("expires_at", "timestamptz"),
      ], [
        { id: "a1b2c3d4-…", user_id: 1, ip: "127.0.0.1", expires_at: "2026-07-26 12:00:00" },
      ]),
      leaf("postgres", "auth", "api_keys", "table", [
        col("id", "uuid", { pk: true }),
        col("user_id", "integer"),
        col("prefix", "text"),
        col("created_at", "timestamptz"),
      ], [
        { id: "k1…", user_id: 1, prefix: "msk_live_", created_at: "2026-06-01 09:00:00" },
      ]),
    ], "default"),
    db("mystique_dev", [
      leaf("mystique_dev", "public", "users", "table", pgUsersCols(), pgUsersRows()),
      leaf("mystique_dev", "public", "events", "table", [
        col("id", "bigint", { pk: true }),
        col("name", "text"),
        col("payload", "jsonb"),
        col("created_at", "timestamptz"),
      ], [
        { id: 1, name: "signup", payload: '{"plan":"pro"}', created_at: "2026-07-01 10:00:00" },
        { id: 2, name: "login", payload: '{}', created_at: "2026-07-01 10:05:00" },
      ]),
      leaf("mystique_dev", "billing", "invoices", "table", [
        col("id", "uuid", { pk: true }),
        col("amount_cents", "integer"),
        col("status", "text"),
      ], [
        { id: "inv-1", amount_cents: 4900, status: "paid" },
      ]),
    ]),
    db("template1", [
      leaf("template1", "public", "placeholder", "table", [
        col("id", "integer", { pk: true }),
      ], [{ id: 1 }]),
    ], "template"),
  ];
}

/** @deprecated Prefer mockCatalogForEngine — flat list of all tables across DBs. */
export function mockTablesForEngine(engine: string): TableMeta[] {
  return mockCatalogForEngine(engine).flatMap((d) => d.tables);
}

function db(name: string, tables: TableMeta[], meta?: string): DatabaseMeta {
  return { name, tables, meta };
}

function col(
  name: string,
  type: string,
  opts?: { pk?: boolean; nullable?: boolean },
) {
  return { name, type, pk: opts?.pk, nullable: opts?.nullable };
}

function leaf(
  database: string,
  schema: string,
  name: string,
  kind: CatalogObjectKind,
  columns: TableMeta["columns"],
  rows: Record<string, unknown>[],
): TableMeta {
  const id = schema
    ? `${database}.${schema}.${name}`
    : `${database}.${name}`;
  return {
    id,
    database,
    schema,
    name,
    kind,
    rowCount: rows.length * 240 + 17,
    columns,
    rows,
  };
}

function pgUsersCols() {
  return [
    col("id", "integer", { pk: true }),
    col("email", "text"),
    col("display_name", "text"),
    col("is_active", "boolean"),
    col("created_at", "timestamptz"),
  ];
}
function pgUsersRows() {
  return [
    { id: 1, email: "ada@example.com", display_name: "Ada Lovelace", is_active: true, created_at: "2026-01-12 10:00:00" },
    { id: 2, email: "grace@example.com", display_name: "Grace Hopper", is_active: true, created_at: "2026-02-03 11:30:00" },
    { id: 3, email: "alan@example.com", display_name: "Alan Turing", is_active: false, created_at: "2026-03-18 08:15:00" },
    { id: 4, email: "katherine@example.com", display_name: "Katherine Johnson", is_active: true, created_at: "2026-04-01 16:45:00" },
    { id: 5, email: "linus@example.com", display_name: "Linus Torvalds", is_active: true, created_at: "2026-05-09 12:00:00" },
  ];
}
function pgProjectsCols() {
  return [
    col("id", "integer", { pk: true }),
    col("name", "text"),
    col("owner_id", "integer"),
    col("status", "text"),
    col("budget", "numeric"),
  ];
}
function pgProjectsRows() {
  return [
    { id: 10, name: "Mystique core", owner_id: 1, status: "active", budget: 120000 },
    { id: 11, name: "Query studio", owner_id: 2, status: "active", budget: 84000 },
    { id: 12, name: "Docs site", owner_id: 4, status: "paused", budget: 15000 },
    { id: 13, name: "Legacy import", owner_id: 3, status: "archived", budget: 0 },
  ];
}
function pgOrdersCols() {
  return [
    col("id", "bigint", { pk: true }),
    col("user_id", "integer"),
    col("total_cents", "integer"),
    col("currency", "char(3)"),
    col("status", "text"),
    col("placed_at", "timestamptz"),
  ];
}
function pgOrdersRows() {
  return [
    { id: 1001, user_id: 1, total_cents: 12850, currency: "USD", status: "paid", placed_at: "2026-05-10 14:22:00" },
    { id: 1002, user_id: 2, total_cents: 4200, currency: "USD", status: "pending", placed_at: "2026-05-11 09:01:00" },
    { id: 1003, user_id: 3, total_cents: 91025, currency: "EUR", status: "shipped", placed_at: "2026-05-12 18:40:00" },
    { id: 1004, user_id: 5, total_cents: 1999, currency: "USD", status: "refunded", placed_at: "2026-05-13 07:12:00" },
    { id: 1005, user_id: 4, total_cents: 56000, currency: "USD", status: "paid", placed_at: "2026-05-14 21:05:00" },
  ];
}
function mongoUsersCols() {
  return [
    col("_id", "ObjectId", { pk: true }),
    col("email", "string"),
    col("name", "string"),
    col("role", "string"),
    col("createdAt", "date"),
  ];
}
function mongoUsersRows() {
  return [
    { _id: "665a1…f01", email: "ada@example.com", name: "Ada Lovelace", role: "admin", createdAt: "2026-01-12" },
    { _id: "665a1…f02", email: "grace@example.com", name: "Grace Hopper", role: "editor", createdAt: "2026-02-03" },
    { _id: "665a1…f03", email: "alan@example.com", name: "Alan Turing", role: "viewer", createdAt: "2026-03-18" },
    { _id: "665a1…f04", email: "katherine@example.com", name: "Katherine Johnson", role: "editor", createdAt: "2026-04-01" },
  ];
}
function mongoOrdersCols() {
  return [
    col("_id", "ObjectId", { pk: true }),
    col("userId", "ObjectId"),
    col("total", "number"),
    col("status", "string"),
    col("placedAt", "date"),
  ];
}
function mongoOrdersRows() {
  return [
    { _id: "77b2…a11", userId: "665a1…f01", total: 128.5, status: "paid", placedAt: "2026-05-10" },
    { _id: "77b2…a12", userId: "665a1…f02", total: 42, status: "pending", placedAt: "2026-05-11" },
    { _id: "77b2…a13", userId: "665a1…f03", total: 910.25, status: "shipped", placedAt: "2026-05-12" },
  ];
}
function mongoEventsCols() {
  return [
    col("_id", "ObjectId", { pk: true }),
    col("type", "string"),
    col("payload", "object"),
    col("ts", "date"),
  ];
}
function mongoEventsRows() {
  return [
    { _id: "99c…01", type: "login", payload: '{ ip: "10.0.0.2" }', ts: "2026-07-20T09:00:00Z" },
    { _id: "99c…02", type: "query", payload: "{ ms: 12 }", ts: "2026-07-20T09:01:12Z" },
  ];
}
