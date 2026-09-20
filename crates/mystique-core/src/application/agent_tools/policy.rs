//! Shared MCP / in-app agent write policy (P3.2).
//!
//! Stored as `mcp.policy` / `mcp.allowlist`. The in-app agent uses the same
//! policy object if write tools are added; the allowlist is MCP-only.

use std::collections::HashSet;

use mystique_connector_api::{mongo_is_safe_dml, runtime_id, sql_is_safe_dml, WriteRisk};

use crate::error::{AppError, AppResult};
use crate::storage::sqlite::repo;

use super::AgentTools;

pub const SETTING_MCP_POLICY: &str = "mcp.policy";
pub const SETTING_MCP_ALLOWLIST: &str = "mcp.allowlist";

/// Who is calling [`AgentTools::call_tool`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCaller {
    /// In-app query-tab agent. Bound to the open connection; no MCP allowlist.
    Agent,
    /// Localhost MCP HTTP server. Allowlist applies (empty = none).
    Mcp,
}

/// Global write ceiling for agent/MCP query tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentWritePolicy {
    /// SELECT / find only. Default.
    #[default]
    ReadOnly,
    /// INSERT / UPDATE (and Mongo insert/update). No DELETE, DDL, or DROP.
    SafeWrite,
    /// DDL / DROP / DELETE. Production still hits the P3.1 write guard.
    HighRiskWrite,
}

impl AgentWritePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::SafeWrite => "safe_write",
            Self::HighRiskWrite => "high_risk_write",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "safe_write" => Self::SafeWrite,
            "high_risk_write" => Self::HighRiskWrite,
            _ => Self::ReadOnly,
        }
    }

    pub fn allows_writes(self) -> bool {
        !matches!(self, Self::ReadOnly)
    }

    /// `safe_dml` is INSERT/UPDATE (see [`sql_is_safe_dml`] / [`mongo_is_safe_dml`]).
    pub fn allows(self, risk: WriteRisk, safe_dml: bool) -> bool {
        match self {
            Self::ReadOnly => risk == WriteRisk::Read,
            Self::SafeWrite => risk == WriteRisk::Read || (risk == WriteRisk::Write && safe_dml),
            Self::HighRiskWrite => true,
        }
    }

    pub fn deny_message(self, risk: WriteRisk) -> String {
        match self {
            Self::ReadOnly => {
                "Write policy is read-only. INSERT and other writes are blocked.".into()
            }
            Self::SafeWrite if risk == WriteRisk::Destructive => {
                "Write policy is safe_write. DROP, TRUNCATE, and other high-risk statements are blocked.".into()
            }
            Self::SafeWrite => {
                "Write policy is safe_write. Only INSERT/UPDATE (and Mongo insert/update) are allowed.".into()
            }
            Self::HighRiskWrite => "Write policy denied this statement.".into(),
        }
    }
}

/// JSON array of connection ids. Invalid JSON → empty (fail closed: none, not all).
pub fn parse_allowlist(raw: &str) -> Vec<String> {
    let parsed: Vec<String> = serde_json::from_str(raw.trim()).unwrap_or_default();
    parsed
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn is_safe_dml(engine: &str, text: &str) -> bool {
    match runtime_id(engine).unwrap_or(engine.trim()) {
        "mongodb" => mongo_is_safe_dml(text),
        _ => sql_is_safe_dml(text),
    }
}

impl AgentTools {
    pub fn write_policy(&self) -> AgentWritePolicy {
        self.db
            .with_conn(|c| repo::settings::get(c, SETTING_MCP_POLICY))
            .ok()
            .flatten()
            .map(|s| AgentWritePolicy::parse(&s.value))
            .unwrap_or_default()
    }

    pub fn mcp_allowlist(&self) -> HashSet<String> {
        self.db
            .with_conn(|c| repo::settings::get(c, SETTING_MCP_ALLOWLIST))
            .ok()
            .flatten()
            .map(|s| parse_allowlist(&s.value).into_iter().collect())
            .unwrap_or_default()
    }

    pub fn require_allowlisted(&self, caller: ToolCaller, connection_id: &str) -> AppResult<()> {
        if caller != ToolCaller::Mcp {
            return Ok(());
        }
        if self.mcp_allowlist().contains(connection_id) {
            Ok(())
        } else {
            Err(AppError::Invalid(
                "This connection is not on the MCP allowlist.".into(),
            ))
        }
    }

    pub(super) fn classify_text(&self, engine: &str, text: &str) -> WriteRisk {
        self.connections
            .registry()
            .policy(engine)
            .map(|p| p.classify(text))
            .unwrap_or(WriteRisk::Write)
    }

    pub(super) fn enforce_write_policy(&self, risk: WriteRisk, safe_dml: bool) -> AppResult<()> {
        let policy = self.write_policy();
        if policy.allows(risk, safe_dml) {
            Ok(())
        } else {
            Err(AppError::Invalid(policy.deny_message(risk)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_defaults_unknown_to_read_only() {
        assert_eq!(AgentWritePolicy::parse(""), AgentWritePolicy::ReadOnly);
        assert_eq!(
            AgentWritePolicy::parse("read-only"),
            AgentWritePolicy::ReadOnly
        );
        assert_eq!(
            AgentWritePolicy::parse("read_only"),
            AgentWritePolicy::ReadOnly
        );
    }

    #[test]
    fn parse_accepts_hyphen_and_underscore() {
        assert_eq!(
            AgentWritePolicy::parse("safe-write"),
            AgentWritePolicy::SafeWrite
        );
        assert_eq!(
            AgentWritePolicy::parse("high_risk_write"),
            AgentWritePolicy::HighRiskWrite
        );
    }

    #[test]
    fn read_only_allows_only_reads() {
        let p = AgentWritePolicy::ReadOnly;
        assert!(p.allows(WriteRisk::Read, false));
        assert!(!p.allows(WriteRisk::Write, true));
        assert!(!p.allows(WriteRisk::Destructive, false));
    }

    #[test]
    fn safe_write_allows_insert_not_drop_or_delete() {
        let p = AgentWritePolicy::SafeWrite;
        assert!(p.allows(WriteRisk::Write, true));
        assert!(!p.allows(WriteRisk::Write, false));
        assert!(!p.allows(WriteRisk::Destructive, true));
    }

    #[test]
    fn high_risk_write_allows_destructive() {
        assert!(AgentWritePolicy::HighRiskWrite.allows(WriteRisk::Destructive, false));
    }

    #[test]
    fn empty_or_invalid_allowlist_is_none() {
        assert!(parse_allowlist("").is_empty());
        assert!(parse_allowlist("not-json").is_empty());
        assert!(parse_allowlist("[]").is_empty());
    }

    #[test]
    fn allowlist_parses_json_ids() {
        assert_eq!(
            parse_allowlist(r#"["a"," b ",""]"#),
            vec!["a".to_string(), "b".to_string()]
        );
    }
}
