//! One confirmation policy for writes (environment × action × risk).
//!
//! The manager calls [`confirmation_for`] before every write and turns a
//! non-`None` answer into a [`crate::WriteGuardPrompt`]. Read-only sits above
//! the matrix: it blocks writes and nothing else is asked.

use crate::{ConnectorError, ConnectorResult, WriteRisk};

/// Profile environment. Anything unknown counts as local.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Local,
    Staging,
    Production,
}

impl Environment {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "production" => Self::Production,
            "staging" => Self::Staging,
            _ => Self::Local,
        }
    }
}

/// What the user is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteAction {
    /// Grid save and its undo.
    GridSave,
    /// Query editor, explain analyze, agent and MCP.
    Query,
    /// Structure apply and schema diff.
    Structure,
    /// Rename, duplicate, create, drop or truncate from a menu.
    ObjectAdmin,
    /// The import dialog is the confirmation.
    Import,
    /// The terminate dialog is the confirmation.
    Terminate,
}

/// Session facts the policy needs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WriteState {
    pub read_only: bool,
    /// Writes were allowed for this session.
    pub unlocked: bool,
    /// Manual-commit tab: Commit is the decision.
    pub pinned: bool,
    /// The request carries the expected object name.
    pub confirmed: bool,
}

/// What to ask before the write runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirmation {
    None,
    /// One dialog with the SQL. `unlock` means confirming allows writes for
    /// the rest of the session; otherwise the retry carries the object name.
    Sql {
        statements: Vec<String>,
        unlock: bool,
    },
    /// Type the object name. On production this also unlocks the session.
    TypeConfirm {
        name: String,
        statements: Vec<String>,
    },
}

pub const READ_ONLY_MESSAGE: &str =
    "This connection is read-only. Disable read-only to run writes.";

/// The §4.1 matrix. `statements` is what the dialog shows.
pub fn confirmation_for(
    env: Environment,
    action: WriteAction,
    risk: WriteRisk,
    state: WriteState,
    name: &str,
    statements: Vec<String>,
) -> ConnectorResult<Confirmation> {
    if risk == WriteRisk::Read {
        return Ok(Confirmation::None);
    }
    if state.read_only {
        return Err(ConnectorError::Policy(READ_ONLY_MESSAGE.into()));
    }
    let destructive = risk == WriteRisk::Destructive;
    let ask = match env {
        Environment::Local => Confirmation::None,
        Environment::Staging if destructive && !state.confirmed => Confirmation::Sql {
            statements,
            unlock: false,
        },
        Environment::Staging => Confirmation::None,
        Environment::Production => match action {
            WriteAction::Terminate | WriteAction::Import => Confirmation::None,
            _ if destructive && !state.confirmed => Confirmation::TypeConfirm {
                name: name.to_string(),
                statements,
            },
            _ if destructive => Confirmation::None,
            WriteAction::Query if state.pinned => Confirmation::None,
            _ if state.unlocked => Confirmation::None,
            _ => Confirmation::Sql {
                statements,
                unlock: true,
            },
        },
    };
    Ok(ask)
}

/// A typed confirmation on production also allows later writes.
pub fn confirmation_unlocks(env: Environment, risk: WriteRisk, state: WriteState) -> bool {
    env == Environment::Production && risk == WriteRisk::Destructive && state.confirmed
}

#[cfg(test)]
mod tests {
    use super::*;

    use Environment::{Local, Production, Staging};
    use WriteAction::{GridSave, Import, ObjectAdmin, Query, Structure, Terminate};
    use WriteRisk::{Destructive, Read, Write};

    #[derive(Debug, PartialEq)]
    enum Want {
        Nothing,
        Unlock,
        Dialog,
        Type,
        Blocked,
    }
    use Want::{Blocked, Dialog, Nothing, Type, Unlock};

    const FRESH: WriteState = WriteState {
        read_only: false,
        unlocked: false,
        pinned: false,
        confirmed: false,
    };
    const UNLOCKED: WriteState = WriteState {
        unlocked: true,
        ..FRESH
    };
    const PINNED: WriteState = WriteState {
        pinned: true,
        ..FRESH
    };
    const CONFIRMED: WriteState = WriteState {
        confirmed: true,
        ..FRESH
    };
    const READ_ONLY: WriteState = WriteState {
        read_only: true,
        unlocked: true,
        confirmed: true,
        pinned: false,
    };

    fn got(env: Environment, action: WriteAction, risk: WriteRisk, state: WriteState) -> Want {
        match confirmation_for(env, action, risk, state, "orders", vec!["SQL".into()]) {
            Err(_) => Blocked,
            Ok(Confirmation::None) => Nothing,
            Ok(Confirmation::Sql { unlock: true, .. }) => Unlock,
            Ok(Confirmation::Sql { unlock: false, .. }) => Dialog,
            Ok(Confirmation::TypeConfirm { .. }) => Type,
        }
    }

    #[test]
    fn matrix() {
        let rows: &[(Environment, WriteAction, WriteRisk, WriteState, Want)] = &[
            // Reads never ask, even on a read-only production profile.
            (Production, Query, Read, READ_ONLY, Nothing),
            (Production, Query, Read, FRESH, Nothing),
            // Read-only blocks every write, above the matrix.
            (Local, GridSave, Write, READ_ONLY, Blocked),
            (Staging, Query, Destructive, READ_ONLY, Blocked),
            (Production, Terminate, Write, READ_ONLY, Blocked),
            (Production, Import, Write, READ_ONLY, Blocked),
            // Grid save.
            (Local, GridSave, Write, FRESH, Nothing),
            (Staging, GridSave, Write, FRESH, Nothing),
            (Production, GridSave, Write, FRESH, Unlock),
            (Production, GridSave, Write, UNLOCKED, Nothing),
            // Query-editor DML.
            (Local, Query, Write, FRESH, Nothing),
            (Staging, Query, Write, FRESH, Nothing),
            (Production, Query, Write, FRESH, Unlock),
            (Production, Query, Write, UNLOCKED, Nothing),
            (Production, Query, Write, PINNED, Nothing),
            // Structure apply, non-destructive DDL.
            (Local, Structure, Write, FRESH, Nothing),
            (Staging, Structure, Write, FRESH, Nothing),
            (Production, Structure, Write, FRESH, Unlock),
            (Production, Structure, Write, UNLOCKED, Nothing),
            // Destructive. Local asks nothing here; a menu shows its own dialog.
            (Local, Query, Destructive, FRESH, Nothing),
            (Local, ObjectAdmin, Destructive, FRESH, Nothing),
            (Staging, Query, Destructive, FRESH, Dialog),
            (Staging, ObjectAdmin, Destructive, FRESH, Dialog),
            (Staging, Structure, Destructive, CONFIRMED, Nothing),
            (Production, Query, Destructive, FRESH, Type),
            (Production, Query, Destructive, UNLOCKED, Type),
            (Production, Query, Destructive, PINNED, Type),
            (Production, ObjectAdmin, Destructive, FRESH, Type),
            (Production, Structure, Destructive, FRESH, Type),
            (Production, ObjectAdmin, Destructive, CONFIRMED, Nothing),
            // Import: the mapping step is the confirmation.
            (Local, Import, Write, FRESH, Nothing),
            (Staging, Import, Write, FRESH, Nothing),
            (Production, Import, Write, FRESH, Nothing),
            // Rename, duplicate, create.
            (Local, ObjectAdmin, Write, FRESH, Nothing),
            (Staging, ObjectAdmin, Write, FRESH, Nothing),
            (Production, ObjectAdmin, Write, FRESH, Unlock),
            (Production, ObjectAdmin, Write, UNLOCKED, Nothing),
            // Terminate: one dialog in the UI, never an unlock.
            (Local, Terminate, Write, FRESH, Nothing),
            (Staging, Terminate, Write, FRESH, Nothing),
            (Production, Terminate, Write, FRESH, Nothing),
        ];
        for (env, action, risk, state, want) in rows {
            assert_eq!(
                &got(*env, *action, *risk, *state),
                want,
                "{env:?} {action:?} {risk:?} {state:?}"
            );
        }
    }

    #[test]
    fn prompts_carry_the_statements_and_name() {
        let ask = confirmation_for(
            Production,
            Query,
            Destructive,
            FRESH,
            "orders",
            vec!["DROP TABLE orders".into()],
        )
        .expect("policy");
        assert_eq!(
            ask,
            Confirmation::TypeConfirm {
                name: "orders".into(),
                statements: vec!["DROP TABLE orders".into()],
            }
        );
        let ask = confirmation_for(
            Production,
            GridSave,
            Write,
            FRESH,
            "orders",
            vec!["UPDATE orders SET n = 1".into()],
        )
        .expect("policy");
        assert_eq!(
            ask,
            Confirmation::Sql {
                statements: vec!["UPDATE orders SET n = 1".into()],
                unlock: true,
            }
        );
    }

    #[test]
    fn only_a_typed_production_confirm_unlocks() {
        assert!(confirmation_unlocks(Production, Destructive, CONFIRMED));
        assert!(!confirmation_unlocks(Production, Destructive, FRESH));
        assert!(!confirmation_unlocks(Staging, Destructive, CONFIRMED));
        assert!(!confirmation_unlocks(Production, Write, CONFIRMED));
    }

    #[test]
    fn unknown_environment_is_local() {
        assert_eq!(Environment::parse(" Production "), Production);
        assert_eq!(Environment::parse("STAGING"), Staging);
        assert_eq!(Environment::parse("dev"), Local);
        assert_eq!(Environment::parse(""), Local);
    }
}
