import type { TableMeta } from "$lib/types/workspace";

/** Statement logged to history when opening a table preview. */
export function previewBodyForTable(engine: string, table: TableMeta): string {
  const e = engine.toLowerCase();
  if (e === "mongodb" || e === "mongo") {
    const db = table.database ? `${table.database}.` : "";
    return `// preview\ndb.${db}${table.name}.find({})`;
  }
  if (e === "mysql" || e === "mariadb") {
    const tick = (id: string) => `\`${id.replace(/`/g, "``")}\``;
    const q =
      table.schema || table.database
        ? `${tick(table.schema || table.database)}.${tick(table.name)}`
        : tick(table.name);
    return `SELECT * FROM ${q} LIMIT 100`;
  }
  if (e === "sqlite") {
    return `SELECT * FROM "${table.name.replace(/"/g, '""')}" LIMIT 100`;
  }
  const q =
    table.schema && table.schema.length > 0
      ? `"${table.schema.replace(/"/g, '""')}"."${table.name.replace(/"/g, '""')}"`
      : `"${table.name.replace(/"/g, '""')}"`;
  return `SELECT * FROM ${q} LIMIT 100`;
}
