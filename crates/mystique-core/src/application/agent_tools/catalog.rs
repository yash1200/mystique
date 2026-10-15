use crate::error::AppResult;
use crate::storage::sqlite::repo;
use mystique_connector_api::{
    ColumnInfo, ListCatalogColumnsRequest, ListCatalogRequest, ListRelationshipsRequest,
};

use super::super::dbml::catalog_to_dbml;
use super::args::{object_info, profile_database};
use super::policy::ToolCaller;
use super::types::{AgentColumnInfo, AgentConnectionInfo, AgentObjectInfo};
use super::AgentTools;

impl AgentTools {
    pub async fn list_connections(
        &self,
        caller: ToolCaller,
    ) -> AppResult<Vec<AgentConnectionInfo>> {
        let profiles = self.db.with_conn(repo::connections::list_all)?;
        let live: std::collections::HashSet<String> = self
            .connections
            .list_sessions()
            .await
            .into_iter()
            .map(|s| s.session_id)
            .collect();
        let allow = (caller == ToolCaller::Mcp).then(|| self.mcp_allowlist());
        Ok(profiles
            .into_iter()
            .filter(|p| allow.as_ref().is_none_or(|set| set.contains(&p.id)))
            .map(|p| AgentConnectionInfo {
                connected: live.contains(&p.id)
                    || live.contains(&format!("agent:{}", p.id))
                    || live.contains(&format!("mcp:{}", p.id)),
                database: profile_database(&p),
                id: p.id.clone(),
                name: p.name,
                engine: p.engine,
                read_only: p.read_only,
            })
            .collect())
    }

    pub async fn get_schema_dbml(
        &self,
        connection_id: &str,
        database: Option<&str>,
        caller: ToolCaller,
    ) -> AppResult<String> {
        let sid = self.ensure_session_for(connection_id, caller).await?;
        let profile = self
            .db
            .with_conn(|c| repo::connections::get_by_id(c, connection_id.trim()))?;
        let db = database
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or_else(|| profile_database(&profile));
        let columns = self
            .connections
            .list_catalog_columns(ListCatalogColumnsRequest {
                session_id: sid.clone(),
                database: db.clone(),
                schema: None,
            })
            .await?;
        let rels = self
            .connections
            .list_relationships(ListRelationshipsRequest {
                session_id: sid,
                database: db.clone(),
            })
            .await
            .unwrap_or_default();
        Ok(catalog_to_dbml(
            &profile.engine,
            &profile.name,
            db.as_deref(),
            &columns,
            &rels,
        ))
    }

    pub async fn list_objects(
        &self,
        connection_id: &str,
        database: Option<&str>,
        caller: ToolCaller,
    ) -> AppResult<Vec<AgentObjectInfo>> {
        let sid = self.ensure_session_for(connection_id, caller).await?;
        let catalog = self
            .connections
            .list_catalog(ListCatalogRequest {
                session_id: sid,
                preferred_database: database
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                include_objects: true,
            })
            .await?;
        let want = database.map(str::trim).filter(|s| !s.is_empty());
        let mut out = Vec::new();
        for db in catalog {
            if let Some(w) = want {
                if db.name != w {
                    continue;
                }
            }
            for obj in db.objects {
                out.push(object_info(obj));
            }
        }
        Ok(out)
    }

    pub async fn describe_object(
        &self,
        connection_id: &str,
        name: &str,
        schema: Option<&str>,
        database: Option<&str>,
        caller: ToolCaller,
    ) -> AppResult<Vec<AgentColumnInfo>> {
        let sid = self.ensure_session_for(connection_id, caller).await?;
        let cols: Vec<ColumnInfo> = self
            .connections
            .describe_table(mystique_connector_api::DescribeTableRequest {
                session_id: sid,
                table: name.trim().to_string(),
                schema: schema
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                database: database
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
            })
            .await?;
        Ok(cols
            .into_iter()
            .map(|c| AgentColumnInfo {
                name: c.name,
                data_type: c.data_type,
                nullable: c.nullable,
                pk: c.pk,
            })
            .collect())
    }
}
