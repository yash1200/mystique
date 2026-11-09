//! Write risk for Mongo shell text, read off the parsed [`Statement`].
//!
//! The risk comes from the method, the command `op`, and stage keys
//! (`$out`, `$merge`, `$drop`) in the parsed arguments. Filter values are
//! data: `deleteMany({op: "find"})` is a delete, and `"$out"` as a value is
//! a string. Text that does not parse fails closed as a write.

use mystique_connector_api::{
    ConnectorError, ConnectorResult, EngineQueryPolicy, QueryMode, WriteRisk,
};

use super::parse::{lower, parse, ShellCall, Statement};
use super::value::Lit;

/// The MongoDB [`EngineQueryPolicy`]. Core gets it from
/// [`crate::query_policy`].
pub struct MongoQueryPolicy;

impl EngineQueryPolicy for MongoQueryPolicy {
    fn engine_id(&self) -> &'static str {
        "mongodb"
    }

    /// Read mode refuses anything but a read. Otherwise the text must be a
    /// call the executor can run, so an unsupported method fails here, before
    /// the write guard asks to confirm it.
    fn validate(&self, text: &str, mode: QueryMode) -> ConnectorResult<()> {
        let stmt = parse(text)?;
        if mode == QueryMode::Read && statement_risk(&stmt) != WriteRisk::Read {
            return Err(ConnectorError::Policy(
                "Write operation blocked. This connection is read-only.".into(),
            ));
        }
        lower(&stmt).map(|_| ())
    }

    fn classify(&self, text: &str) -> WriteRisk {
        classify(text)
    }

    fn is_safe_dml(&self, text: &str) -> bool {
        is_safe_dml(text)
    }

    fn op_preview(&self, text: &str) -> String {
        super::preview::op_preview(text)
    }

    fn split(&self, text: &str) -> Vec<std::ops::Range<usize>> {
        super::split::split(text)
    }
}

/// Text that does not parse is a write. Text with more than one statement
/// takes the worst of them, so a caller that classifies a whole buffer (the
/// agent and MCP write tiers) sees the `drop` in `find(); drop()`.
pub(crate) fn classify(text: &str) -> WriteRisk {
    let parts = super::split::split(text);
    if parts.len() > 1 {
        return parts
            .into_iter()
            .map(|range| classify_one(&text[range]))
            .max()
            .unwrap_or(WriteRisk::Write);
    }
    classify_one(text)
}

fn classify_one(text: &str) -> WriteRisk {
    match parse(text) {
        Ok(stmt) => statement_risk(&stmt),
        Err(_) => WriteRisk::Write,
    }
}

pub(crate) fn statement_risk(stmt: &Statement) -> WriteRisk {
    let stages = stage_risk(stmt);
    let call = match stmt {
        Statement::Listing(_) | Statement::Filter(_) | Statement::Pipeline(_) => WriteRisk::Read,
        Statement::Command(_) => stmt
            .command_ops()
            .into_iter()
            .map(|op| match op {
                Lit::Str(name) => op_risk(&name.to_ascii_lowercase()),
                _ => WriteRisk::Write,
            })
            .max()
            .unwrap_or(WriteRisk::Write),
        Statement::Call(call) => call_risk(call),
    };
    stages.max(call)
}

/// INSERT / UPDATE / REPLACE only: the MCP `safe_write` tier. Deletes,
/// `bulkWrite`, `findOneAndUpdate` and `$out` / `$merge` are not. More than
/// one statement is safe only when every one of them is.
pub(crate) fn is_safe_dml(text: &str) -> bool {
    let parts = super::split::split(text);
    if parts.len() > 1 {
        return parts.into_iter().all(|range| is_safe_dml_one(&text[range]));
    }
    is_safe_dml_one(text)
}

fn is_safe_dml_one(text: &str) -> bool {
    let Ok(stmt) = parse(text) else {
        return false;
    };
    if statement_risk(&stmt) != WriteRisk::Write || stage_risk(&stmt) != WriteRisk::Read {
        return false;
    }
    match &stmt {
        Statement::Command(_) => match stmt.command_ops().as_slice() {
            [Lit::Str(op)] => is_safe_write(&op.to_ascii_lowercase()),
            _ => false,
        },
        Statement::Call(call) => {
            call.collection.is_some()
                && is_safe_write(&call.method_lower())
                && call
                    .chain
                    .iter()
                    .all(|link| chain_risk(&link.method) == WriteRisk::Read)
        }
        _ => false,
    }
}

fn is_safe_write(op: &str) -> bool {
    matches!(
        op,
        "insert"
            | "insertone"
            | "insertmany"
            | "update"
            | "updateone"
            | "updatemany"
            | "replaceone"
    )
}

/// `$out` / `$merge` are writes and `$drop` destroys, at any depth, as keys.
fn stage_risk(stmt: &Statement) -> WriteRisk {
    let mut worst = WriteRisk::Read;
    for lit in stmt.literals() {
        lit.walk_keys(&mut |key| {
            let key = key.to_ascii_lowercase();
            if key == "$drop" {
                worst = WriteRisk::Destructive;
            } else if key == "$out" || key == "$merge" {
                worst = worst.max(WriteRisk::Write);
            }
        });
    }
    worst
}

fn call_risk(call: &ShellCall) -> WriteRisk {
    let method = call.method_lower();
    let base = match method.as_str() {
        "runcommand" | "admincommand" => command_risk(call.args.first()),
        "renamecollection" => {
            let drop_target = matches!(call.args.get(1), Some(Lit::Bool(true)))
                || call
                    .args
                    .get(1)
                    .and_then(|opts| opts.get("dropTarget"))
                    .is_some_and(|v| matches!(v, Lit::Bool(true)));
            if drop_target {
                WriteRisk::Destructive
            } else {
                WriteRisk::Write
            }
        }
        _ => op_risk(&method),
    };
    call.chain
        .iter()
        .map(|link| chain_risk(&link.method))
        .fold(base, WriteRisk::max)
}

/// `db.runCommand({ drop: "t" })`: the command name is the first key.
fn command_risk(arg: Option<&Lit>) -> WriteRisk {
    let name = match arg {
        Some(Lit::Str(name)) => name.to_ascii_lowercase(),
        Some(Lit::Object(entries)) => {
            let drop_target = entries
                .iter()
                .any(|(k, v)| k == "dropTarget" && matches!(v, Lit::Bool(true)));
            if drop_target {
                return WriteRisk::Destructive;
            }
            match entries.first() {
                Some((key, _)) => key.to_ascii_lowercase(),
                None => return WriteRisk::Write,
            }
        }
        _ => return WriteRisk::Write,
    };
    match name.as_str() {
        "drop"
        | "dropdatabase"
        | "dropindexes"
        | "deleteindexes"
        | "dropsearchindex"
        | "dropuser"
        | "dropallusersfromdatabase"
        | "droprole"
        | "dropallrolesfromdatabase"
        | "shutdown" => WriteRisk::Destructive,
        // Commands can do anything; only the destructive ones get a higher
        // tier. None of them count as a read.
        _ => WriteRisk::Write,
    }
}

/// One method or command op, lowercased. Unknown names are writes.
fn op_risk(op: &str) -> WriteRisk {
    match op {
        // Reads.
        "find"
        | "findone"
        | "count"
        | "countdocuments"
        | "estimateddocumentcount"
        | "distinct"
        | "explain"
        | "aggregate"
        | "listdatabases"
        | "listcollections"
        | "listindexes"
        | "getindexes"
        | "stats"
        | "datasize"
        | "storagesize" => WriteRisk::Read,
        "drop" | "dropdatabase" | "dropcollection" | "dropindex" | "dropindexes" => {
            WriteRisk::Destructive
        }
        _ => WriteRisk::Write,
    }
}

/// A method chained after the call. Cursor modifiers change nothing.
fn chain_risk(method: &str) -> WriteRisk {
    match method.to_ascii_lowercase().as_str() {
        "limit" | "skip" | "sort" | "project" | "projection" | "hint" | "pretty" | "toarray"
        | "foreach" | "map" | "hasnext" | "next" | "close" | "batchsize" | "maxtimems"
        | "collation" | "comment" | "allowdiskuse" | "readpref" | "max" | "min" | "itcount"
        | "size" => WriteRisk::Read,
        other => op_risk(other),
    }
}
