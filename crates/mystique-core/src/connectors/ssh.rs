//! SSH local-forward transport. Plugins still connect to 127.0.0.1.

mod rewrite;
mod tunnel;

pub use rewrite::{rewrite_plugin_config_for_tunnel, ssh_target};
pub use tunnel::{open_tunnel, SshTunnel};
