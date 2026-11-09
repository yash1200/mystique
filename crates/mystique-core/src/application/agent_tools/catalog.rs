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
        let (profiles, rows, global) = self.db.with_conn(|c| {
            Ok((
                repo::connections::list_all(c)?,
                repo::tool_access::list_all(c)?,
                super::policy::default_level(c, caller)?,
            ))
        })?;
        let live: std::collections::HashSet<String> = self
            .connections
            .list_sessions()
            .await
            .into_iter()
            .map(|s| s.session_id)
            .collect();
        let mut out = Vec::new();
        for p in profiles {
            let own: Vec<_> = rows
                .iter()
                .filter(|row| row.connection_id == p.id)
                .cloned()
                .collect();
            let levels = super::policy::ConnectionLevels::from_rows(&own);
            let production = super::policy::is_production(&p);
            // `none` hides the connection, unless a database on it is open.
            if !levels.visible(global, production) {
                continue;
            }
            let access = levels.chain(global, production, None).resolve().level;
            out.push(AgentConnectionInfo {
                connected: live.contains(&p.id)
                    || live.contains(&format!("agent:{}", p.id))
                    || live.contains(&format!("mcp:{}", p.id)),
                database: profile_database(&p),
                id: p.id.clone(),
                name: p.name,
                engine: p.engine,
                // A read-only profile reads, whatever the level says.
                access: if p.read_only {
                    access.min(super::levels::AccessLevel::Read)
                } else {
                    access
                }
                .as_str()
                .into(),
                read_only: p.read_only,
                asks_for_approval: levels.ask && !p.read_only,
            });
        }
        Ok(out)
    }

    pub async fn get_schema_dbml(
        &self,
        connection_id: &str,
        database: Option<&str>,
        caller: ToolCaller,
    ) -> AppResult<String> {
        self.require_read(caller, connection_id, database)?;
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
        let profile = self.require_read(caller, connection_id, database)?;
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
            // A database set to `none` is left out of the listing.
            if self
                .require_read_on(caller, &profile, Some(&db.name))
                .is_err()
            {
                continue;
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
        self.require_read(caller, connection_id, database)?;
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
