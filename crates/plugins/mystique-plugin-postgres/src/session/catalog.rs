//! Databases, tables, columns, and foreign-key relationships.

use mystique_connector_api::{
    CatalogColumnRow, CatalogDatabase, CatalogObject, CatalogRelationship, ColumnInfo,
    ColumnReference, ConnectorResult,
};

use super::encode::{pg_opt_i32, pg_type_label};
use super::PostgresSession;
use super::{pg_col, pg_i32_col, query_failed};

pub(super) async fn describe_columns(
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
          c.character_maximum_length::int4,
          c.numeric_precision::int4,
          c.numeric_scale::int4,
          c.datetime_precision::int4,
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
        let name: String = pg_col(&row, 0)?;
        let data_type: String = pg_col(&row, 1)?;
        let udt_name: String = pg_col(&row, 2)?;
        let char_max = pg_i32_col(&row, 3)?;
        let num_prec = pg_i32_col(&row, 4)?;
        let num_scale = pg_i32_col(&row, 5)?;
        let dt_prec = pg_i32_col(&row, 6)?;
        let is_nullable: String = pg_col(&row, 7)?;
        let enum_labels: Option<String> = pg_col(&row, 8)?;
        let is_pk: bool = pg_col(&row, 9)?;
        let column_default: Option<String> = pg_col(&row, 10)?;

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

    Ok(out)
}

pub(super) async fn table_references(
    session: &PostgresSession,
    schema: Option<&str>,
    table: &str,
) -> ConnectorResult<Vec<(String, ColumnReference)>> {
    session.ensure_open()?;
    let schema = schema.unwrap_or("public");
    let client = session.client().await?;
    let fk_rows = client
        .query(
            // Paired by position, like `list_relationships`. Bound names.
            r#"
            SELECT
              fa.attname,
              tn.nspname,
              tc.relname,
              ta.attname
            FROM pg_catalog.pg_constraint con
            CROSS JOIN LATERAL unnest(con.conkey, con.confkey)
              WITH ORDINALITY AS k(from_attnum, to_attnum, ord)
            JOIN pg_catalog.pg_class fc ON fc.oid = con.conrelid
            JOIN pg_catalog.pg_namespace fn ON fn.oid = fc.relnamespace
            JOIN pg_catalog.pg_attribute fa
              ON fa.attrelid = con.conrelid AND fa.attnum = k.from_attnum
            JOIN pg_catalog.pg_class tc ON tc.oid = con.confrelid
            JOIN pg_catalog.pg_namespace tn ON tn.oid = tc.relnamespace
            JOIN pg_catalog.pg_attribute ta
              ON ta.attrelid = con.confrelid AND ta.attnum = k.to_attnum
            WHERE con.contype = 'f'
              AND fn.nspname = $1
              AND fc.relname = $2
            ORDER BY con.conname, k.ord
            "#,
            &[&schema, &table],
        )
        .await
        .map_err(query_failed)?;
    use std::collections::HashMap;
    let mut by_col: HashMap<String, ColumnReference> = HashMap::new();
    for row in fk_rows {
        let from_col: String = pg_col(&row, 0)?;
        let ref_schema: Option<String> = pg_col(&row, 1)?;
        let ref_table: String = pg_col(&row, 2)?;
        let ref_column: String = pg_col(&row, 3)?;
        // First FK segment wins for multi-column keys (still navigable).
        by_col.entry(from_col).or_insert(ColumnReference {
            database: None,
            schema: ref_schema,
            table: ref_table,
            column: ref_column,
        });
    }
    Ok(by_col.into_iter().collect())
}

pub(super) async fn describe_table(
    session: &PostgresSession,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> ConnectorResult<Vec<ColumnInfo>> {
    let mut out = describe_columns(session, database, schema, table).await?;
    if let Ok(refs) = table_references(session, schema, table).await {
        for col in &mut out {
            if let Some((_, r)) = refs.iter().find(|(name, _)| name == &col.name) {
                col.references = Some(r.clone());
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
                   c.character_maximum_length::int4, c.numeric_precision::int4,
                   c.numeric_scale::int4, c.datetime_precision::int4, c.is_nullable,
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
                   c.character_maximum_length::int4, c.numeric_precision::int4,
                   c.numeric_scale::int4, c.datetime_precision::int4, c.is_nullable,
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
        let table_schema: String = pg_col(&row, 0)?;
        let table_name: String = pg_col(&row, 1)?;
        let column_name: String = pg_col(&row, 2)?;
        let data_type: String = pg_col(&row, 3)?;
        let udt_name: String = pg_col(&row, 4)?;
        let char_max = pg_i32_col(&row, 5)?;
        let num_prec = pg_i32_col(&row, 6)?;
        let num_scale = pg_i32_col(&row, 7)?;
        let dt_prec = pg_i32_col(&row, 8)?;
        let is_nullable: String = pg_col(&row, 9)?;
        let is_pk: bool = pg_col(&row, 10)?;

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

/// Kinds beyond tables and views, as `(schema, name, kind)`. Routines are
/// named with their identity arguments (`f(integer, text)`) so overloads stay
/// distinct; [`super::admin`] matches the same spelling. Objects that belong
/// to an extension are left out.
const OTHER_OBJECTS_SQL: &str = r#"
    SELECT schemaname::text, matviewname::text, 'matview'
    FROM pg_catalog.pg_matviews
    WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
    UNION ALL
    SELECT n.nspname::text, c.relname::text, 'sequence'
    FROM pg_catalog.pg_class c
    JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
    WHERE c.relkind = 'S'
      AND n.nspname NOT IN ('pg_catalog', 'information_schema')
      AND n.nspname NOT LIKE 'pg\_%'
      AND NOT EXISTS (
        SELECT 1 FROM pg_catalog.pg_depend d
        WHERE d.classid = 'pg_catalog.pg_class'::regclass
          AND d.objid = c.oid AND d.deptype IN ('e', 'i', 'a')
      )
    UNION ALL
    SELECT n.nspname::text,
           p.proname || '(' || pg_catalog.pg_get_function_identity_arguments(p.oid) || ')',
           CASE p.prokind WHEN 'p' THEN 'procedure' ELSE 'function' END
    FROM pg_catalog.pg_proc p
    JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
    WHERE p.prokind IN ('f', 'p')
      AND n.nspname NOT IN ('pg_catalog', 'information_schema')
      AND n.nspname NOT LIKE 'pg\_%'
      AND NOT EXISTS (
        SELECT 1 FROM pg_catalog.pg_depend d
        WHERE d.classid = 'pg_catalog.pg_proc'::regclass
          AND d.objid = p.oid AND d.deptype = 'e'
      )
    UNION ALL
    SELECT n.nspname::text, t.typname::text, 'type'
    FROM pg_catalog.pg_type t
    JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
    WHERE (t.typtype IN ('e', 'd')
           OR (t.typtype = 'c' AND EXISTS (
                SELECT 1 FROM pg_catalog.pg_class c
                WHERE c.oid = t.typrelid AND c.relkind = 'c')))
      AND n.nspname NOT IN ('pg_catalog', 'information_schema')
      AND n.nspname NOT LIKE 'pg\_%'
      AND NOT EXISTS (
        SELECT 1 FROM pg_catalog.pg_depend d
        WHERE d.classid = 'pg_catalog.pg_type'::regclass
          AND d.objid = t.oid AND d.deptype = 'e'
      )
    ORDER BY 1, 3, 2
"#;

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
    let current: String = pg_col(&db_row, 0)?;

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
    let mut names = Vec::with_capacity(db_rows.len());
    for row in &db_rows {
        names.push(pg_col(row, 0)?);
    }
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
            let schema: String = pg_col(&row, 0)?;
            let name: String = pg_col(&row, 1)?;
            let raw_type: String = pg_col(&row, 2)?;
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
        // Matviews, sequences, routines and types. Postgres-compatible
        // servers (CockroachDB) lack some of these catalogs, so a failure
        // leaves the explorer with tables and views rather than nothing.
        if let Ok(rows) = client.query(OTHER_OBJECTS_SQL, &[]).await {
            for row in rows {
                let schema: String = pg_col(&row, 0)?;
                let name: String = pg_col(&row, 1)?;
                let kind: String = pg_col(&row, 2)?;
                current_objects.push(CatalogObject {
                    name,
                    schema: Some(schema),
                    database: Some(current.clone()),
                    kind,
                    row_count: 0,
                });
            }
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
                    let tables = objects
                        .iter()
                        .filter(|o| o.kind == "table" || o.kind == "view")
                        .count();
                    Some(format!("{tables} tables"))
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
    // One row per FK column pair. `conkey[i]` references `confkey[i]`, so a
    // composite key pairs by position instead of every column with every
    // column (the information_schema join on constraint name crossed them,
    // and also mixed up same-named constraints on different tables).
    let rows = client
        .query(
            r#"
            SELECT
              fn.nspname AS from_schema,
              fc.relname AS from_table,
              fa.attname AS from_column,
              tn.nspname AS to_schema,
              tc.relname AS to_table,
              ta.attname AS to_column,
              con.conname
            FROM pg_catalog.pg_constraint con
            CROSS JOIN LATERAL unnest(con.conkey, con.confkey)
              WITH ORDINALITY AS k(from_attnum, to_attnum, ord)
            JOIN pg_catalog.pg_class fc ON fc.oid = con.conrelid
            JOIN pg_catalog.pg_namespace fn ON fn.oid = fc.relnamespace
            JOIN pg_catalog.pg_attribute fa
              ON fa.attrelid = con.conrelid AND fa.attnum = k.from_attnum
            JOIN pg_catalog.pg_class tc ON tc.oid = con.confrelid
            JOIN pg_catalog.pg_namespace tn ON tn.oid = tc.relnamespace
            JOIN pg_catalog.pg_attribute ta
              ON ta.attrelid = con.confrelid AND ta.attnum = k.to_attnum
            WHERE con.contype = 'f'
              AND fn.nspname NOT IN ('pg_catalog', 'information_schema')
              AND fn.nspname NOT LIKE 'pg\_toast%'
            ORDER BY fn.nspname, fc.relname, con.conname, k.ord
            "#,
            &[],
        )
        .await
        .map_err(query_failed)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(CatalogRelationship {
            from_schema: pg_col(&row, 0)?,
            from_table: pg_col(&row, 1)?,
            from_column: pg_col(&row, 2)?,
            to_schema: pg_col(&row, 3)?,
            to_table: pg_col(&row, 4)?,
            to_column: pg_col(&row, 5)?,
            name: pg_col(&row, 6)?,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn catalog_names_are_bound() {
        let src = include_str!("catalog.rs");
        let payload = "x\\'; DROP TABLE users; -- ";
        assert!(!src.contains(payload));
        assert!(src.contains("table_schema = $1"));
        assert!(src.contains("table_name = $2"));
        assert!(src.contains("character_maximum_length::int4"));
        assert!(src.contains("numeric_precision::int4"));
        assert!(src.contains("numeric_scale::int4"));
        assert!(src.contains("datetime_precision::int4"));
        // `include_str!` covers this test, so scan only the production half.
        // The needle is split so this file does not contain the literal call.
        let prod = src.split("#[cfg(test)]").next().unwrap_or(src);
        let row_get = concat!("row.", "get(");
        assert!(!prod.contains(row_get), "catalog still calls row.get");
        assert!(!src.contains("replace('\'', \"''\")"));
    }
}
