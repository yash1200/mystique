use mystique_connector_api::{
    hidden_schemas, CatalogColumnRow, CatalogDatabase, CatalogRelationship, ColumnInfo,
    ConnectorError, ConnectorResult, DescribeTableRequest, ListCatalogColumnsRequest,
    ListCatalogRequest, ListRelationshipsRequest,
};

use super::depth::{reject_beyond_depth, DepthFeature};
use super::meta_cache::{MetaKey, MetaValue};
use super::ConnectionManager;

impl ConnectionManager {
    /// Column metadata with plugin-owned type labels (e.g. `timestamptz`).
    /// Cached per session; see [`super::meta_cache`].
    pub async fn describe_table(
        &self,
        req: DescribeTableRequest,
    ) -> ConnectorResult<Vec<ColumnInfo>> {
        self.describe_cached(
            &req.session_id,
            req.database.as_deref(),
            req.schema.as_deref(),
            &req.table,
        )
        .await
    }

    pub(super) async fn describe_cached(
        &self,
        session_id: &str,
        database: Option<&str>,
        schema: Option<&str>,
        table: &str,
    ) -> ConnectorResult<Vec<ColumnInfo>> {
        let key = MetaKey::Describe {
            database: database.map(str::to_string),
            schema: schema.map(str::to_string),
            table: table.to_string(),
        };
        if let Some(MetaValue::Describe(columns)) = self.meta.get(session_id, &key) {
            return Ok(columns);
        }
        let session = self.session(session_id).await?;
        let columns = self
            .with_metadata(session.describe_table(database, schema, table))
            .await?;
        // An empty description is usually a table that does not exist yet.
        if !columns.is_empty() {
            self.meta
                .put(session_id, key, MetaValue::Describe(columns.clone()));
        }
        Ok(columns)
    }

    /// Bulk column listing for ER / catalog (plugin-normalized types).
    pub async fn list_catalog_columns(
        &self,
        req: ListCatalogColumnsRequest,
    ) -> ConnectorResult<Vec<CatalogColumnRow>> {
        let key = MetaKey::Columns {
            database: req.database.clone(),
            schema: req.schema.clone(),
        };
        if let Some(MetaValue::Columns(rows)) = self.meta.get(&req.session_id, &key) {
            return Ok(rows);
        }
        let session = self.session(&req.session_id).await?;
        let rows = self
            .with_metadata(
                session.list_catalog_columns(req.database.as_deref(), req.schema.as_deref()),
            )
            .await?;
        self.meta
            .put(&req.session_id, key, MetaValue::Columns(rows.clone()));
        Ok(rows)
    }

    pub async fn list_catalog(
        &self,
        req: ListCatalogRequest,
    ) -> ConnectorResult<Vec<CatalogDatabase>> {
        let key = MetaKey::Catalog {
            preferred: req.preferred_database.clone(),
            include_objects: req.include_objects,
        };
        if let Some(MetaValue::Catalog(databases)) = self.meta.get(&req.session_id, &key) {
            return Ok(databases);
        }
        let meta = self.session_meta(&req.session_id).await?;
        let mut databases = self
            .with_metadata(
                meta.session
                    .list_catalog(req.preferred_database.as_deref(), req.include_objects),
            )
            .await?;
        hide_system_schemas(&meta.product_id, &mut databases);
        self.meta
            .put(&req.session_id, key, MetaValue::Catalog(databases.clone()));
        Ok(databases)
    }

    pub async fn list_relationships(
        &self,
        req: ListRelationshipsRequest,
    ) -> ConnectorResult<Vec<CatalogRelationship>> {
        let meta = self.session_meta(&req.session_id).await?;
        reject_beyond_depth(&meta.caps, DepthFeature::Er)?;
        let key = MetaKey::Relationships {
            database: req.database.clone(),
        };
        if let Some(MetaValue::Relationships(rows)) = self.meta.get(&req.session_id, &key) {
            return Ok(rows);
        }
        let rows = self
            .with_metadata(meta.session.list_relationships(req.database.as_deref()))
            .await?;
        self.meta
            .put(&req.session_id, key, MetaValue::Relationships(rows.clone()));
        Ok(rows)
    }

    /// Explorer refresh, or anything else that knows the catalog changed.
    /// `database` limits the drop to one database; `None` drops the session.
    pub fn invalidate_metadata(&self, session_id: &str, database: Option<&str>) {
        match database.map(str::trim).filter(|db| !db.is_empty()) {
            Some(db) => self.meta.invalidate_database(session_id, db),
            None => self.meta.invalidate(session_id),
        }
    }

    /// Plugin-owned structure (columns, indexes, triggers, relations, …).
    pub async fn describe_object_structure(
        &self,
        req: crate::ObjectStructureRequest,
    ) -> ConnectorResult<crate::ObjectStructure> {
        if req.table.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "Object name is required.".into(),
            ));
        }
        let session = self.session(&req.session_id).await?;
        self.with_metadata(session.describe_object_structure(
            req.database.as_deref(),
            req.schema.as_deref(),
            &req.table,
            req.kind.as_deref(),
        ))
        .await
    }
}

/// Drop product system schemas from an explorer payload.
/// Cockroach schemas live on objects. TiDB system databases are the catalog name.
pub(super) fn hide_system_schemas(product_id: &str, databases: &mut Vec<CatalogDatabase>) {
    let hidden = hidden_schemas(product_id);
    if hidden.is_empty() {
        return;
    }
    databases.retain(|db| !schema_hidden(&db.name, hidden));
    for db in databases.iter_mut() {
        db.objects.retain(|obj| match obj.schema.as_deref() {
            Some(schema) => !schema_hidden(schema, hidden),
            None => true,
        });
    }
}

fn schema_hidden(name: &str, hidden: &[&str]) -> bool {
    hidden.iter().any(|item| name.eq_ignore_ascii_case(item))
}

#[cfg(test)]
mod hide_tests {
    use super::*;
    use mystique_connector_api::CatalogObject;

    fn object(schema: &str, name: &str) -> CatalogObject {
        CatalogObject {
            name: name.into(),
            schema: Some(schema.into()),
            database: Some("defaultdb".into()),
            kind: "table".into(),
            row_count: 0,
        }
    }

    fn database(name: &str) -> CatalogDatabase {
        CatalogDatabase {
            name: name.into(),
            meta: None,
            system: false,
            objects: vec![],
        }
    }

    #[test]
    fn cockroach_explorer_drops_internal_schemas() {
        let mut databases = vec![CatalogDatabase {
            name: "defaultdb".into(),
            meta: None,
            system: false,
            objects: vec![
                object("public", "users"),
                object("crdb_internal", "jobs"),
                object("pg_extension", "spatial_ref_sys"),
            ],
        }];
        hide_system_schemas("cockroachdb", &mut databases);
        let schemas: Vec<_> = databases[0]
            .objects
            .iter()
            .filter_map(|obj| obj.schema.clone())
            .collect();
        assert_eq!(schemas, ["public"]);
    }

    #[test]
    fn tidb_explorer_drops_upper_case_system_schemas() {
        let mut databases = vec![
            database("INFORMATION_SCHEMA"),
            database("METRICS_SCHEMA"),
            database("app"),
        ];
        hide_system_schemas("tidb", &mut databases);
        assert_eq!(
            databases
                .iter()
                .map(|db| db.name.as_str())
                .collect::<Vec<_>>(),
            ["app"]
        );
    }

    #[test]
    fn postgres_keeps_user_schemas() {
        let mut databases = vec![CatalogDatabase {
            name: "app".into(),
            meta: None,
            system: false,
            objects: vec![object("public", "users")],
        }];
        hide_system_schemas("postgres", &mut databases);
        assert_eq!(databases[0].objects.len(), 1);
    }
}
