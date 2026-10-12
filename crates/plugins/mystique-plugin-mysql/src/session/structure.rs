//! Object structure sections (columns, indexes, FKs, inbound refs, triggers).

use mystique_connector_api::{
    cell_bool, ConnectorError, ConnectorResult, ObjectStructure, StructureSection,
};

use super::catalog;
use super::MysqlSession;

/// One `information_schema.STATISTICS` row. `column` is `None` for a functional
/// index, whose `COLUMN_NAME` is NULL.
pub(crate) struct MysqlIndexColumn {
    pub name: String,
    pub column: Option<String>,
    pub unique: String,
    pub seq: i64,
    pub index_type: String,
}

pub(crate) fn decode_mysql_index_column(
    name: String,
    column: Option<String>,
    non_unique: i64,
    seq: i64,
    index_type: String,
) -> MysqlIndexColumn {
    MysqlIndexColumn {
        name,
        column,
        unique: if non_unique == 0 {
            "yes".into()
        } else {
            "no".into()
        },
        seq,
        index_type,
    }
}

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

    let idx: Vec<(String, Option<String>, i64, i64, String)> = crate::rows::collect_exec(
        &mut conn,
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
    )
    .await?;
    let index_rows: Vec<Vec<String>> = idx
        .into_iter()
        .map(|(name, col, non_unique, seq, itype)| {
            let decoded = decode_mysql_index_column(name, col, non_unique, seq, itype);
            vec![
                decoded.name,
                decoded.column.unwrap_or_default(),
                decoded.unique,
                decoded.seq.to_string(),
                decoded.index_type,
            ]
        })
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

    let fks: Vec<(
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = crate::rows::collect_exec(
        &mut conn,
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
    )
    .await?;
    let fk_rows: Vec<Vec<String>> = fks
        .into_iter()
        .map(|(c, col, rschema, rtable, rcol)| {
            let rschema = rschema.unwrap_or_else(|| db.to_string());
            let rtable = rtable.unwrap_or_default();
            vec![
                c,
                col.unwrap_or_default(),
                format!("{rschema}.{rtable}"),
                rcol.unwrap_or_default(),
            ]
        })
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

    let inbound: Vec<(String, String, String, Option<String>)> = crate::rows::collect_exec(
        &mut conn,
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
    )
    .await?;
    let inbound_rows: Vec<Vec<String>> = inbound
        .into_iter()
        .map(|(c, tschema, tname, col)| {
            vec![c, format!("{tschema}.{tname}"), col.unwrap_or_default()]
        })
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

    let triggers: Vec<(String, String, String, String)> = crate::rows::collect_exec(
        &mut conn,
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
    )
    .await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_index_column_name_stays_null() {
        let col = decode_mysql_index_column("idx_expr".into(), None, 1, 1, "BTREE".into());
        assert!(col.column.is_none());
        let cell = col.column.unwrap_or_default();
        assert_eq!(cell, "");
    }
}
