//! Object structure sections (columns, indexes, FKs, triggers).

use mystique_connector_api::{
    cell_bool, ConnectorError, ConnectorResult, ObjectStructure, StructureSection,
};

use super::query_failed;
use super::SqliteSession;

pub(super) async fn describe_object_structure(
    session: &SqliteSession,
    _database: Option<&str>,
    _schema: Option<&str>,
    table: &str,
    kind: Option<&str>,
) -> ConnectorResult<ObjectStructure> {
    session.ensure_open()?;
    let pool = session.pool();
    let table_owned = table.to_string();
    let kind_owned = kind.map(str::to_string);
    session
        .blocking(move || -> ConnectorResult<ObjectStructure> {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            let safe = table_owned.replace('\'', "''");

            let mut col_stmt = conn
                .prepare(&format!("PRAGMA table_info('{safe}')"))
                .map_err(query_failed)?;
            let col_rows: Vec<Vec<String>> = col_stmt
                .query_map([], |row| {
                    let name: String = row.get(1)?;
                    let ctype: String = row.get(2)?;
                    let notnull: i64 = row.get(3)?;
                    let dflt: Option<String> = row.get(4)?;
                    let pk: i64 = row.get(5)?;
                    Ok(vec![
                        name,
                        if ctype.is_empty() {
                            "—".into()
                        } else {
                            ctype
                        },
                        cell_bool(notnull == 0),
                        cell_bool(pk != 0),
                        dflt.unwrap_or_else(|| "—".into()),
                    ])
                })
                .map_err(query_failed)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(query_failed)?;
            let columns_sec = StructureSection::table(
                "columns",
                "Columns",
                vec![
                    "Name".into(),
                    "Type".into(),
                    "Nullable".into(),
                    "PK".into(),
                    "Default".into(),
                ],
                col_rows,
            )
            .with_empty_message("No columns found.");

            let mut idx_list = conn
                .prepare(&format!("PRAGMA index_list('{safe}')"))
                .map_err(query_failed)?;
            let indexes_meta: Vec<(String, bool, String)> = idx_list
                .query_map([], |row| {
                    let name: String = row.get(1)?;
                    let unique: i64 = row.get(2)?;
                    let origin: String = row.get(3).unwrap_or_else(|_| "c".into());
                    Ok((name, unique != 0, origin))
                })
                .map_err(query_failed)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(query_failed)?;

            let mut index_rows = Vec::new();
            for (name, unique, origin) in indexes_meta {
                let safe_idx = name.replace('\'', "''");
                let mut info = conn
                    .prepare(&format!("PRAGMA index_info('{safe_idx}')"))
                    .map_err(query_failed)?;
                let cols: Vec<String> = info
                    .query_map([], |row| {
                        let cname: Option<String> = row.get(2)?;
                        Ok(cname.unwrap_or_else(|| "—".into()))
                    })
                    .map_err(query_failed)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(query_failed)?;
                index_rows.push(vec![
                    name,
                    cell_bool(unique),
                    origin,
                    if cols.is_empty() {
                        "—".into()
                    } else {
                        cols.join(", ")
                    },
                ]);
            }
            let indexes_sec = StructureSection::table(
                "indexes",
                "Indexes",
                vec![
                    "Name".into(),
                    "Unique".into(),
                    "Origin".into(),
                    "Columns".into(),
                ],
                index_rows,
            )
            .with_empty_message("No indexes.");

            let mut fk_stmt = conn
                .prepare(&format!("PRAGMA foreign_key_list('{safe}')"))
                .map_err(query_failed)?;
            let fk_rows: Vec<Vec<String>> = fk_stmt
                .query_map([], |row| {
                    let to_table: String = row.get(2)?;
                    let from_col: String = row.get(3)?;
                    let to_col: String = row.get(4)?;
                    let on_update: String = row.get(5).unwrap_or_else(|_| "—".into());
                    let on_delete: String = row.get(6).unwrap_or_else(|_| "—".into());
                    Ok(vec![from_col, to_table, to_col, on_update, on_delete])
                })
                .map_err(query_failed)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(query_failed)?;
            let fks_sec = StructureSection::table(
                "foreign_keys",
                "Foreign keys",
                vec![
                    "Column".into(),
                    "References".into(),
                    "Ref column".into(),
                    "On update".into(),
                    "On delete".into(),
                ],
                fk_rows,
            )
            .with_empty_message("No foreign keys.");

            let mut trig_stmt = conn
                .prepare(
                    "SELECT name, sql FROM sqlite_master \
                 WHERE type = 'trigger' AND tbl_name = ?1 \
                 ORDER BY name",
                )
                .map_err(query_failed)?;
            let trigger_rows: Vec<Vec<String>> = trig_stmt
                .query_map([&table_owned], |row| {
                    let name: String = row.get(0)?;
                    let sql: Option<String> = row.get(1)?;
                    Ok(vec![name, sql.unwrap_or_else(|| "—".into())])
                })
                .map_err(query_failed)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(query_failed)?;
            let triggers_sec = StructureSection::table(
                "triggers",
                "Triggers",
                vec!["Name".into(), "Definition".into()],
                trigger_rows,
            )
            .with_empty_message("No triggers.");

            let object_kind = kind_owned.unwrap_or_else(|| "table".into());
            Ok(ObjectStructure::new(
                table_owned,
                vec![columns_sec, indexes_sec, fks_sec, triggers_sec],
            )
            .with_subtitle(format!("SQLite · {object_kind}"))
            .with_object_kind(object_kind))
        })
        .await
}
