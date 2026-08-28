//! User-facing MongoDB query errors (the driver Display dumps Kind/labels).

use mongodb::error::{Error as MongoError, ErrorKind, WriteFailure};
use mystique_connector_api::ConnectorError;

pub(crate) fn format_query_error(err: &MongoError) -> String {
    format_kind(err.kind.as_ref())
}

fn format_kind(kind: &ErrorKind) -> String {
    match kind {
        ErrorKind::Command(cmd) => {
            let msg = cmd.message.trim();
            if msg.is_empty() {
                format!("MongoDB command failed ({})", cmd.code_name)
            } else {
                msg.to_string()
            }
        }
        ErrorKind::Write(WriteFailure::WriteError(w)) => nonempty_or(&w.message, kind),
        ErrorKind::Write(WriteFailure::WriteConcernError(w)) => nonempty_or(&w.message, kind),
        ErrorKind::InsertMany(bulk) => {
            let mut parts = Vec::new();
            if let Some(wc) = &bulk.write_concern_error {
                push_unique(&mut parts, wc.message.trim());
            }
            if let Some(errors) = &bulk.write_errors {
                for e in errors {
                    push_unique(&mut parts, e.message.trim());
                }
            }
            if parts.is_empty() {
                "MongoDB write failed.".into()
            } else {
                parts.join(" ")
            }
        }
        ErrorKind::InvalidArgument { message, .. }
        | ErrorKind::Authentication { message, .. }
        | ErrorKind::InvalidResponse { message, .. }
        | ErrorKind::ServerSelection { message, .. }
        | ErrorKind::Transaction { message, .. }
        | ErrorKind::IncompatibleServer { message, .. }
        | ErrorKind::DnsResolve { message, .. }
        | ErrorKind::ConnectionPoolCleared { message, .. } => nonempty_or(message, kind),
        ErrorKind::Io(io) => format!("Could not reach MongoDB: {io}"),
        other => other.to_string(),
    }
}

fn nonempty_or(message: &str, kind: &ErrorKind) -> String {
    let msg = message.trim();
    if msg.is_empty() {
        kind.to_string()
    } else {
        msg.to_string()
    }
}

fn push_unique(parts: &mut Vec<String>, msg: &str) {
    if !msg.is_empty() && !parts.iter().any(|p| p == msg) {
        parts.push(msg.to_string());
    }
}

pub(crate) fn query_failed(err: MongoError) -> ConnectorError {
    ConnectorError::Query(format_query_error(&err))
}
