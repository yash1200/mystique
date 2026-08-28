//! Object structure sections (columns, indexes, FKs, inbound refs, triggers).

use mysql_async::prelude::*;
use mystique_connector_api::{
    cell_bool, ConnectorError, ConnectorResult, ObjectStructure, StructureSection,
};

use super::catalog;
use super::query_failed;
use super::MysqlSession;

pub(super) async fn describe_object_structure(
    session: &MysqlSession,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    kind: Option<&str>,
) -> ConnectorResult<ObjectStructure> {
    session.ensure_open()?;
    let db = database
        .or(schema)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ConnectorError::InvalidConfig("MySQL object structure requires database/schema".into())
        })?;
    let mut conn = session.conn().await?;

    let cols = catalog::describe_table(session, Some(db), schema, table).await?;
    let col_rows: Vec<Vec<String>> = cols
        .iter()
        .map(|c| {
            vec![
                c.name.clone(),
                c.data_type.clone(),
                cell_bool(c.nullable),
                cell_bool(c.pk),
                c.default_value
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "—".into()),
                c.enum_values
                    .as_ref()
                    .map(|v| v.join(", "))
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "—".into()),
            ]
        })
        .collect();
    let columns_sec = StructureSection::table(
        "columns",
        "Columns",
        vec![
            "Name".into(),
            "Type".into(),
            "Nullable".into(),
            "PK".into(),
            "Default".into(),
            "Enum".into(),
        ],
        col_rows,
    )
    .with_empty_message("No columns found.");

    let idx: Vec<(String, String, String, i64, String)> = conn
        .exec_map(
            r#"
            SELECT
              INDEX_NAME,
              COLUMN_NAME,
              NON_UNIQUE,
              SEQ_IN_INDEX,
              INDEX_TYPE
            FROM information_schema.STATISTICS
            WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?
            ORDER BY INDEX_NAME, SEQ_IN_INDEX
            "#,
            (db, table),
            |(name, col, non_unique, seq, itype): (String, String, i64, i64, String)| {
                (
                    name,
                    col,
                    if non_unique == 0 {
                        "yes".into()
                    } else {
                        "no".into()
                    },
                    seq,
                    itype,
                )
            },
        )
        .await
        .map_err(query_failed)?;
    let index_rows: Vec<Vec<String>> = idx
        .into_iter()
        .map(|(name, col, unique, seq, itype)| vec![name, col, unique, seq.to_string(), itype])
        .collect();
    let indexes_sec = StructureSection::table(
        "indexes",
        "Indexes",
        vec![
            "Name".into(),
            "Column".into(),
            "Unique".into(),
            "Seq".into(),
            "Type".into(),
        ],
        index_rows,
    )
    .with_empty_message("No indexes.");

    let fks: Vec<(String, String, String, String, String)> = conn
        .exec_map(
            r#"
            SELECT
              CONSTRAINT_NAME,
              COLUMN_NAME,
              REFERENCED_TABLE_SCHEMA,
              REFERENCED_TABLE_NAME,
              REFERENCED_COLUMN_NAME
            FROM information_schema.KEY_COLUMN_USAGE
            WHERE TABLE_SCHEMA = ?
              AND TABLE_NAME = ?
              AND REFERENCED_TABLE_NAME IS NOT NULL
            ORDER BY CONSTRAINT_NAME, ORDINAL_POSITION
            "#,
            (db, table),
            |(c, col, rschema, rtable, rcol): (String, String, Option<String>, String, String)| {
                (
                    c,
                    col,
                    rschema.unwrap_or_else(|| db.to_string()),
                    rtable,
                    rcol,
                )
            },
        )
        .await
        .map_err(query_failed)?;
    let fk_rows: Vec<Vec<String>> = fks
        .into_iter()
        .map(|(c, col, rschema, rtable, rcol)| vec![c, col, format!("{rschema}.{rtable}"), rcol])
        .collect();
    let fks_sec = StructureSection::table(
        "foreign_keys",
        "Foreign keys",
        vec![
            "Constraint".into(),
            "Column".into(),
            "References".into(),
            "Ref column".into(),
        ],
        fk_rows,
    )
    .with_empty_message("No foreign keys.");

    let inbound: Vec<(String, String, String, String)> = conn
        .exec_map(
            r#"
            SELECT
              CONSTRAINT_NAME,
              TABLE_SCHEMA,
              TABLE_NAME,
              COLUMN_NAME
            FROM information_schema.KEY_COLUMN_USAGE
            WHERE REFERENCED_TABLE_SCHEMA = ?
              AND REFERENCED_TABLE_NAME = ?
            ORDER BY TABLE_SCHEMA, TABLE_NAME
            "#,
            (db, table),
            |(c, tschema, tname, col): (String, String, String, String)| (c, tschema, tname, col),
        )
        .await
        .map_err(query_failed)?;
    let inbound_rows: Vec<Vec<String>> = inbound
        .into_iter()
        .map(|(c, tschema, tname, col)| vec![c, format!("{tschema}.{tname}"), col])
        .collect();
    let refs_sec = StructureSection::table(
        "referenced_by",
        "Referenced by",
        vec![
            "Constraint".into(),
            "From table".into(),
            "From column".into(),
        ],
        inbound_rows,
    )
    .with_empty_message("No inbound foreign keys.");

    let triggers: Vec<(String, String, String, String)> = conn
        .exec_map(
            r#"
            SELECT
              TRIGGER_NAME,
              ACTION_TIMING,
              EVENT_MANIPULATION,
              ACTION_STATEMENT
            FROM information_schema.TRIGGERS
            WHERE EVENT_OBJECT_SCHEMA = ?
              AND EVENT_OBJECT_TABLE = ?
            ORDER BY TRIGGER_NAME
            "#,
            (db, table),
            |(n, t, e, a): (String, String, String, String)| (n, t, e, a),
        )
        .await
        .map_err(query_failed)?;
    let trigger_rows: Vec<Vec<String>> = triggers
        .into_iter()
        .map(|(n, t, e, a)| vec![n, t, e, a])
        .collect();
    let triggers_sec = StructureSection::table(
        "triggers",
        "Triggers",
        vec![
            "Name".into(),
            "Timing".into(),
            "Event".into(),
            "Action".into(),
        ],
        trigger_rows,
    )
    .with_empty_message("No triggers.");

    let object_kind = kind.unwrap_or("table").to_string();
    Ok(ObjectStructure::new(
        format!("{db}.{table}"),
        vec![columns_sec, indexes_sec, fks_sec, refs_sec, triggers_sec],
    )
    .with_subtitle(format!("MySQL · {object_kind}"))
    .with_object_kind(object_kind))
}
