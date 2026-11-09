//! Which databases a Mongo write lands in, for per-database production.
//!
//! - `db.coll.updateOne(…)`, a filter, a pipeline or a command object:
//!   the session database.
//! - `db.getSiblingDB("x")…` / `db.getMongo().getDB("x")…`: `x`.
//! - `$out: { db: "x", … }` and `$merge: { into: { db: "x", … } }`: `x`
//!   as well.
//! - `adminCommand` (it can name any namespace), text that does not parse,
//!   and a write with no session database are unresolved.

use mystique_connector_api::{WriteRisk, WriteTargets};

use super::parse::{parse, Statement};
use super::policy::statement_risk;
use super::value::Lit;

pub(crate) fn write_targets(text: &str, current: Option<&str>) -> WriteTargets {
    let parts = super::split::split(text);
    if parts.len() > 1 {
        let mut out = WriteTargets::default();
        for range in parts {
            out.extend(one(&text[range], current));
        }
        return out;
    }
    one(text, current)
}

fn one(text: &str, current: Option<&str>) -> WriteTargets {
    let Ok(stmt) = parse(text) else {
        return WriteTargets::unresolved();
    };
    if statement_risk(&stmt) == WriteRisk::Read {
        return WriteTargets::default();
    }
    let home = match &stmt {
        Statement::Call(call) if call.method_lower() == "admincommand" => {
            return WriteTargets::unresolved();
        }
        Statement::Call(call) => call.database.as_deref().or(current),
        _ => current,
    };
    let mut out = WriteTargets::current(home);
    for lit in stmt.literals() {
        stage_targets(lit, home, &mut out);
    }
    out
}

/// `$out` / `$merge` into another database, at any depth.
fn stage_targets(lit: &Lit, home: Option<&str>, out: &mut WriteTargets) {
    match lit {
        Lit::Object(entries) => {
            for (key, value) in entries {
                match key.to_ascii_lowercase().as_str() {
                    "$out" => out.extend(stage_database(value, home)),
                    "$merge" => {
                        let into = value.get("into").unwrap_or(value);
                        out.extend(stage_database(into, home));
                    }
                    _ => {}
                }
                stage_targets(value, home, out);
            }
        }
        Lit::Array(items) | Lit::Helper { args: items, .. } => {
            for item in items {
                stage_targets(item, home, out);
            }
        }
        _ => {}
    }
}

/// `"coll"` is the home database; `{ db: "x", coll: … }` is `x`.
fn stage_database(value: &Lit, home: Option<&str>) -> WriteTargets {
    match value {
        Lit::Str(_) => WriteTargets::current(home),
        Lit::Object(_) => match value.get("db") {
            Some(Lit::Str(db)) => WriteTargets::database(db, None),
            Some(_) => WriteTargets::unresolved(),
            None => WriteTargets::current(home),
        },
        _ => WriteTargets::unresolved(),
    }
}

#[cfg(test)]
mod tests {
    use mystique_connector_api::WriteTarget;

    use super::*;

    fn dbs(targets: &WriteTargets) -> Vec<&str> {
        targets
            .targets
            .iter()
            .map(|t: &WriteTarget| t.database.as_str())
            .collect()
    }

    #[test]
    fn reads_name_nothing() {
        assert!(write_targets("db.users.find({})", Some("app")).is_empty());
        assert!(write_targets("{ name: 'x' }", Some("app")).is_empty());
    }

    #[test]
    fn a_write_lands_in_the_session_database() {
        let got = write_targets("db.users.deleteMany({ a: 1 })", Some("app"));
        assert_eq!(dbs(&got), ["app"]);
        assert!(!got.unresolved);
    }

    #[test]
    fn sibling_database_is_that_database() {
        let got = write_targets(
            "db.getSiblingDB('prod').users.updateOne({}, { $set: { a: 1 } })",
            Some("app"),
        );
        assert_eq!(dbs(&got), ["prod"]);
        let got = write_targets("db.getMongo().getDB(\"prod\").users.drop()", Some("app"));
        assert_eq!(dbs(&got), ["prod"]);
    }

    #[test]
    fn out_and_merge_into_another_database_count() {
        let got = write_targets(
            "db.users.aggregate([{ $match: {} }, { $out: { db: 'prod', coll: 'x' } }])",
            Some("app"),
        );
        assert_eq!(dbs(&got), ["app", "prod"]);
        let got = write_targets(
            "db.users.aggregate([{ $merge: { into: { db: 'prod', coll: 'x' } } }])",
            Some("app"),
        );
        assert_eq!(dbs(&got), ["app", "prod"]);
        let got = write_targets("db.users.aggregate([{ $out: 'x' }])", Some("app"));
        assert_eq!(dbs(&got), ["app"]);
    }

    #[test]
    fn admin_commands_unparsed_text_and_no_database_are_unresolved() {
        assert!(
            write_targets(
                "db.adminCommand({ renameCollection: 'a.b', to: 'c.d' })",
                Some("app")
            )
            .unresolved
        );
        assert!(write_targets("db.users.insertOne(", Some("app")).unresolved);
        assert!(write_targets("db.users.insertOne({})", None).unresolved);
    }

    #[test]
    fn every_statement_of_a_batch_counts() {
        let got = write_targets(
            "db.a.insertOne({});\ndb.getSiblingDB('prod').b.insertOne({})",
            Some("app"),
        );
        assert_eq!(dbs(&got), ["app", "prod"]);
    }
}
