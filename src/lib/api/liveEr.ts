/**
 * Build a live ER / model diagram from plugin catalog + relationships.
 * Layout stays in the FE; SQL / FK discovery lives in engine plugins.
 */
import {
  listEngineCatalogColumns,
  listEngineRelationships,
  type CatalogColumnRow,
  type CatalogRelationship,
} from "./engines";
import type {
  DataFamily,
  ErColumn,
  ErDiagramState,
  ErEdge,
  ErNode,
} from "$lib/types/er";
import { layoutErNodes } from "$lib/utils/erLayout";

function nodeId(schema: string, name: string): string {
  const s = schema?.trim();
  return s ? `n_${s}__${name}` : `n_${name}`;
}

function colId(schema: string, table: string, column: string): string {
  const s = schema?.trim();
  return s ? `c_${s}__${table}__${column}` : `c_${table}__${column}`;
}

export function familyForEngine(engine: string): DataFamily {
  const e = engine.toLowerCase();
  if (e === "mongodb" || e === "mongo" || e === "documentdb") return "nosql";
  if (e === "redis") return "redis";
  return "sql";
}

export type LiveErLoadResult = {
  diagram: ErDiagramState;
  title: string;
  summary: string;
};

export async function loadLiveErDiagram(
  sessionId: string,
  engine: string,
  database: string | null | undefined,
): Promise<LiveErLoadResult> {
  const family = familyForEngine(engine);
  const db = (database ?? "").trim() || null;
  const catalogCols = await listEngineCatalogColumns({
    sessionId,
    database: db,
  });
  const rels = await listEngineRelationships({
    sessionId,
    database: db,
  });
  const diagram = buildFromCatalogColumns(family, catalogCols, db);
  addRelationshipEdges(diagram, rels);

  const title =
    family === "nosql"
      ? `Model · ${db || "database"}`
      : db
        ? `ER · ${db}`
        : "ER diagram";
  const noun =
    family === "nosql"
      ? "collections"
      : family === "redis"
        ? "key patterns"
        : "tables";
  const relLabel =
    family === "sql"
      ? `${diagram.edges.length} relationships`
      : diagram.edges.length > 0
        ? `${diagram.edges.length} inferred links`
        : "no declared FKs";
  return {
    diagram: {
      ...diagram,
      nodes: layoutErNodes(diagram.nodes, diagram.edges),
    },
    title,
    summary: `${diagram.nodes.length} ${noun} · ${relLabel}`,
  };
}

function buildFromCatalogColumns(
  family: DataFamily,
  rows: CatalogColumnRow[],
  defaultSchema: string | null,
): ErDiagramState {
  type Acc = { schema: string; name: string; columns: ErColumn[] };
  const tables = new Map<string, Acc>();

  for (const row of rows) {
    const table = row.table;
    const column = row.column;
    if (!table || !column) continue;
    const schema = row.schema?.trim() || defaultSchema || "";
    const key = `${schema}::${table}`;
    let acc = tables.get(key);
    if (!acc) {
      acc = { schema, name: table, columns: [] };
      tables.set(key, acc);
    }
    acc.columns.push({
      id: colId(schema, table, column),
      name: column,
      dataType: row.dataType || "unknown",
      pk: !!row.pk,
      nullable: row.nullable,
    });
  }

  const nodes: ErNode[] = [...tables.values()].map((t) => ({
    id: nodeId(t.schema, t.name),
    kind: family === "nosql" ? "collection" : "table",
    name:
      t.schema && t.schema !== "public" && t.schema !== "main"
        ? `${t.schema}.${t.name}`
        : t.name,
    columns: t.columns,
    x: 0,
    y: 0,
  }));

  return { family, nodes, edges: [] };
}

function addRelationshipEdges(
  diagram: ErDiagramState,
  rels: CatalogRelationship[],
) {
  const nodeIds = new Set(diagram.nodes.map((n) => n.id));
  const edges: ErEdge[] = [];
  let i = 0;
  for (const rel of rels) {
    const fromSchema = rel.fromSchema?.trim() || "";
    const toSchema = rel.toSchema?.trim() || "";
    const fromId = nodeId(fromSchema, rel.fromTable);
    const toId = nodeId(toSchema, rel.toTable);
    if (!nodeIds.has(fromId) || !nodeIds.has(toId)) continue;
    edges.push({
      id: `e_${i++}_${fromId}_${toId}`,
      from: fromId,
      to: toId,
      fromColumnId: colId(fromSchema, rel.fromTable, rel.fromColumn),
      toColumnId: colId(toSchema, rel.toTable, rel.toColumn),
      kind: "fk",
    });
  }
  diagram.edges = edges;
}
