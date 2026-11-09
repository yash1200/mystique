//! Access levels for MCP and the built-in agent (F-02).
//!
//! `none < read < write < full`, set globally, per connection, and per
//! database. The most specific level that is set wins; an unset level
//! inherits. Pure: no storage, no sessions. [`decide`] is the one place a
//! tool call is allowed, denied, or sent to the approval dock.

use mystique_connector_api::WriteRisk;

/// How far a tool principal may go on a connection or database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccessLevel {
    /// Not visible: the connection is left out of `list_connections`.
    None,
    /// Read mode only.
    Read,
    /// Non-destructive writes: INSERT, UPDATE, MERGE, REPLACE (Mongo insert/update).
    Write,
    /// Everything, including DELETE, DDL and DROP.
    Full,
}

impl AccessLevel {
    pub const ALL: [Self; 4] = [Self::None, Self::Read, Self::Write, Self::Full];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Read => "read",
            Self::Write => "write",
            Self::Full => "full",
        }
    }

    /// Strict: an unknown value is `None` (the caller decides what that means).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "read" => Some(Self::Read),
            "write" => Some(Self::Write),
            "full" => Some(Self::Full),
            _ => None,
        }
    }

    /// What a statement of this risk needs. `safe_dml` is the engine policy's
    /// INSERT / UPDATE check; any other write needs `full`.
    pub fn required_for(risk: WriteRisk, safe_dml: bool) -> Self {
        match risk {
            WriteRisk::Read => Self::Read,
            WriteRisk::Write if safe_dml => Self::Write,
            WriteRisk::Write | WriteRisk::Destructive => Self::Full,
        }
    }

    fn what(self) -> &'static str {
        match self {
            Self::None => "no access",
            Self::Read => "reads only",
            Self::Write => "INSERT and UPDATE",
            Self::Full => "DELETE, DDL, DROP and other destructive statements",
        }
    }
}

/// Who is calling the shared tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolCaller {
    /// The built-in agent in a query tab.
    Agent,
    /// The localhost MCP server.
    Mcp,
}

impl ToolCaller {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Mcp => "mcp",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Agent => "The built-in agent",
            Self::Mcp => "MCP",
        }
    }

    /// The fallback when nothing is set: MCP sees nothing, the agent reads.
    pub fn default_level(self) -> AccessLevel {
        match self {
            Self::Agent => AccessLevel::Read,
            Self::Mcp => AccessLevel::None,
        }
    }
}

/// Where the effective level came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelSource {
    Global,
    Connection,
    Database,
}

impl LevelSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Connection => "connection",
            Self::Database => "database",
        }
    }
}

/// The levels set at each scope. `None` = inherit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelChain {
    pub global: AccessLevel,
    pub connection: Option<AccessLevel>,
    pub database: Option<AccessLevel>,
    /// The write lands in production: the connection, or a database marked
    /// production that the statement writes to (unresolved counts if any is).
    pub production: bool,
}

/// The level that applies, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effective {
    pub level: AccessLevel,
    pub source: LevelSource,
    /// The global level was above `read` and the connection is production.
    pub capped: bool,
}

impl LevelChain {
    /// Most specific wins. A level inherited from the global default never
    /// grants a write on production: that needs a level on the connection or
    /// database itself.
    pub fn resolve(self) -> Effective {
        if let Some(level) = self.database {
            return Effective {
                level,
                source: LevelSource::Database,
                capped: false,
            };
        }
        if let Some(level) = self.connection {
            return Effective {
                level,
                source: LevelSource::Connection,
                capped: false,
            };
        }
        let capped = self.production && self.global > AccessLevel::Read;
        Effective {
            level: if capped {
                AccessLevel::Read
            } else {
                self.global
            },
            source: LevelSource::Global,
            capped,
        }
    }
}

/// Names for the deny message. Never secrets.
#[derive(Debug, Clone, Copy)]
pub struct Target<'a> {
    pub caller: ToolCaller,
    /// The id the caller sent. A `none` deny names only this.
    pub connection_id: &'a str,
    pub connection: &'a str,
    pub database: Option<&'a str>,
}

impl Target<'_> {
    fn place(&self) -> String {
        match self.database {
            Some(db) => format!("database \"{db}\" on connection \"{}\"", self.connection),
            None => format!("connection \"{}\"", self.connection),
        }
    }
}

/// What happens to one tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Refused. The message says which level is needed and how to grant it.
    Deny(String),
    /// "Ask me for high-risk writes" is on: send a single-use request to the
    /// approval dock.
    Ask,
}

/// The decision table.
///
/// | effective | needs read | needs write | needs full |
/// |-----------|------------|-------------|------------|
/// | none      | deny       | deny        | deny       |
/// | read      | allow      | deny / ask  | deny / ask |
/// | write     | allow      | allow       | deny / ask |
/// | full      | allow      | allow       | allow      |
///
/// "ask" replaces "deny" only when the connection asks for high-risk writes
/// and the call is a write. A connection at `none` is never asked about.
pub fn decide(effective: Effective, need: AccessLevel, ask: bool, target: Target<'_>) -> Decision {
    if effective.level >= need && effective.level > AccessLevel::None {
        return Decision::Allow;
    }
    if effective.level == AccessLevel::None {
        return Decision::Deny(no_access_message(target.caller, target.connection_id));
    }
    if ask && need >= AccessLevel::Write {
        return Decision::Ask;
    }
    Decision::Deny(above_level_message(effective, need, target))
}

/// Names the id the caller sent, never the connection's name: a hidden
/// connection stays hidden, and an unknown id reads the same.
pub fn no_access_message(caller: ToolCaller, connection_id: &str) -> String {
    format!(
        "Access denied: {who} has no access to connection id \"{connection_id}\" (or it does not exist). Nothing ran. Use list_connections to see what is available; the user can grant access in Mystique: Settings → MCP → Access (read, write or full).",
        who = caller.label(),
    )
}

fn above_level_message(effective: Effective, need: AccessLevel, target: Target<'_>) -> String {
    let why = if effective.capped {
        " (the global default never allows writes on production: a production connection or a database marked production)"
    } else {
        match effective.source {
            LevelSource::Global => " (the global default)",
            LevelSource::Connection => " (set on the connection)",
            LevelSource::Database => " (set on the database)",
        }
    };
    format!(
        "Access denied: this statement needs `{need}` access ({what}), but {who} has `{have}` on {place}{why}. Nothing ran. To allow it, the user can set {place} to `{need}` in Mystique: Settings → MCP → Access, or turn on \"Ask me for high-risk writes\" for the connection to approve single statements.",
        need = need.as_str(),
        what = need.what(),
        who = target.caller.label(),
        have = effective.level.as_str(),
        place = target.place(),
    )
}

#[cfg(test)]
mod tests {
    use super::AccessLevel::{Full, None as NoAccess, Read, Write};
    use super::*;

    fn chain(global: AccessLevel) -> LevelChain {
        LevelChain {
            global,
            connection: None,
            database: None,
            production: false,
        }
    }

    const MCP: Target<'static> = Target {
        caller: ToolCaller::Mcp,
        connection_id: "7",
        connection: "prod",
        database: Some("app"),
    };

    #[test]
    fn levels_are_ordered_and_round_trip() {
        assert!(NoAccess < Read && Read < Write && Write < Full);
        for level in AccessLevel::ALL {
            assert_eq!(AccessLevel::parse(level.as_str()), Some(level));
        }
        assert_eq!(AccessLevel::parse(" FULL "), Some(Full));
        assert_eq!(AccessLevel::parse("read-only"), None);
        assert_eq!(AccessLevel::parse(""), None);
    }

    #[test]
    fn statements_need_the_level_of_their_risk() {
        assert_eq!(AccessLevel::required_for(WriteRisk::Read, false), Read);
        assert_eq!(AccessLevel::required_for(WriteRisk::Write, true), Write);
        // DELETE, CREATE and other non-INSERT/UPDATE writes.
        assert_eq!(AccessLevel::required_for(WriteRisk::Write, false), Full);
        assert_eq!(
            AccessLevel::required_for(WriteRisk::Destructive, true),
            Full
        );
    }

    #[test]
    fn the_most_specific_level_wins() {
        let base = chain(Read);
        assert_eq!(base.resolve().level, Read);
        assert_eq!(base.resolve().source, LevelSource::Global);

        let conn = LevelChain {
            connection: Some(Write),
            ..base
        };
        assert_eq!(conn.resolve().level, Write);
        assert_eq!(conn.resolve().source, LevelSource::Connection);

        // A database set lower than its connection wins too.
        let db = LevelChain {
            database: Some(NoAccess),
            ..conn
        };
        assert_eq!(db.resolve().level, NoAccess);
        assert_eq!(db.resolve().source, LevelSource::Database);

        // Unset database inherits the connection; unset connection the global.
        let only_db = LevelChain {
            database: Some(Full),
            ..base
        };
        assert_eq!(only_db.resolve().level, Full);
    }

    #[test]
    fn the_global_default_never_writes_on_production() {
        let prod = LevelChain {
            production: true,
            ..chain(Full)
        };
        let eff = prod.resolve();
        assert_eq!(eff.level, Read);
        assert!(eff.capped);
        // Read and none pass through untouched.
        assert!(
            !LevelChain {
                production: true,
                ..chain(Read)
            }
            .resolve()
            .capped
        );
        // A level on the connection itself is honoured.
        let explicit = LevelChain {
            connection: Some(Write),
            ..prod
        };
        assert_eq!(explicit.resolve().level, Write);
        assert!(!explicit.resolve().capped);
    }

    fn got(level: AccessLevel, need: AccessLevel, ask: bool) -> &'static str {
        let eff = LevelChain {
            connection: Some(level),
            ..chain(NoAccess)
        }
        .resolve();
        match decide(eff, need, ask, MCP) {
            Decision::Allow => "allow",
            Decision::Deny(_) => "deny",
            Decision::Ask => "ask",
        }
    }

    #[test]
    fn decision_table() {
        let rows = [
            // level, need, ask, want
            (NoAccess, Read, false, "deny"),
            (NoAccess, Write, true, "deny"),
            (NoAccess, Full, true, "deny"),
            (Read, Read, false, "allow"),
            (Read, Write, false, "deny"),
            (Read, Write, true, "ask"),
            (Read, Full, false, "deny"),
            (Read, Full, true, "ask"),
            (Write, Read, false, "allow"),
            (Write, Write, false, "allow"),
            (Write, Full, false, "deny"),
            (Write, Full, true, "ask"),
            (Full, Read, false, "allow"),
            (Full, Write, false, "allow"),
            (Full, Full, false, "allow"),
            (Full, Full, true, "allow"),
        ];
        for (level, need, ask, want) in rows {
            assert_eq!(got(level, need, ask), want, "{level:?} {need:?} ask={ask}");
        }
    }

    #[test]
    fn a_deny_says_what_is_needed_and_how_to_grant_it() {
        let eff = LevelChain {
            connection: Some(Write),
            ..chain(NoAccess)
        }
        .resolve();
        let Decision::Deny(msg) = decide(eff, Full, false, MCP) else {
            panic!("expected deny");
        };
        assert!(msg.contains("needs `full`"), "{msg}");
        assert!(msg.contains("has `write`"), "{msg}");
        assert!(
            msg.contains("database \"app\" on connection \"prod\""),
            "{msg}"
        );
        assert!(msg.contains("Settings → MCP → Access"), "{msg}");
        assert!(msg.contains("Nothing ran"), "{msg}");

        let prod = LevelChain {
            production: true,
            ..chain(Full)
        }
        .resolve();
        let Decision::Deny(msg) = decide(prod, Write, false, MCP) else {
            panic!("expected deny");
        };
        assert!(msg.contains("production"), "{msg}");

        let none = chain(NoAccess).resolve();
        let Decision::Deny(msg) = decide(none, Read, false, MCP) else {
            panic!("expected deny");
        };
        assert!(msg.contains("no access"), "{msg}");
        assert!(msg.contains("\"7\""), "{msg}");
        assert!(
            !msg.contains("prod"),
            "a hidden connection's name leaked: {msg}"
        );
    }
}
