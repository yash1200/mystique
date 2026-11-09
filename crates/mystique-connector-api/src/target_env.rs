//! Per-database environment: which marked database a write lands in.
//!
//! A connection has one environment, but a dev server can host one
//! production database. [`DatabaseEnvironment`] marks it, and
//! [`target_environment`] judges a write by the most dangerous database it
//! lands in ([`crate::WriteTargets`], read off the statement).

use serde::{Deserialize, Serialize};

use crate::{Environment, WriteTarget, WriteTargets};

/// A database (or one schema of it) whose environment is not the
/// connection's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct DatabaseEnvironment {
    pub database: String,
    /// Postgres schema. `None` marks the whole database.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// `local` | `staging` | `production`.
    pub environment: String,
}

impl DatabaseEnvironment {
    fn env(&self) -> Environment {
        Environment::parse(&self.environment)
    }

    fn is_database(&self, database: &str) -> bool {
        self.database.trim().eq_ignore_ascii_case(database.trim())
    }

    fn schema(&self) -> Option<&str> {
        self.schema
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }
}

/// The environment a write is judged by, and the marked database that set it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetEnvironment {
    pub environment: Environment,
    /// The marked database the write lands in. `None`: the connection's
    /// environment applies, or the target could not be named.
    pub database: Option<String>,
}

/// The most dangerous environment among the databases a write lands in.
///
/// No marks: the connection environment, as before. An unresolved target
/// counts as the most dangerous database on the connection, so a production
/// mark on any database makes it production.
pub fn target_environment(
    connection: Environment,
    marks: &[DatabaseEnvironment],
    targets: &WriteTargets,
) -> TargetEnvironment {
    let mut worst = TargetEnvironment {
        environment: connection,
        database: None,
    };
    if marks.is_empty() {
        return worst;
    }
    if targets.unresolved {
        let most = marks.iter().map(DatabaseEnvironment::env).max();
        worst.environment = most.map_or(connection, |env| env.max(connection));
        return worst;
    }
    for (index, target) in targets.targets.iter().enumerate() {
        let env = mark_for(connection, marks, target);
        if index == 0 || env > worst.environment {
            worst = TargetEnvironment {
                environment: env,
                database: marks
                    .iter()
                    .any(|m| m.is_database(&target.database))
                    .then(|| target.database.clone()),
            };
        }
    }
    worst
}

fn mark_for(
    connection: Environment,
    marks: &[DatabaseEnvironment],
    target: &WriteTarget,
) -> Environment {
    let in_db = || marks.iter().filter(|m| m.is_database(&target.database));
    let whole = in_db()
        .find(|m| m.schema().is_none())
        .map_or(connection, DatabaseEnvironment::env);
    match target.schema.as_deref() {
        Some(schema) => in_db()
            .find(|m| m.schema() == Some(schema))
            .map_or(whole, DatabaseEnvironment::env),
        // The search path picks the schema: any marked schema may be it.
        None => in_db()
            .filter(|m| m.schema().is_some())
            .map(DatabaseEnvironment::env)
            .fold(whole, Environment::max),
    }
}

/// True when the connection or any database on it is production.
pub fn any_production(connection: Environment, marks: &[DatabaseEnvironment]) -> bool {
    connection == Environment::Production
        || marks.iter().any(|m| m.env() == Environment::Production)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Environment::{Local, Production, Staging};

    fn mark(database: &str, schema: Option<&str>, env: &str) -> DatabaseEnvironment {
        DatabaseEnvironment {
            database: database.into(),
            schema: schema.map(str::to_string),
            environment: env.into(),
        }
    }

    fn at(targets: &[(&str, Option<&str>)]) -> WriteTargets {
        let mut out = WriteTargets::default();
        for (database, schema) in targets {
            out.extend(WriteTargets::database(database, *schema));
        }
        out
    }

    fn env(
        connection: Environment,
        marks: &[DatabaseEnvironment],
        targets: &WriteTargets,
    ) -> Environment {
        target_environment(connection, marks, targets).environment
    }

    #[test]
    fn no_marks_keeps_the_connection_environment() {
        assert_eq!(env(Local, &[], &WriteTargets::unresolved()), Local);
        assert_eq!(env(Production, &[], &at(&[("scratch", None)])), Production);
    }

    #[test]
    fn a_production_database_on_a_local_server() {
        let marks = [mark("prod", None, "production")];
        let got = target_environment(Local, &marks, &at(&[("prod", None)]));
        assert_eq!(got.environment, Production);
        assert_eq!(got.database.as_deref(), Some("prod"));
        assert_eq!(env(Local, &marks, &at(&[("scratch", None)])), Local);
        assert_eq!(env(Local, &marks, &at(&[("PROD", None)])), Production);
    }

    #[test]
    fn the_most_dangerous_target_wins() {
        let marks = [
            mark("prod", None, "production"),
            mark("stage", None, "staging"),
        ];
        let got = target_environment(
            Local,
            &marks,
            &at(&[("scratch", None), ("stage", None), ("prod", None)]),
        );
        assert_eq!(got.environment, Production);
        assert_eq!(got.database.as_deref(), Some("prod"));
        assert_eq!(
            env(Local, &marks, &at(&[("scratch", None), ("stage", None)])),
            Staging
        );
    }

    #[test]
    fn unresolved_counts_as_production_when_any_database_is() {
        let marks = [mark("prod", None, "production")];
        let got = target_environment(Local, &marks, &WriteTargets::unresolved());
        assert_eq!(got.environment, Production);
        assert_eq!(got.database, None);
        let marks = [mark("stage", None, "staging")];
        assert_eq!(env(Local, &marks, &WriteTargets::unresolved()), Staging);
    }

    #[test]
    fn a_local_mark_on_a_production_connection_relaxes_that_database_only() {
        let marks = [mark("scratch", None, "local")];
        assert_eq!(env(Production, &marks, &at(&[("scratch", None)])), Local);
        assert_eq!(env(Production, &marks, &at(&[("app", None)])), Production);
        assert_eq!(
            env(Production, &marks, &WriteTargets::unresolved()),
            Production
        );
    }

    #[test]
    fn schema_marks_apply_to_that_schema_and_to_unknown_schemas() {
        let marks = [mark("app", Some("billing"), "production")];
        assert_eq!(
            env(Local, &marks, &at(&[("app", Some("billing"))])),
            Production
        );
        assert_eq!(env(Local, &marks, &at(&[("app", Some("public"))])), Local);
        // An unqualified name goes through the search path.
        assert_eq!(env(Local, &marks, &at(&[("app", None)])), Production);
        let marks = [
            mark("app", None, "production"),
            mark("app", Some("scratch"), "local"),
        ];
        assert_eq!(env(Local, &marks, &at(&[("app", Some("scratch"))])), Local);
        assert_eq!(
            env(Local, &marks, &at(&[("app", Some("public"))])),
            Production
        );
    }

    #[test]
    fn any_production_reads_the_connection_and_the_marks() {
        assert!(any_production(Production, &[]));
        assert!(!any_production(Local, &[mark("a", None, "staging")]));
        assert!(any_production(Local, &[mark("a", None, "production")]));
    }
}
