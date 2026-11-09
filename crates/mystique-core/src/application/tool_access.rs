//! Settings for MCP and agent access levels (F-02).

use crate::domain::{ToolAccessConnection, ToolAccessDatabase, ToolAccessView};
use crate::error::{AppError, AppResult};
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

use super::agent_tools::{
    default_level, is_production, setting_key, AccessLevel, ConnectionLevels, ToolCaller,
};

pub struct ToolAccessService<'a> {
    pub(crate) db: &'a AppDatabase,
}

fn parse_level(raw: &str) -> AppResult<AccessLevel> {
    AccessLevel::parse(raw).ok_or_else(|| {
        AppError::Invalid(format!(
            "unknown access level `{raw}` (none, read, write or full)"
        ))
    })
}

fn parse_caller(raw: &str) -> AppResult<ToolCaller> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "mcp" => Ok(ToolCaller::Mcp),
        "agent" => Ok(ToolCaller::Agent),
        other => Err(AppError::Invalid(format!(
            "unknown caller `{other}` (mcp or agent)"
        ))),
    }
}

impl ToolAccessService<'_> {
    pub fn view(&self) -> AppResult<ToolAccessView> {
        self.db.with_conn(|c| {
            let mcp = default_level(c, ToolCaller::Mcp)?;
            let agent = default_level(c, ToolCaller::Agent)?;
            let rows = repo::tool_access::list_all(c)?;
            let connections = repo::connections::list_all(c)?
                .into_iter()
                .map(|p| {
                    let own: Vec<_> = rows
                        .iter()
                        .filter(|row| row.connection_id == p.id)
                        .cloned()
                        .collect();
                    let levels = ConnectionLevels::from_rows(&own);
                    let production = is_production(&p);
                    let at = |global| {
                        levels
                            .chain(global, production, None)
                            .resolve()
                            .level
                            .as_str()
                            .to_string()
                    };
                    ToolAccessConnection {
                        mcp_level: at(mcp),
                        agent_level: at(agent),
                        default_database: super::agent_tools::profile_default_database(&p),
                        level: levels.connection.map(|l| l.as_str().to_string()),
                        ask_high_risk: levels.ask,
                        databases: levels
                            .databases
                            .iter()
                            .map(|(name, level)| ToolAccessDatabase {
                                name: name.clone(),
                                level: level.as_str().into(),
                            })
                            .collect(),
                        id: p.id,
                        name: p.name,
                        engine: p.engine,
                        environment: p.environment,
                    }
                })
                .collect();
            Ok(ToolAccessView {
                mcp_default: mcp.as_str().into(),
                agent_default: agent.as_str().into(),
                connections,
            })
        })
    }

    /// The global default for `mcp` or `agent`.
    pub fn set_default(&self, caller: &str, level: &str) -> AppResult<ToolAccessView> {
        let caller = parse_caller(caller)?;
        let level = parse_level(level)?;
        self.db
            .with_conn(|c| repo::settings::set(c, setting_key(caller), level.as_str()))?;
        self.view()
    }

    /// A connection's level (`database` empty or absent) or one database's.
    /// `level` absent: inherit again.
    pub fn set_level(
        &self,
        connection_id: &str,
        database: Option<&str>,
        level: Option<&str>,
    ) -> AppResult<ToolAccessView> {
        let level = match level.map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => Some(parse_level(raw)?),
            None => None,
        };
        let database = database.map(str::trim).unwrap_or("");
        self.db.with_conn(|c| {
            // Fails for an unknown connection rather than storing a stray row.
            repo::connections::get_by_id(c, connection_id)?;
            repo::tool_access::set_level(c, connection_id, database, level.map(|l| l.as_str()))
        })?;
        self.view()
    }

    /// "Ask me for high-risk writes" on one connection.
    pub fn set_ask(&self, connection_id: &str, ask: bool) -> AppResult<ToolAccessView> {
        self.db.with_conn(|c| {
            repo::connections::get_by_id(c, connection_id)?;
            repo::tool_access::set_ask(c, connection_id, ask)
        })?;
        self.view()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SaveConnectionInput;
    use crate::AppServices;

    fn app() -> AppServices {
        AppServices::with_secrets(
            AppDatabase::open_in_memory().unwrap(),
            crate::ConnectionManager::bootstrap(),
            crate::SecretStore::memory(),
        )
    }

    fn save(app: &AppServices, name: &str, environment: &str) -> String {
        app.profiles()
            .save(SaveConnectionInput {
                id: None,
                name: name.into(),
                engine: "postgres".into(),
                color_id: "teal".into(),
                environment: environment.into(),
                read_only: false,
                notes: None,
                plugin_config: serde_json::json!({"host": "127.0.0.1", "database": "app"}),
                password: None,
                secret_ref: None,
                clear_secret: false,
                copy_secret_from: None,
                favorite: None,
                ssh_config: crate::domain::SshConfig::default(),
                ssh_password: None,
                clear_ssh_secret: false,
                tls_key_passphrase: None,
                query_timeout_secs: None,
                save_password: None,
            })
            .unwrap()
            .id
    }

    #[test]
    fn the_view_shows_set_and_inherited_levels() {
        let app = app();
        let dev = save(&app, "dev", "local");
        let prod = save(&app, "prod", "production");
        let access = app.tool_access();
        let view = access.view().unwrap();
        assert_eq!(view.mcp_default, "none");
        assert_eq!(view.agent_default, "read");

        let view = access.set_default("mcp", "full").unwrap();
        let card = |view: &ToolAccessView, id: &str| {
            view.connections
                .iter()
                .find(|c| c.id == id)
                .cloned()
                .unwrap()
        };
        assert_eq!(card(&view, &dev).mcp_level, "full");
        assert_eq!(card(&view, &dev).level, None);
        // The global default never writes on production.
        assert_eq!(card(&view, &prod).mcp_level, "read");
        assert_eq!(card(&view, &prod).default_database.as_deref(), Some("app"));

        let view = access.set_level(&prod, None, Some("write")).unwrap();
        assert_eq!(card(&view, &prod).mcp_level, "write");
        assert_eq!(card(&view, &prod).agent_level, "write");
        let view = access
            .set_level(&prod, Some("audit"), Some("none"))
            .unwrap();
        assert_eq!(card(&view, &prod).databases.len(), 1);
        assert_eq!(card(&view, &prod).databases[0].level, "none");

        let view = access.set_ask(&prod, true).unwrap();
        assert!(card(&view, &prod).ask_high_risk);
        // Clearing the level keeps the row for the ask flag.
        let view = access.set_level(&prod, None, None).unwrap();
        assert_eq!(card(&view, &prod).level, None);
        assert!(card(&view, &prod).ask_high_risk);
        let view = access.set_ask(&prod, false).unwrap();
        assert!(!card(&view, &prod).ask_high_risk);

        assert!(access.set_default("mcp", "admin").is_err());
        assert!(access.set_default("cli", "read").is_err());
        assert!(access.set_level(&dev, None, Some("root")).is_err());
        assert!(access.set_level("999", None, Some("read")).is_err());
    }
}
