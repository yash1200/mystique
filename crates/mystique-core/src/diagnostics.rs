//! Log files and the diagnostics bundle a user attaches to a bug report.
//!
//! The shell installs the `tracing` subscriber and writes through
//! [`RotatingLog`]: `mystique.log` plus up to four older files, 2 MB each.
//! [`build_bundle`] packs a `.tar.gz` a user can attach to an issue. See
//! [`bundle`] for what goes in and what never does.

pub mod bundle;
mod rotating;

pub use bundle::{build_bundle, default_bundle_name, BundleContext, BUNDLE_ROOT};
pub use rotating::{log_files, RotatingLog};

/// Current log file in the app log directory.
pub const LOG_FILE_NAME: &str = "mystique.log";
/// A log file is rotated once it would pass this size.
pub const LOG_MAX_BYTES: u64 = 2 * 1024 * 1024;
/// Files kept, the current one included.
pub const LOG_KEEP_FILES: usize = 5;
