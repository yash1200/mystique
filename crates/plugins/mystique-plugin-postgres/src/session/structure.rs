//! Object structure sections (columns, indexes, FKs, triggers, checks).

use mystique_connector_api::{cell_bool, ConnectorResult, ObjectStructure, StructureSection};

use super::catalog;
use super::query_failed;
use super::PostgresSession;

pub(super) async fn describe_object_structure(
    session: &PostgresSession,
    _database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    kind: Option<&str>,
) -> ConnectorResult<ObjectStructure> {
    session.ensure_open()?;
    let schema = schema.unwrap_or("public");
    let client = session.client().await?;

    let cols = catalog::describe_table(session, None, Some(schema), table).await?;
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
    let mut columns_sec = StructureSection::table(
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
    );
    if columns_sec.rows.is_empty() {
        columns_sec = columns_sec.with_empty_message("No columns found.");
    }

    let idx_rows = client
        .query(
            r#"
            SELECT
              i.relname::text,
              ix.indisunique,
              ix.indisprimary,
              pg_get_indexdef(ix.indexrelid)
            FROM pg_class t
            JOIN pg_index ix ON t.oid = ix.indrelid
            JOIN pg_class i ON i.oid = ix.indexrelid
            JOIN pg_namespace n ON n.oid = t.relnamespace
            WHERE n.nspname = $1 AND t.relname = $2
            ORDER BY i.relname
            "#,
            &[&schema, &table],
        )
        .await
        .map_err(query_failed)?;
    let index_rows: Vec<Vec<String>> = idx_rows
        .iter()
        .map(|r| {
            vec![
                r.get::<_, String>(0),
                cell_bool(r.get::<_, bool>(1)),
                cell_bool(r.get::<_, bool>(2)),
                r.get::<_, String>(3),
            ]
        })
        .collect();
    let indexes_sec = StructureSection::table(
        "indexes",
        "Indexes",
        vec![
            "Name".into(),
            "Unique".into(),
            "Primary".into(),
            "Definition".into(),
        ],
        index_rows,
    )
    .with_empty_message("No indexes.");

    let fk_rows = client
        .query(
            r#"
            SELECT
              tc.constraint_name,
              kcu.column_name,
              ccu.table_schema,
              ccu.table_name,
              ccu.column_name
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
              ON tc.constraint_name = kcu.constraint_name
             AND tc.table_schema = kcu.table_schema
            JOIN information_schema.constraint_column_usage ccu
              ON ccu.constraint_name = tc.constraint_name
             AND ccu.table_schema = tc.table_schema
            WHERE tc.constraint_type = 'FOREIGN KEY'
              AND tc.table_schema = $1
              AND tc.table_name = $2
            ORDER BY tc.constraint_name, kcu.ordinal_position
            "#,
            &[&schema, &table],
        )
        .await
        .map_err(query_failed)?;
    let fk_display: Vec<Vec<String>> = fk_rows
        .iter()
        .map(|r| {
            let to_schema: String = r.get(2);
            let to_table: String = r.get(3);
            vec![
                r.get::<_, String>(0),
                r.get::<_, String>(1),
                format!("{to_schema}.{to_table}"),
                r.get::<_, String>(4),
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
        fk_display,
    )
    .with_empty_message("No foreign keys.");

    let inbound = client
        .query(
            r#"
            SELECT
              tc.constraint_name,
              kcu.table_schema,
              kcu.table_name,
              kcu.column_name,
              ccu.column_name
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
              ON tc.constraint_name = kcu.constraint_name
             AND tc.table_schema = kcu.table_schema
            JOIN information_schema.constraint_column_usage ccu
              ON ccu.constraint_name = tc.constraint_name
             AND ccu.table_schema = tc.table_schema
            WHERE tc.constraint_type = 'FOREIGN KEY'
              AND ccu.table_schema = $1
              AND ccu.table_name = $2
            ORDER BY kcu.table_schema, kcu.table_name
            "#,
            &[&schema, &table],
        )
        .await
        .map_err(query_failed)?;
    let inbound_display: Vec<Vec<String>> = inbound
        .iter()
        .map(|r| {
            let from_schema: String = r.get(1);
            let from_table: String = r.get(2);
            vec![
                r.get::<_, String>(0),
                format!("{from_schema}.{from_table}"),
                r.get::<_, String>(3),
                r.get::<_, String>(4),
            ]
        })
        .collect();
    let refs_sec = StructureSection::table(
        "referenced_by",
        "Referenced by",
        vec![
            "Constraint".into(),
            "From table".into(),
            "From column".into(),
            "To column".into(),
        ],
        inbound_display,
    )
    .with_empty_message("No inbound foreign keys.");

    let trig_rows = client
        .query(
            r#"
            SELECT
              trigger_name,
              action_timing,
              event_manipulation,
              action_statement
            FROM information_schema.triggers
            WHERE event_object_schema = $1
              AND event_object_table = $2
            ORDER BY trigger_name, event_manipulation
            "#,
            &[&schema, &table],
        )
        .await
        .map_err(query_failed)?;
    let trigger_display: Vec<Vec<String>> = trig_rows
        .iter()
        .map(|r| {
            vec![
                r.get::<_, String>(0),
                r.get::<_, String>(1),
                r.get::<_, String>(2),
                r.get::<_, String>(3),
            ]
        })
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
        trigger_display,
    )
    .with_empty_message("No triggers.");

    let check_rows = client
        .query(
            r#"
            SELECT
              tc.constraint_name,
              cc.check_clause
            FROM information_schema.table_constraints tc
            JOIN information_schema.check_constraints cc
              ON tc.constraint_name = cc.constraint_name
             AND tc.constraint_schema = cc.constraint_schema
            WHERE tc.constraint_type = 'CHECK'
              AND tc.table_schema = $1
              AND tc.table_name = $2
            ORDER BY tc.constraint_name
            "#,
            &[&schema, &table],
        )
        .await
        .map_err(query_failed)?;
    let check_display: Vec<Vec<String>> = check_rows
        .iter()
        .map(|r| vec![r.get::<_, String>(0), r.get::<_, String>(1)])
        .collect();
    let checks_sec = StructureSection::table(
        "checks",
        "Check constraints",
        vec!["Name".into(), "Clause".into()],
        check_display,
    )
    .with_empty_message("No check constraints.");

    let title = format!("{schema}.{table}");
    let object_kind = kind.unwrap_or("table").to_string();
    Ok(ObjectStructure::new(
        title,
        vec![
            columns_sec,
            indexes_sec,
            fks_sec,
            refs_sec,
            triggers_sec,
            checks_sec,
        ],
    )
    .with_subtitle(format!("PostgreSQL · {object_kind}"))
    .with_object_kind(object_kind))
}
