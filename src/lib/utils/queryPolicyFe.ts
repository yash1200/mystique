/**
 * Lightweight FE-side query checks (stub until EngineQueryPolicy BE lands).
 * Fail closed: unknown / unparsed → block.
 */

export type QueryMode = "read" | "write";

export type FePolicyResult =
  | { ok: true; summary: string }
  | { ok: false; code: string; message: string };

function stripSqlComments(sql: string): string {
  return sql
    .replace(/\/\*[\s\S]*?\*\//g, " ")
    .replace(/--[^\n]*/g, " ")
    .trim();
}

function firstSqlKeyword(sql: string): string | null {
  const cleaned = stripSqlComments(sql);
  if (!cleaned) return null;
  const m = cleaned.match(/^([a-zA-Z_]+)/);
  return m ? m[1].toLowerCase() : null;
}

function hasMultipleSqlStatements(sql: string): boolean {
  const cleaned = stripSqlComments(sql);
  // Trailing semicolon only is OK; more than one non-empty segment is not.
  const parts = cleaned
    .split(";")
    .map((p) => p.trim())
    .filter(Boolean);
  return parts.length > 1;
}

const SQL_READ = new Set([
  "select",
  "with",
  "show",
  "describe",
  "desc",
  "explain",
  "values",
]);
const SQL_WRITE = new Set(["insert", "update", "replace", "merge"]);
const SQL_DELETE = new Set(["delete", "truncate", "drop", "alter", "create", "grant", "revoke"]);

function validateSql(text: string, mode: QueryMode): FePolicyResult {
  const cleaned = stripSqlComments(text);
  if (!cleaned) {
    return { ok: false, code: "empty", message: "Query is empty." };
  }
  if (hasMultipleSqlStatements(text)) {
    return {
      ok: false,
      code: "multi_statement",
      message: "Multi-statement SQL is not allowed. Run one statement at a time.",
    };
  }
  const kw = firstSqlKeyword(text);
  if (!kw) {
    return {
      ok: false,
      code: "unparsed",
      message: "Could not classify this SQL. Blocked (fail closed).",
    };
  }
  if (SQL_DELETE.has(kw) || kw === "delete") {
    return {
      ok: false,
      code: "delete_blocked",
      message: "DELETE and other destructive statements are disabled for now.",
    };
  }
  if (SQL_READ.has(kw)) {
    return { ok: true, summary: `Read SQL (${kw.toUpperCase()}) allowed.` };
  }
  if (SQL_WRITE.has(kw)) {
    if (mode === "read") {
      return {
        ok: false,
        code: "read_only",
        message: `Write statement (${kw.toUpperCase()}) blocked. Enable “Allow writes” to run inserts/updates.`,
      };
    }
    return { ok: true, summary: `Write SQL (${kw.toUpperCase()}) allowed (DELETE still blocked).` };
  }
  return {
    ok: false,
    code: "unparsed",
    message: `Statement type “${kw.toUpperCase()}” is not allowed in this mode.`,
  };
}

function validateMongo(text: string, mode: QueryMode): FePolicyResult {
  const t = text.trim();
  if (!t) {
    return { ok: false, code: "empty", message: "Query is empty." };
  }
  const lower = t.toLowerCase();

  // Destructive / delete-class
  if (
    /\bdelete(one|many)?\b/.test(lower) ||
    /\bdrop(database|collection)?\b/.test(lower) ||
    /\$out\b/.test(lower) ||
    /\$merge\b/.test(lower) ||
    /\bremove\b/.test(lower)
  ) {
    return {
      ok: false,
      code: "delete_blocked",
      message: "Delete/drop and write stages ($out / $merge) are disabled for now.",
    };
  }

  const isWrite =
    /\binsert(one|many)?\b/.test(lower) ||
    /\bupdate(one|many)?\b/.test(lower) ||
    /\breplaceone\b/.test(lower) ||
    /\bbulkwrite\b/.test(lower);

  const isRead =
    /\bfind\b/.test(lower) ||
    /\baggregate\b/.test(lower) ||
    /\bcount\b/.test(lower) ||
    /\bestimateddocumentcount\b/.test(lower) ||
    t.startsWith("{") ||
    t.startsWith("[");

  if (isWrite) {
    if (mode === "read") {
      return {
        ok: false,
        code: "read_only",
        message: "Write operation blocked. Enable “Allow writes” to run inserts/updates.",
      };
    }
    return { ok: true, summary: "Write MQL allowed (delete/drop still blocked)." };
  }

  if (isRead) {
    return { ok: true, summary: "Read MQL / filter allowed." };
  }

  return {
    ok: false,
    code: "unparsed",
    message: "Could not classify this Mongo query. Blocked (fail closed).",
  };
}

/** Engine-aware FE validation before (future) BE execute. */
export function validateQueryFe(
  engine: string,
  text: string,
  mode: QueryMode,
): FePolicyResult {
  const e = engine.toLowerCase();
  if (e === "mongodb" || e === "mongo") {
    return validateMongo(text, mode);
  }
  // postgres, mysql, sqlite, and unknown SQL-family engines
  return validateSql(text, mode);
}

export function queryLanguageForEngine(engine: string): "sql" | "mql" {
  const e = engine.toLowerCase();
  if (e === "mongodb" || e === "mongo") return "mql";
  return "sql";
}

export function queryPlaceholder(engine: string): string {
  const lang = queryLanguageForEngine(engine);
  if (lang === "mql") {
    return '{\n  "status": "active"\n}\n// or: db.users.find({ status: "active" })';
  }
  return "SELECT *\nFROM your_table\nLIMIT 100;";
}
