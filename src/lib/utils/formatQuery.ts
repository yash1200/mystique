/**
 * Query formatting registry (frontend).
 *
 * Each engine maps to a formatter. New engines: add a case in
 * `formatterIdForEngine` and/or register a custom formatter.
 * Prefer frontend over BE — formatting is local and needs no DB session.
 */
import { format as formatSql, type SqlLanguage } from "sql-formatter";
import {
  defaultCapabilitiesForEngine,
  queryLangFromCaps,
} from "$lib/types/engineCaps";

export type FormatQueryResult =
  | { ok: true; text: string }
  | { ok: false; error: string; text: string };

export type QueryFormatter = (text: string) => FormatQueryResult;

/** Built-in SQL dialect formatters (sql-formatter). */
const SQL_DIALECT: Record<string, SqlLanguage> = {
  postgres: "postgresql",
  postgresql: "postgresql",
  pg: "postgresql",
  mysql: "mysql",
  mariadb: "mysql",
  sqlite: "sqlite",
  sql: "sql",
};

function formatWithSqlDialect(dialect: SqlLanguage): QueryFormatter {
  return (text) => {
    const src = text.trim();
    if (!src) {
      return { ok: false, error: "Nothing to format.", text };
    }
    try {
      const formatted = formatSql(src, {
        language: dialect,
        tabWidth: 2,
        keywordCase: "upper",
        dataTypeCase: "upper",
        functionCase: "upper",
        logicalOperatorNewline: "before",
        expressionWidth: 80,
        linesBetweenQueries: 1,
      });
      // Preserve a single trailing newline for editor comfort.
      const out = formatted.replace(/\s+$/, "") + "\n";
      return { ok: true, text: out };
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      return { ok: false, error: msg || "Could not format SQL.", text };
    }
  };
}

/**
 * Mongo / MQL: pretty-print bare JSON filters; lightly tidy shell-style calls.
 * Full shell ASTs are engine-specific — keep this conservative.
 */
const formatMql: QueryFormatter = (text) => {
  const src = text.trim();
  if (!src) {
    return { ok: false, error: "Nothing to format.", text };
  }

  // Bare JSON document / array
  if (src.startsWith("{") || src.startsWith("[")) {
    try {
      const parsed = JSON.parse(src) as unknown;
      return {
        ok: true,
        text: JSON.stringify(parsed, null, 2) + "\n",
      };
    } catch {
      return {
        ok: false,
        error: "Invalid JSON filter.",
        text,
      };
    }
  }

  // Shell-style: pretty-print object/array literals inside find(...), aggregate([...]), etc.
  try {
    const pretty = prettyPrintEmbeddedJson(src);
    if (pretty !== src) {
      return { ok: true, text: pretty.replace(/\s+$/, "") + "\n" };
    }
  } catch {
    /* fall through */
  }

  // Fallback: normalize line endings / trim trailing space per line
  const normalized = src
    .split("\n")
    .map((l) => l.replace(/\s+$/, ""))
    .join("\n")
    .replace(/\n{3,}/g, "\n\n");
  return { ok: true, text: normalized.replace(/\s+$/, "") + "\n" };
};

/** Replace balanced `{...}` / `[...]` segments with pretty JSON when parseable. */
function prettyPrintEmbeddedJson(src: string): string {
  let out = "";
  let i = 0;
  while (i < src.length) {
    const ch = src[i];
    if (ch === "{" || ch === "[") {
      const end = findMatching(src, i);
      if (end > i) {
        const slice = src.slice(i, end + 1);
        try {
          const parsed = JSON.parse(slice) as unknown;
          const pretty = JSON.stringify(parsed, null, 2);
          // Indent continuation lines to match surrounding indent
          const lineStart = src.lastIndexOf("\n", i) + 1;
          const baseIndent = src.slice(lineStart, i).match(/^\s*/)?.[0] ?? "";
          const indented = pretty
            .split("\n")
            .map((line, idx) => (idx === 0 ? line : baseIndent + line))
            .join("\n");
          out += indented;
          i = end + 1;
          continue;
        } catch {
          /* not pure JSON — keep raw */
        }
      }
    }
    out += ch;
    i += 1;
  }
  return out;
}

function findMatching(src: string, openIdx: number): number {
  const open = src[openIdx];
  const close = open === "{" ? "}" : "]";
  let depth = 0;
  let inStr: '"' | "'" | null = null;
  let escape = false;
  for (let i = openIdx; i < src.length; i++) {
    const c = src[i];
    if (inStr) {
      if (escape) {
        escape = false;
        continue;
      }
      if (c === "\\") {
        escape = true;
        continue;
      }
      if (c === inStr) inStr = null;
      continue;
    }
    if (c === '"' || c === "'") {
      inStr = c;
      continue;
    }
    if (c === open) depth += 1;
    else if (c === close) {
      depth -= 1;
      if (depth === 0) return i;
    }
  }
  return -1;
}

const formatters: Record<string, QueryFormatter> = {
  // SQL family
  postgres: formatWithSqlDialect("postgresql"),
  postgresql: formatWithSqlDialect("postgresql"),
  pg: formatWithSqlDialect("postgresql"),
  mysql: formatWithSqlDialect("mysql"),
  mariadb: formatWithSqlDialect("mysql"),
  sqlite: formatWithSqlDialect("sqlite"),
  sql: formatWithSqlDialect("sql"),
  // Document
  mongodb: formatMql,
  mongo: formatMql,
  mql: formatMql,
};

/**
 * Register or override a formatter for an engine id (future plugins / dialects).
 * Call once at app init if needed.
 */
export function registerQueryFormatter(
  engineId: string,
  formatter: QueryFormatter,
): void {
  formatters[engineId.toLowerCase()] = formatter;
}

function resolveFormatter(engine: string): QueryFormatter {
  const id = engine.toLowerCase().trim();
  if (formatters[id]) return formatters[id];
  const caps = defaultCapabilitiesForEngine(id);
  if (queryLangFromCaps(caps) === "mql") return formatMql;
  const fromCaps = caps.sqlDialect.trim().toLowerCase();
  if (fromCaps) return formatWithSqlDialect(fromCaps as SqlLanguage);
  const dialect = SQL_DIALECT[id];
  if (dialect) return formatWithSqlDialect(dialect);
  return formatWithSqlDialect("sql");
}

/** Format query text for the given engine. Never throws. */
export function formatQuery(engine: string, text: string): FormatQueryResult {
  try {
    return resolveFormatter(engine)(text);
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    return { ok: false, error: msg || "Format failed.", text };
  }
}
