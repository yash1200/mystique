//! Databases, collections, and inferred field catalogs.

use std::collections::HashMap;
use std::sync::Arc;

use mongodb::bson::{doc, Bson, Document};
use mongodb::Client;
use mystique_connector_api::{
    CatalogColumnRow, CatalogDatabase, CatalogObject, ColumnInfo, ConnectorResult,
};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use super::encode::bson_type_label;
use super::query_failed;
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

    let mut cursor = coll.find(doc! {}).limit(50).await.map_err(query_failed)?;

    let mut keys: Vec<String> = Vec::new();
    let mut samples: std::collections::HashMap<String, Bson> = std::collections::HashMap::new();
    while cursor.advance().await.map_err(query_failed)? {
        let d = cursor.deserialize_current().map_err(query_failed)?;
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
        .map_err(query_failed)?;

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
        .map_err(query_failed)?;
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

    let mut pending = Vec::new();
    let mut listed: Vec<(String, bool, Vec<String>)> = Vec::new();
    for db_name in names {
        let is_system = SYSTEM.contains(&db_name.as_str());
        let mut colls = Vec::new();
        if include_objects && !is_system {
            if let Ok(mut names) = session
                .client
                .database(&db_name)
                .list_collection_names()
                .await
            {
                names.sort();
                for name in names {
                    pending.push((db_name.clone(), name.clone()));
                    colls.push(name);
                }
            }
        }
        listed.push((db_name, is_system, colls));
    }
    // Counts overlap, at most 8 at a time. They are not deferred: the tree
    // shows a row count on first open.
    let counts = count_collections(&session.client, pending).await;

    let mut out = Vec::new();
    for (db_name, is_system, colls) in listed {
        let mut objects = Vec::new();
        for name in colls {
            let count = counts
                .get(&(db_name.clone(), name.clone()))
                .copied()
                .unwrap_or(0);
            objects.push(CatalogObject {
                name,
                schema: None,
                database: Some(db_name.clone()),
                kind: "collection".into(),
                row_count: count,
            });
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

/// How many `estimated_document_count` calls run at once.
const COUNT_CONCURRENCY: usize = 8;

async fn count_collections(
    client: &Client,
    jobs: Vec<(String, String)>,
) -> HashMap<(String, String), u64> {
    let counted = map_bounded(jobs, COUNT_CONCURRENCY, |(db, name)| {
        let client = client.clone();
        async move {
            let count = client
                .database(&db)
                .collection::<Document>(&name)
                .estimated_document_count()
                .await
                .unwrap_or(0);
            ((db, name), count)
        }
    })
    .await;
    counted.into_iter().collect()
}

/// Run `f` with at most `limit` futures polled at once.
async fn map_bounded<T, R, F, Fut>(items: Vec<T>, limit: usize, f: F) -> Vec<R>
where
    T: Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = R> + Send + 'static,
{
    let limit = limit.max(1);
    let sem = Arc::new(Semaphore::new(limit));
    let mut set = JoinSet::new();
    for item in items {
        let sem = Arc::clone(&sem);
        let fut = f(item);
        set.spawn(async move {
            let Ok(permit) = sem.acquire_owned().await else {
                return None;
            };
            let value = fut.await;
            drop(permit);
            Some(value)
        });
    }
    let mut out = Vec::new();
    while let Some(joined) = set.join_next().await {
        if let Ok(Some(value)) = joined {
            out.push(value);
        }
    }
    out
}

#[cfg(test)]
mod concurrency_tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use super::map_bounded;

    #[tokio::test]
    async fn collection_counts_overlap_and_stay_bounded() {
        let current = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let items: Vec<u32> = (0..20).collect();
        let current_job = Arc::clone(&current);
        let peak_job = Arc::clone(&peak);
        map_bounded(items, 8, move |_| {
            let current_job = Arc::clone(&current_job);
            let peak_job = Arc::clone(&peak_job);
            async move {
                let now = current_job.fetch_add(1, Ordering::SeqCst) + 1;
                peak_job.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(40)).await;
                current_job.fetch_sub(1, Ordering::SeqCst);
            }
        })
        .await;
        let high = peak.load(Ordering::SeqCst);
        assert!(high > 1, "counts ran one at a time ({high})");
        assert!(high <= 8, "more than 8 counts at once ({high})");
    }
}
