//! Databases, collections, and inferred field catalogs.

use mongodb::bson::{doc, Bson, Document};
use mystique_connector_api::{
    CatalogColumnRow, CatalogDatabase, CatalogObject, ColumnInfo, ConnectorError, ConnectorResult,
};

use super::encode::bson_type_label;
use super::MongoSession;

pub(super) async fn describe_table(
    session: &MongoSession,
    database: Option<&str>,
    _schema: Option<&str>,
    table: &str,
) -> ConnectorResult<Vec<ColumnInfo>> {
    session.ensure_open()?;
    let db_name = session.resolve_database(database)?;
    let coll = session
        .client
        .database(&db_name)
        .collection::<Document>(table);

    let mut cursor = coll
        .find(doc! {})
        .limit(50)
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?;

    let mut keys: Vec<String> = Vec::new();
    let mut samples: std::collections::HashMap<String, Bson> = std::collections::HashMap::new();
    while cursor
        .advance()
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?
    {
        let d = cursor
            .deserialize_current()
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        for (k, v) in d {
            if !keys.iter().any(|x| x == &k) {
                keys.push(k.clone());
            }
            samples.entry(k).or_insert(v);
        }
    }
    if let Some(i) = keys.iter().position(|k| k == "_id") {
        let id = keys.remove(i);
        keys.insert(0, id);
    }

    Ok(keys
        .into_iter()
        .map(|name| {
            let pk = name == "_id";
            let sample = samples.get(&name);
            ColumnInfo {
                name,
                data_type: bson_type_label(sample),
                nullable: !pk,
                pk,
                enum_values: None,
                // Documents have no column defaults; leave unset.
                default_value: None,
                // No declared FKs in MongoDB.
                references: None,
            }
        })
        .collect())
}

pub(super) async fn list_catalog_columns(
    session: &MongoSession,
    database: Option<&str>,
    _schema: Option<&str>,
) -> ConnectorResult<Vec<CatalogColumnRow>> {
    session.ensure_open()?;
    let db_name = session.resolve_database(database)?;
    let names = session
        .client
        .database(&db_name)
        .list_collection_names()
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?;

    let mut out = Vec::new();
    for name in names {
        let cols = describe_table(session, Some(&db_name), None, &name)
            .await
            .unwrap_or_default();
        for c in cols {
            out.push(CatalogColumnRow {
                database: Some(db_name.clone()),
                schema: None,
                table: name.clone(),
                column: c.name,
                data_type: c.data_type,
                nullable: c.nullable,
                pk: c.pk,
            });
        }
    }
    Ok(out)
}

pub(super) async fn list_catalog(
    session: &MongoSession,
    preferred_database: Option<&str>,
    include_objects: bool,
) -> ConnectorResult<Vec<CatalogDatabase>> {
    session.ensure_open()?;
    let mut names = session
        .client
        .list_database_names()
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?;
    names.sort();
    const SYSTEM: &[&str] = &["admin", "local", "config"];
    let pref = preferred_database.map(str::trim).unwrap_or("");
    names.sort_by(|a, b| {
        let asys = SYSTEM.contains(&a.as_str());
        let bsys = SYSTEM.contains(&b.as_str());
        if asys != bsys {
            return asys.cmp(&bsys);
        }
        if !pref.is_empty() && a == pref {
            return std::cmp::Ordering::Less;
        }
        if !pref.is_empty() && b == pref {
            return std::cmp::Ordering::Greater;
        }
        a.cmp(b)
    });
    if !pref.is_empty() && !names.iter().any(|n| n == pref) {
        names.insert(0, pref.to_string());
    }

    let mut out = Vec::new();
    for db_name in names {
        let is_system = SYSTEM.contains(&db_name.as_str());
        let mut objects = Vec::new();
        if include_objects && !is_system {
            if let Ok(mut colls) = session
                .client
                .database(&db_name)
                .list_collection_names()
                .await
            {
                colls.sort();
                for name in colls {
                    let count = session
                        .client
                        .database(&db_name)
                        .collection::<Document>(&name)
                        .estimated_document_count()
                        .await
                        .unwrap_or(0);
                    objects.push(CatalogObject {
                        name,
                        schema: None,
                        database: Some(db_name.clone()),
                        kind: "collection".into(),
                        row_count: count,
                    });
                }
            }
        }
        let meta = if is_system {
            Some("system".into())
        } else if objects.is_empty() {
            None
        } else {
            Some(format!("{} collections", objects.len()))
        };
        out.push(CatalogDatabase {
            name: db_name,
            meta,
            system: is_system,
            objects,
        });
    }
    Ok(out)
}
