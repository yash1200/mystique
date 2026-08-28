//! Databases, tables, columns, and foreign-key relationships.

use mystique_connector_api::{
    CatalogColumnRow, CatalogDatabase, CatalogObject, CatalogRelationship, ColumnInfo,
    ColumnReference, ConnectorResult,
};

use super::encode::{pg_opt_i32, pg_type_label};
use super::query_failed;
use super::PostgresSession;

pub(super) async fn describe_table(
    session: &PostgresSession,
    _database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> ConnectorResult<Vec<ColumnInfo>> {
    session.ensure_open()?;
    let schema = schema.unwrap_or("public");
    let client = session.client().await?;

    let sql = r#"
        SELECT
          c.column_name,
          c.data_type,
          c.udt_name,
          c.character_maximum_length,
          c.numeric_precision,
          c.numeric_scale,
          c.datetime_precision,
          c.is_nullable,
          (
            SELECT string_agg(e.enumlabel, E'\x1f' ORDER BY e.enumsortorder)
            FROM pg_catalog.pg_type t
            JOIN pg_catalog.pg_enum e ON e.enumtypid = t.oid
            JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
            WHERE t.typname = c.udt_name
              AND (n.nspname = c.udt_schema OR c.udt_schema IS NULL)
          ) AS enum_labels,
          EXISTS (
            SELECT 1
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
              ON tc.constraint_name = kcu.constraint_name
             AND tc.table_schema = kcu.table_schema
            WHERE tc.constraint_type = 'PRIMARY KEY'
              AND tc.table_schema = c.table_schema
              AND tc.table_name = c.table_name
              AND kcu.column_name = c.column_name
          ) AS is_pk,
          c.column_default
        FROM information_schema.columns c
        WHERE c.table_schema = $1 AND c.table_name = $2
        ORDER BY c.ordinal_position
    "#;

    let rows = client
        .query(sql, &[&schema, &table])
        .await
        .map_err(query_failed)?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let name: String = row.get(0);
        let data_type: String = row.get(1);
        let udt_name: String = row.get(2);
        let char_max: Option<i32> = row.get(3);
        let num_prec: Option<i32> = row.get(4);
        let num_scale: Option<i32> = row.get(5);
        let dt_prec: Option<i32> = row.get(6);
        let is_nullable: String = row.get(7);
        let enum_labels: Option<String> = row.get(8);
        let is_pk: bool = row.get(9);
        let column_default: Option<String> = row.get(10);

        let enum_values = enum_labels.and_then(|labels| {
            let vals: Vec<String> = if labels.contains('\u{001f}') {
                labels
                    .split('\u{001f}')
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            } else {
                labels
                    .trim_matches(|c| c == '{' || c == '}')
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            };
            if vals.is_empty() {
                None
            } else {
                Some(vals)
            }
        });

        out.push(ColumnInfo {
            name,
            data_type: pg_type_label(
                &udt_name,
                &data_type,
                pg_opt_i32(char_max),
                pg_opt_i32(num_prec),
                pg_opt_i32(num_scale),
                pg_opt_i32(dt_prec),
            ),
            nullable: !is_nullable.eq_ignore_ascii_case("NO"),
            pk: is_pk,
            enum_values,
            default_value: column_default
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            references: None,
        });
    }

    // Attach single-column FK targets (plugin-owned navigation for the grid).
    if let Ok(fk_rows) = client
        .query(
            r#"
            SELECT
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
    {
        use std::collections::HashMap;
        let mut by_col: HashMap<String, ColumnReference> = HashMap::new();
        for row in fk_rows {
            let from_col: String = row.get(0);
            // First FK segment wins for multi-column keys (still navigable).
            by_col.entry(from_col).or_insert(ColumnReference {
                database: None,
                schema: row.get::<_, Option<String>>(1),
                table: row.get(2),
                column: row.get(3),
            });
        }
        for col in &mut out {
            if let Some(r) = by_col.remove(&col.name) {
                col.references = Some(r);
            }
        }
    }

    Ok(out)
}

pub(super) async fn list_catalog_columns(
    session: &PostgresSession,
    _database: Option<&str>,
    schema: Option<&str>,
) -> ConnectorResult<Vec<CatalogColumnRow>> {
    session.ensure_open()?;
    let client = session.client().await?;

    let (sql, params_schema): (&str, Option<String>) = if let Some(s) = schema {
        (
            r#"
            SELECT c.table_schema, c.table_name, c.column_name,
                   c.data_type, c.udt_name,
                   c.character_maximum_length, c.numeric_precision,
                   c.numeric_scale, c.datetime_precision, c.is_nullable,
                   EXISTS (
                     SELECT 1
                     FROM information_schema.table_constraints tc
                     JOIN information_schema.key_column_usage kcu
                       ON tc.constraint_name = kcu.constraint_name
                      AND tc.table_schema = kcu.table_schema
                     WHERE tc.constraint_type = 'PRIMARY KEY'
                       AND tc.table_schema = c.table_schema
                       AND tc.table_name = c.table_name
                       AND kcu.column_name = c.column_name
                   ) AS is_pk
            FROM information_schema.columns c
            WHERE c.table_schema = $1
            ORDER BY c.table_schema, c.table_name, c.ordinal_position
            "#,
            Some(s.to_string()),
        )
    } else {
        (
            r#"
            SELECT c.table_schema, c.table_name, c.column_name,
                   c.data_type, c.udt_name,
                   c.character_maximum_length, c.numeric_precision,
                   c.numeric_scale, c.datetime_precision, c.is_nullable,
                   EXISTS (
                     SELECT 1
                     FROM information_schema.table_constraints tc
                     JOIN information_schema.key_column_usage kcu
                       ON tc.constraint_name = kcu.constraint_name
                      AND tc.table_schema = kcu.table_schema
                     WHERE tc.constraint_type = 'PRIMARY KEY'
                       AND tc.table_schema = c.table_schema
                       AND tc.table_name = c.table_name
                       AND kcu.column_name = c.column_name
                   ) AS is_pk
            FROM information_schema.columns c
            WHERE c.table_schema NOT IN ('pg_catalog', 'information_schema')
            ORDER BY c.table_schema, c.table_name, c.ordinal_position
            "#,
            None,
        )
    };

    let rows = if let Some(ref sch) = params_schema {
        client.query(sql, &[sch]).await.map_err(query_failed)?
    } else {
        client.query(sql, &[]).await.map_err(query_failed)?
    };

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let table_schema: String = row.get(0);
        let table_name: String = row.get(1);
        let column_name: String = row.get(2);
        let data_type: String = row.get(3);
        let udt_name: String = row.get(4);
        let char_max: Option<i32> = row.get(5);
        let num_prec: Option<i32> = row.get(6);
        let num_scale: Option<i32> = row.get(7);
        let dt_prec: Option<i32> = row.get(8);
        let is_nullable: String = row.get(9);
        let is_pk: bool = row.get(10);

        out.push(CatalogColumnRow {
            database: None,
            schema: Some(table_schema),
            table: table_name,
            column: column_name,
            data_type: pg_type_label(
                &udt_name, &data_type, char_max, num_prec, num_scale, dt_prec,
            ),
            nullable: !is_nullable.eq_ignore_ascii_case("NO"),
            pk: is_pk,
        });
    }
    Ok(out)
}

pub(super) async fn list_catalog(
    session: &PostgresSession,
    preferred_database: Option<&str>,
    include_objects: bool,
) -> ConnectorResult<Vec<CatalogDatabase>> {
    session.ensure_open()?;
    let client = session.client().await?;
    let db_row = client
        .query_one("SELECT current_database()", &[])
        .await
        .map_err(query_failed)?;
    let current: String = db_row.get(0);

    // Postgres connections are per-database; still list every connectable
    // database on the server so the explorer picker can switch.
    let db_rows = client
        .query(
            r#"
            SELECT datname
            FROM pg_database
            WHERE datallowconn
              AND NOT datistemplate
              AND has_database_privilege(datname, 'CONNECT')
            ORDER BY datname
            "#,
            &[],
        )
        .await
        .map_err(query_failed)?;
    let mut names: Vec<String> = db_rows.iter().map(|row| row.get(0)).collect();
    if !names.iter().any(|n| n == &current) {
        names.push(current.clone());
    }
    if let Some(pref) = preferred_database.map(str::trim).filter(|s| !s.is_empty()) {
        if !names.iter().any(|n| n == pref) {
            names.push(pref.to_string());
        }
    }

    let mut current_objects = Vec::new();
    if include_objects {
        let table_rows = client
            .query(
                r#"
                SELECT table_schema, table_name, table_type
                FROM information_schema.tables
                WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
                ORDER BY table_schema, table_name
                "#,
                &[],
            )
            .await
            .map_err(query_failed)?;
        for row in table_rows {
            let schema: String = row.get(0);
            let name: String = row.get(1);
            let raw_type: String = row.get(2);
            let kind = if raw_type.to_ascii_lowercase().contains("view") {
                "view"
            } else {
                "table"
            };
            current_objects.push(CatalogObject {
                name,
                schema: Some(schema),
                database: Some(current.clone()),
                kind: kind.into(),
                row_count: 0,
            });
        }
    }

    let pref = preferred_database.map(str::trim).unwrap_or("");
    names.sort_by(|a, b| {
        if !pref.is_empty() && a == pref {
            return std::cmp::Ordering::Less;
        }
        if !pref.is_empty() && b == pref {
            return std::cmp::Ordering::Greater;
        }
        if a == &current {
            return std::cmp::Ordering::Less;
        }
        if b == &current {
            return std::cmp::Ordering::Greater;
        }
        a.cmp(b)
    });

    Ok(names
        .into_iter()
        .map(|name| {
            let is_current = name == current;
            let objects = if is_current {
                current_objects.clone()
            } else {
                Vec::new()
            };
            let meta = if is_current {
                if objects.is_empty() {
                    Some("empty".into())
                } else {
                    Some(format!("{} tables", objects.len()))
                }
            } else {
                None
            };
            CatalogDatabase {
                name,
                meta,
                system: false,
                objects,
            }
        })
        .collect())
}

pub(super) async fn list_relationships(
    session: &PostgresSession,
    _database: Option<&str>,
) -> ConnectorResult<Vec<CatalogRelationship>> {
    session.ensure_open()?;
    let client = session.client().await?;
    let rows = client
        .query(
            r#"
            SELECT
              kcu.table_schema AS from_schema,
              kcu.table_name AS from_table,
              kcu.column_name AS from_column,
              ccu.table_schema AS to_schema,
              ccu.table_name AS to_table,
              ccu.column_name AS to_column,
              tc.constraint_name
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
              ON tc.constraint_name = kcu.constraint_name
             AND tc.table_schema = kcu.table_schema
            JOIN information_schema.constraint_column_usage ccu
              ON ccu.constraint_name = tc.constraint_name
             AND ccu.table_schema = tc.table_schema
            WHERE tc.constraint_type = 'FOREIGN KEY'
            ORDER BY kcu.table_schema, kcu.table_name
            "#,
            &[],
        )
        .await
        .map_err(query_failed)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(CatalogRelationship {
            from_schema: row.get::<_, Option<String>>(0),
            from_table: row.get(1),
            from_column: row.get(2),
            to_schema: row.get::<_, Option<String>>(3),
            to_table: row.get(4),
            to_column: row.get(5),
            name: row.get::<_, Option<String>>(6),
        });
    }
    Ok(out)
}
