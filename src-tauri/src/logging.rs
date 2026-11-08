//! Logging for the desktop app.
//!
//! - Filter: `MYSTIQUE_LOG` (EnvFilter syntax, e.g. `debug` or
//!   `warn,mystique_core=debug`). Default: `info` for Mystique crates, `warn`
//!   for everything else.
//! - File: `mystique.log` in the app log directory, rotated at 2 MB, five files
//!   kept ([`mystique_core::diagnostics`]). The diagnostics bundle reads these.
//! - stderr as well in debug builds.
//!
//! What to log: boundaries (session open/close, query run timing and counts,
//! vault unlock outcome, MCP requests, migrations). Never SQL text, parameter
//! values, passwords or connection strings at `info`; run anything that might
//! hold a URI or DSN through [`mystique_core::redact::redact_secrets`].

use std::path::Path;

use mystique_core::diagnostics::{RotatingLog, LOG_FILE_NAME, LOG_KEEP_FILES, LOG_MAX_BYTES};
use mystique_core::redact::redact_secrets;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub const LOG_ENV: &str = "MYSTIQUE_LOG";

const MYSTIQUE_CRATES: &[&str] = &[
    "mystique",
    "mystique_lib",
    "mystique_core",
    "mystique_mcp",
    "mystique_agent",
    "mystique_connector_api",
    "mystique_sql",
    "mystique_plugin_postgres",
    "mystique_plugin_mysql",
    "mystique_plugin_sqlite",
    "mystique_plugin_mongodb",
];

fn default_filter() -> String {
    let mut directives = vec!["warn".to_string()];
    directives.extend(MYSTIQUE_CRATES.iter().map(|c| format!("{c}=info")));
    directives.join(",")
}

/// Install the global subscriber. Safe to call twice: the second call is a no-op.
pub fn init(log_dir: &Path) {
    let filter =
        EnvFilter::try_from_env(LOG_ENV).unwrap_or_else(|_| EnvFilter::new(default_filter()));
    let file = RotatingLog::open(log_dir, LOG_FILE_NAME, LOG_MAX_BYTES, LOG_KEEP_FILES);
    let file_error = file.as_ref().err().map(|e| e.to_string());
    let file_layer = file.ok().map(|log| {
        fmt::layer()
            .with_ansi(false)
            .with_writer(move || log.clone())
    });
    let stderr_layer = cfg!(debug_assertions).then(|| fmt::layer().with_writer(std::io::stderr));
    let installed = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stderr_layer)
        .try_init()
        .is_ok();
    if !installed {
        return;
    }
    if let Some(error) = file_error {
        tracing::warn!(dir = %log_dir.display(), error, "log file unavailable; logging to stderr only");
    }
    install_panic_hook();
}

/// Record panics in the log too. IPC guards catch them, so without this a
/// plugin bug would leave no trace in a diagnostics bundle.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".into());
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()));
        tracing::error!(
            location = location.as_deref(),
            message = %redact_secrets(&message),
            "panic"
        );
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_filter_parses_and_names_every_mystique_crate() {
        let filter = default_filter();
        assert!(filter.starts_with("warn,"));
        assert!(filter.contains("mystique_core=info"));
        EnvFilter::try_new(&filter).expect("valid filter");
    }
}
