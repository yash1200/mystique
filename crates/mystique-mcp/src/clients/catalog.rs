//! Known MCP hosts and how to detect / write each one.

use std::path::PathBuf;

use mystique_core::{AppError, AppResult};

use super::formats::{read_state, write_entry, FileKind};
use super::host::Host;
use super::{McpEndpoint, McpHarness};

#[derive(Clone, Copy)]
enum ConfigLoc {
    Home(&'static str),
    UserConfig(&'static str),
}

struct Spec {
    id: &'static str,
    label: &'static str,
    mark: &'static str,
    hint: &'static str,
    loc: ConfigLoc,
    kind: FileKind,
    bins: &'static [&'static str],
    apps: &'static [&'static str],
    /// Extra relative paths under home that mean the product is installed.
    home_markers: &'static [&'static str],
}

const CATALOG: &[Spec] = &[
    Spec {
        id: "claude",
        label: "Claude Code",
        mark: "Cl",
        hint: "CLI",
        loc: ConfigLoc::Home(".claude.json"),
        kind: FileKind::ClaudeHttp,
        bins: &["claude"],
        apps: &[],
        home_markers: &[".claude.json", ".claude"],
    },
    Spec {
        id: "agy",
        label: "Antigravity",
        mark: "Ag",
        hint: "agy CLI",
        loc: ConfigLoc::Home(".gemini/config/mcp_config.json"),
        kind: FileKind::AgyJson,
        bins: &["agy"],
        apps: &["Antigravity.app", "Antigravity IDE.app"],
        home_markers: &[".gemini/config/mcp_config.json", ".gemini/antigravity"],
    },
    Spec {
        id: "grok",
        label: "Grok",
        mark: "Gx",
        hint: "CLI",
        loc: ConfigLoc::Home(".grok/config.toml"),
        kind: FileKind::GrokToml,
        bins: &["grok"],
        apps: &[],
        home_markers: &[".grok/config.toml", ".grok"],
    },
    Spec {
        id: "codex",
        label: "Codex",
        mark: "Cx",
        hint: "CLI",
        loc: ConfigLoc::Home(".codex/config.toml"),
        kind: FileKind::CodexToml,
        bins: &["codex"],
        apps: &[],
        home_markers: &[".codex/config.toml", ".codex"],
    },
    Spec {
        id: "cursor",
        label: "Cursor",
        mark: "Cu",
        hint: "Editor",
        loc: ConfigLoc::Home(".cursor/mcp.json"),
        kind: FileKind::CursorJson,
        bins: &["cursor", "cursor-agent"],
        apps: &["Cursor.app"],
        home_markers: &[".cursor/mcp.json", ".cursor"],
    },
    Spec {
        id: "gemini",
        label: "Gemini CLI",
        mark: "Ge",
        hint: "CLI",
        loc: ConfigLoc::Home(".gemini/settings.json"),
        kind: FileKind::GeminiJson,
        bins: &["gemini"],
        apps: &[],
        // Shared with Antigravity — only the `gemini` binary means Gemini CLI.
        home_markers: &[],
    },
    Spec {
        id: "opencode",
        label: "OpenCode",
        mark: "Oc",
        hint: "CLI",
        loc: ConfigLoc::Home(".config/opencode/opencode.json"),
        kind: FileKind::OpenCodeJson,
        bins: &["opencode"],
        apps: &[],
        home_markers: &[".config/opencode/opencode.json", ".config/opencode"],
    },
    Spec {
        id: "vscode",
        label: "VS Code",
        mark: "Vs",
        hint: "Editor",
        loc: ConfigLoc::UserConfig("Code/User/mcp.json"),
        kind: FileKind::VsCodeJson,
        bins: &["code"],
        apps: &["Visual Studio Code.app"],
        home_markers: &[],
    },
    Spec {
        id: "claude-desktop",
        label: "Claude Desktop",
        mark: "Cd",
        hint: "App",
        loc: ConfigLoc::UserConfig("Claude/claude_desktop_config.json"),
        kind: FileKind::ClaudeHttp,
        bins: &[],
        apps: &["Claude.app"],
        home_markers: &[],
    },
    Spec {
        id: "windsurf",
        label: "Windsurf",
        mark: "Ws",
        hint: "Editor",
        loc: ConfigLoc::Home(".codeium/windsurf/mcp_config.json"),
        kind: FileKind::CursorJson,
        bins: &["windsurf"],
        apps: &["Windsurf.app"],
        home_markers: &[".codeium/windsurf/mcp_config.json", ".codeium/windsurf"],
    },
];

impl Spec {
    fn config_path(&self, host: &Host) -> PathBuf {
        match self.loc {
            ConfigLoc::Home(rel) => host.home_path(rel),
            ConfigLoc::UserConfig(rel) => host.user_config_path(rel),
        }
    }

    fn available(&self, host: &Host) -> bool {
        self.bins.iter().any(|b| host.bin_on_path(b))
            || self.apps.iter().any(|a| host.app_installed(a))
            || self
                .home_markers
                .iter()
                .any(|rel| host.home_path(rel).exists())
            || user_config_present(self, host)
    }

    fn to_harness(&self, host: &Host, endpoint: &McpEndpoint) -> McpHarness {
        let path = self.config_path(host);
        let state = read_state(&path, self.kind, endpoint);
        McpHarness {
            id: self.id.to_string(),
            label: self.label.to_string(),
            mark: self.mark.to_string(),
            hint: self.hint.to_string(),
            config_path: host.display_path(&path),
            installed: state.installed,
            stale: state.stale,
        }
    }
}

fn user_config_present(spec: &Spec, host: &Host) -> bool {
    matches!(spec.loc, ConfigLoc::UserConfig(_)) && spec.config_path(host).exists()
}

pub fn list_available(host: &Host, endpoint: &McpEndpoint) -> Vec<McpHarness> {
    CATALOG
        .iter()
        .filter(|spec| spec.available(host))
        .map(|spec| spec.to_harness(host, endpoint))
        .collect()
}

pub fn install(host: &Host, id: &str, endpoint: &McpEndpoint) -> AppResult<McpHarness> {
    let spec = CATALOG
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| AppError::Invalid(format!("Unknown AI tool: {id}")))?;
    if !spec.available(host) {
        return Err(AppError::Invalid(format!(
            "{} is not installed on this machine",
            spec.label
        )));
    }
    write_entry(&spec.config_path(host), spec.kind, endpoint)?;
    Ok(spec.to_harness(host, endpoint))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn ep() -> McpEndpoint {
        McpEndpoint {
            url: "http://127.0.0.1:17321/mcp".into(),
            token: "secret".into(),
        }
    }

    fn setup() -> (tempfile::TempDir, Host) {
        let dir = tempfile::tempdir().unwrap();
        let host = Host::for_test(dir.path());
        fs::create_dir_all(&host.home).unwrap();
        fs::create_dir_all(&host.path_dirs[0]).unwrap();
        fs::create_dir_all(&host.app_dirs[0]).unwrap();
        fs::create_dir_all(&host.user_config).unwrap();
        (dir, host)
    }

    #[test]
    fn list_available_is_empty_on_bare_host() {
        let (_dir, host) = setup();
        assert!(list_available(&host, &ep()).is_empty());
    }

    #[test]
    fn lists_only_detected_tools() {
        let (_dir, host) = setup();
        fs::write(host.path_dirs[0].join("claude"), "").unwrap();
        fs::write(host.path_dirs[0].join("grok"), "").unwrap();
        let ids: Vec<_> = list_available(&host, &ep())
            .into_iter()
            .map(|h| h.id)
            .collect();
        assert_eq!(ids, vec!["claude", "grok"]);
    }

    #[test]
    fn gemini_is_not_inferred_from_shared_gemini_dir() {
        let (_dir, host) = setup();
        fs::create_dir_all(host.home_path(".gemini/antigravity")).unwrap();
        let ids: Vec<_> = list_available(&host, &ep())
            .into_iter()
            .map(|h| h.id)
            .collect();
        assert_eq!(ids, vec!["agy"]);
        assert!(!ids.iter().any(|id| id == "gemini"));
    }

    #[test]
    fn install_writes_and_marks_added() {
        let (_dir, host) = setup();
        fs::write(host.path_dirs[0].join("agy"), "").unwrap();
        let added = install(&host, "agy", &ep()).unwrap();
        assert!(added.installed);
        assert!(!added.stale);
        assert!(host.home_path(".gemini/config/mcp_config.json").exists());
    }

    #[test]
    fn install_rejects_missing_tool() {
        let (_dir, host) = setup();
        let err = install(&host, "claude", &ep()).unwrap_err();
        assert!(err.to_string().contains("not installed"));
    }

    #[test]
    fn vscode_detected_from_app_bundle() {
        let (_dir, host) = setup();
        fs::create_dir_all(host.app_dirs[0].join("Visual Studio Code.app")).unwrap();
        let ids: Vec<_> = list_available(&host, &ep())
            .into_iter()
            .map(|h| h.id)
            .collect();
        assert_eq!(ids, vec!["vscode"]);
    }
}
