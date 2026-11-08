//! User-facing MongoDB query errors (the driver Display dumps Kind/labels).

use mongodb::error::{Error as MongoError, ErrorKind, WriteFailure};
use mystique_connector_api::{prettify_sql_error, ConnectorError};

pub(crate) fn format_query_error(err: &MongoError) -> String {
    prettify_sql_error(&format_kind(err.kind.as_ref()))
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

/// Ends a query error when the server aborted the open transaction.
pub(crate) const TXN_ABORTED: &str = "MongoDB aborted the open transaction.";

pub(crate) fn query_failed(err: MongoError) -> ConnectorError {
    let msg = format_query_error(&err);
    if transaction_aborted(&err) {
        return ConnectorError::Query(format!("{msg} {TXN_ABORTED}"));
    }
    ConnectorError::Query(msg)
}

/// The server dropped the transaction (write conflict, lifetime limit, no
/// such transaction). Nothing in it was committed.
pub(crate) fn transaction_aborted(err: &MongoError) -> bool {
    err.contains_label(mongodb::error::TRANSIENT_TRANSACTION_ERROR)
        || matches!(err.kind.as_ref(), ErrorKind::Command(cmd) if cmd.code == 251)
}

/// Privilege denial (code 13) or Atlas “not authorized on admin”.
pub(crate) fn is_unauthorized(err: &MongoError) -> bool {
    match err.kind.as_ref() {
        ErrorKind::Command(cmd) => {
            cmd.code == 13
                || cmd.code_name.eq_ignore_ascii_case("Unauthorized")
                || cmd.message.to_ascii_lowercase().contains("not authorized")
        }
        _ => format_query_error(err)
            .to_ascii_lowercase()
            .contains("not authorized"),
    }
}

#[cfg(test)]
mod tests {
    use mystique_connector_api::prettify_sql_error;

    #[test]
    fn duplicate_key_is_short() {
        let raw = "E11000 duplicate key error collection: app.users index: email_1 dup key: { email: \"a@b.c\" }";
        assert_eq!(
            prettify_sql_error(raw),
            "A row with this value already exists."
        );
    }
}
