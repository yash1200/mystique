use mystique_connector_api::{
    CatalogColumnRow, CatalogDatabase, CatalogRelationship, ColumnInfo, ConnectorError,
    ConnectorResult, DescribeTableRequest, ListCatalogColumnsRequest, ListCatalogRequest,
    ListRelationshipsRequest,
};

use super::ConnectionManager;

impl ConnectionManager {
    /// Column metadata with plugin-owned type labels (e.g. `timestamptz`).
    pub async fn describe_table(
        &self,
        req: DescribeTableRequest,
    ) -> ConnectorResult<Vec<ColumnInfo>> {
        let session = self.session(&req.session_id).await?;
        session
            .describe_table(req.database.as_deref(), req.schema.as_deref(), &req.table)
            .await
    }

    /// Bulk column listing for ER / catalog (plugin-normalized types).
    pub async fn list_catalog_columns(
        &self,
        req: ListCatalogColumnsRequest,
    ) -> ConnectorResult<Vec<CatalogColumnRow>> {
        let session = self.session(&req.session_id).await?;
        session
            .list_catalog_columns(req.database.as_deref(), req.schema.as_deref())
            .await
    }
    pub async fn list_catalog(
        &self,
        req: ListCatalogRequest,
    ) -> ConnectorResult<Vec<CatalogDatabase>> {
        let session = self.session(&req.session_id).await?;
        session
            .list_catalog(req.preferred_database.as_deref(), req.include_objects)
            .await
    }

    pub async fn list_relationships(
        &self,
        req: ListRelationshipsRequest,
    ) -> ConnectorResult<Vec<CatalogRelationship>> {
        let session = self.session(&req.session_id).await?;
        session.list_relationships(req.database.as_deref()).await
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
        session
            .describe_object_structure(
                req.database.as_deref(),
                req.schema.as_deref(),
                &req.table,
                req.kind.as_deref(),
            )
            .await
    }
}
