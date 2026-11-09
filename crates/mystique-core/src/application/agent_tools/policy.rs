//! Access levels applied to the shared tools (F-02).
//!
//! The level model and the decision table are in [`super::levels`]. This
//! file reads the stored levels and applies them to MCP and the built-in
//! agent: the global default per caller, then the connection, then the
//! database. Both callers share the connection and database levels.

use mystique_connector_api::{target_environment, Environment, WriteRisk, WriteTargets};
use rusqlite::Connection;

use crate::domain::ConnectionProfile;
use crate::error::{AppError, AppResult};
use crate::storage::sqlite::repo;

use super::approvals::{ApprovalAsk, ApprovalOutcome, ToolGrant, APPROVAL_TIMEOUT};
use super::args::profile_database;
use super::levels::{decide, no_access_message, AccessLevel, Decision, LevelChain, Target};
use super::AgentTools;

pub use super::levels::ToolCaller;
pub use repo::tool_access::{SETTING_AGENT_LEVEL, SETTING_MCP_LEVEL};

pub(crate) fn setting_key(caller: ToolCaller) -> &'static str {
    match caller {
        ToolCaller::Agent => SETTING_AGENT_LEVEL,
        ToolCaller::Mcp => SETTING_MCP_LEVEL,
    }
}

/// The caller's global default. Unset: the built-in default. Unreadable:
/// `none` (fail closed).
pub(crate) fn default_level(conn: &Connection, caller: ToolCaller) -> AppResult<AccessLevel> {
    Ok(match repo::settings::get(conn, setting_key(caller))? {
        None => caller.default_level(),
        Some(row) => AccessLevel::parse(&row.value).unwrap_or(AccessLevel::None),
    })
}

/// Levels stored for one connection.
#[derive(Debug, Clone, Default)]
pub(crate) struct ConnectionLevels {
    pub connection: Option<AccessLevel>,
    pub databases: Vec<(String, AccessLevel)>,
    pub ask: bool,
}

impl ConnectionLevels {
    pub fn from_rows(rows: &[repo::tool_access::ToolAccessRow]) -> Self {
        let mut out = Self::default();
        for row in rows {
            // An unreadable stored level is `none`, never a grant.
            let level = row
                .level
                .as_deref()
                .map(|raw| AccessLevel::parse(raw).unwrap_or(AccessLevel::None));
            if row.database.is_empty() {
                out.connection = level;
                out.ask = row.ask_high_risk;
            } else if let Some(level) = level {
                out.databases.push((row.database.clone(), level));
            }
        }
        out
    }

    pub fn load(conn: &Connection, connection_id: &str) -> AppResult<Self> {
        Ok(Self::from_rows(&repo::tool_access::for_connection(
            conn,
            connection_id,
        )?))
    }

    pub fn chain(
        &self,
        global: AccessLevel,
        production: bool,
        database: Option<&str>,
    ) -> LevelChain {
        let database = database.map(str::trim).and_then(|db| {
            self.databases
                .iter()
                .find(|(name, _)| name == db)
                .map(|(_, level)| *level)
        });
        LevelChain {
            global,
            connection: self.connection,
            database,
            production,
        }
    }

    /// Listed when the connection, or any database on it, is above `none`.
    pub fn visible(&self, global: AccessLevel, production: bool) -> bool {
        self.chain(global, production, None).resolve().level > AccessLevel::None
            || self
                .databases
                .iter()
                .any(|(_, level)| *level > AccessLevel::None)
    }
}

/// One statement, as the access check sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Assessment {
    /// `Read` only when ReadProof passed; it then runs in Read mode.
    pub risk: WriteRisk,
    pub need: AccessLevel,
    /// The classifier calls it a read but ReadProof could not prove it: why.
    pub unproven: Option<String>,
}

pub(crate) fn is_production(profile: &ConnectionProfile) -> bool {
    profile
        .environment
        .trim()
        .eq_ignore_ascii_case("production")
}

impl AgentTools {
    pub fn default_level(&self, caller: ToolCaller) -> AccessLevel {
        self.db
            .with_conn(|c| default_level(c, caller))
            .unwrap_or(AccessLevel::None)
    }

    /// The profile, if the caller may see it. An unknown id reads the same
    /// as a hidden one.
    pub(super) fn visible_profile(
        &self,
        caller: ToolCaller,
        connection_id: &str,
    ) -> AppResult<(ConnectionProfile, ConnectionLevels)> {
        let id = connection_id.trim();
        let denied = || AppError::AccessDenied(no_access_message(caller, id));
        let found = self.db.with_conn(|c| {
            let profile = match repo::connections::get_by_id(c, id) {
                Ok(profile) => profile,
                Err(AppError::NotFound(_) | AppError::Invalid(_)) => return Ok(None),
                Err(other) => return Err(other),
            };
            let levels = ConnectionLevels::load(c, &profile.id)?;
            Ok(Some((profile, levels, default_level(c, caller)?)))
        })?;
        let Some((profile, levels, global)) = found else {
            return Err(denied());
        };
        if !levels.visible(global, is_production(&profile)) {
            return Err(denied());
        }
        Ok((profile, levels))
    }

    pub fn is_visible(&self, caller: ToolCaller, connection_id: &str) -> bool {
        self.visible_profile(caller, connection_id).is_ok()
    }

    /// Catalog tools and reads: at least `read` on this database.
    pub(super) fn require_read(
        &self,
        caller: ToolCaller,
        connection_id: &str,
        database: Option<&str>,
    ) -> AppResult<ConnectionProfile> {
        let (profile, _) = self.visible_profile(caller, connection_id)?;
        self.require_read_on(caller, &profile, database)?;
        Ok(profile)
    }

    pub(super) fn require_read_on(
        &self,
        caller: ToolCaller,
        profile: &ConnectionProfile,
        database: Option<&str>,
    ) -> AppResult<()> {
        self.authorize_sync(caller, profile, database, None, AccessLevel::Read)
            .map(|_| ())
    }

    /// Allow or deny without asking (reads never ask).
    fn authorize_sync(
        &self,
        caller: ToolCaller,
        profile: &ConnectionProfile,
        database: Option<&str>,
        text: Option<&str>,
        need: AccessLevel,
    ) -> AppResult<Decision> {
        let (effective, ask, db, _) = self.effective(caller, profile, database, text)?;
        let target = Target {
            caller,
            connection_id: &profile.id,
            connection: &profile.name,
            database: db.as_deref(),
        };
        match decide(effective, need, ask, target) {
            Decision::Deny(msg) => Err(AppError::AccessDenied(msg)),
            other => Ok(other),
        }
    }

    /// The resolved level, whether the connection asks, the database, and
    /// whether the write is judged production (F-03 marks).
    fn effective(
        &self,
        caller: ToolCaller,
        profile: &ConnectionProfile,
        database: Option<&str>,
        text: Option<&str>,
    ) -> AppResult<(super::levels::Effective, bool, Option<String>, bool)> {
        let db = database
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| profile_database(profile));
        let (levels, global) = self.db.with_conn(|c| {
            Ok((
                ConnectionLevels::load(c, &profile.id)?,
                default_level(c, caller)?,
            ))
        })?;
        let production = self.production_target(profile, db.as_deref(), text);
        let effective = levels.chain(global, production, db.as_deref()).resolve();
        Ok((effective, levels.ask, db, production))
    }

    /// Production for the access cap, judged like the write guard (F-03):
    /// the most dangerous database the statement writes to, read off its
    /// qualified names (`prod.t`, `USE prod`, `getSiblingDB`). A target that
    /// cannot be named counts as production when any database on the
    /// connection is. Without a statement, the request's database.
    pub(super) fn production_target(
        &self,
        profile: &ConnectionProfile,
        database: Option<&str>,
        text: Option<&str>,
    ) -> bool {
        let connection = Environment::parse(&profile.environment);
        let marks = &profile.database_environments;
        let mut targets = match (text, self.connections.registry().policy(&profile.engine)) {
            (Some(text), Some(policy)) => policy.write_targets(text, database),
            (Some(_), None) => WriteTargets::unresolved(),
            (None, _) => WriteTargets::default(),
        };
        // A read names nothing: judge the database it runs in.
        if targets.is_empty() {
            targets = WriteTargets::current(database);
        }
        target_environment(connection, marks, &targets).environment == Environment::Production
    }

    /// Decide one statement. `Ok(None)` runs it; `Ok(Some(grant))` runs it
    /// once on the user's approval; `Err` is the model-readable refusal.
    pub(super) async fn authorize_statement(
        &self,
        caller: ToolCaller,
        profile: &ConnectionProfile,
        database: Option<&str>,
        text: &str,
        need: AccessLevel,
    ) -> AppResult<Option<ToolGrant>> {
        if self.authorize_sync(caller, profile, database, Some(text), need)? == Decision::Allow {
            return Ok(None);
        }
        // Decision::Ask: the connection asks for high-risk writes.
        let (effective, _, db, production) =
            self.effective(caller, profile, database, Some(text))?;
        let outcome = self
            .approvals
            .request(ApprovalAsk {
                caller,
                connection_id: profile.id.clone(),
                connection_name: profile.name.clone(),
                database: db.clone(),
                environment: if production {
                    "production".into()
                } else {
                    profile.environment.clone()
                },
                sql: text.to_string(),
                needed: need,
                granted: effective.level,
            })
            .await;
        match outcome {
            ApprovalOutcome::Approved(grant) => {
                if grant.covers(caller, &profile.id, db.as_deref(), text) {
                    Ok(Some(grant))
                } else {
                    Err(AppError::AccessDenied(
                        "Access denied: the approval did not match this statement. Nothing ran."
                            .into(),
                    ))
                }
            }
            ApprovalOutcome::Denied => Err(AppError::AccessDenied(
                "Access denied: the user denied this statement in Mystique's approval dock. Nothing ran. Do not retry it unless the user asks you to.".into(),
            )),
            ApprovalOutcome::TimedOut => Err(AppError::AccessDenied(format!(
                "Access denied: nobody approved this statement in Mystique within {} seconds, so it was denied. Nothing ran. Ask the user to watch Mystique's approval dock, then try again.",
                APPROVAL_TIMEOUT.as_secs()
            ))),
        }
    }

    /// True when the caller may write somewhere, so the write tool is listed.
    pub fn may_write(&self, caller: ToolCaller) -> bool {
        self.db
            .with_conn(|c| {
                if default_level(c, caller)? >= AccessLevel::Write {
                    return Ok(true);
                }
                Ok(repo::tool_access::list_all(c)?.iter().any(|row| {
                    row.ask_high_risk
                        || row
                            .level
                            .as_deref()
                            .and_then(AccessLevel::parse)
                            .is_some_and(|level| level >= AccessLevel::Write)
                }))
            })
            .unwrap_or(false)
    }

    /// What the access check needs to know about one statement (F-06).
    ///
    /// `read` access means ReadProof passed: the fail-closed function
    /// allowlist, run in Read mode. Every access decision goes through here.
    pub(super) fn assess(&self, engine: &str, text: &str) -> Assessment {
        let Some(policy) = self.connections.registry().policy(engine) else {
            return Assessment {
                risk: WriteRisk::Write,
                need: AccessLevel::Full,
                unproven: Some("This engine has no query policy.".into()),
            };
        };
        let reason = match policy.read_proof(text) {
            Ok(()) => {
                return Assessment {
                    risk: WriteRisk::Read,
                    need: AccessLevel::Read,
                    unproven: None,
                }
            }
            Err(err) => err.to_string(),
        };
        let risk = policy.classify(text);
        let need = match risk {
            // Looks like a read, but ReadProof cannot prove it (a UDF,
            // nextval, a lock): it runs only where writes are allowed.
            WriteRisk::Read => AccessLevel::Write,
            other => AccessLevel::required_for(other, policy.is_safe_dml(text)),
        };
        Assessment {
            risk: if risk == WriteRisk::Read {
                WriteRisk::Write
            } else {
                risk
            },
            need,
            unproven: (risk == WriteRisk::Read).then_some(reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo::tool_access::ToolAccessRow;

    fn row(db: &str, level: Option<&str>, ask: bool) -> ToolAccessRow {
        ToolAccessRow {
            connection_id: "1".into(),
            database: db.into(),
            level: level.map(str::to_string),
            ask_high_risk: ask,
        }
    }

    #[test]
    fn stored_rows_become_a_chain() {
        let levels = ConnectionLevels::from_rows(&[
            row("", Some("write"), true),
            row("app", Some("full"), false),
            row("audit", Some("none"), false),
            row("odd", Some("admin"), false),
        ]);
        assert!(levels.ask);
        let at = |db: Option<&str>| levels.chain(AccessLevel::Read, false, db).resolve().level;
        assert_eq!(at(None), AccessLevel::Write);
        assert_eq!(at(Some("app")), AccessLevel::Full);
        assert_eq!(at(Some("audit")), AccessLevel::None);
        assert_eq!(at(Some("other")), AccessLevel::Write);
        // An unreadable stored level is none, never a grant.
        assert_eq!(at(Some("odd")), AccessLevel::None);
    }

    #[test]
    fn visible_when_anything_on_it_is_above_none() {
        let hidden = ConnectionLevels::default();
        assert!(!hidden.visible(AccessLevel::None, false));
        assert!(hidden.visible(AccessLevel::Read, false));
        let one_db = ConnectionLevels::from_rows(&[row("app", Some("read"), false)]);
        assert!(one_db.visible(AccessLevel::None, false));
        let off = ConnectionLevels::from_rows(&[row("", Some("none"), false)]);
        assert!(!off.visible(AccessLevel::Full, false));
    }
}
