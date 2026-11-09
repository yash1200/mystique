//! Known ACP agents: how to find each one and how to start it in ACP mode.
//!
//! One entry per agent, each with the page that documents its ACP entry
//! point (researched November 2026 from the ACP registry,
//! <https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json>,
//! and the vendors' docs). To add or fix an agent, edit [`CATALOG`] and its
//! row in `docs/ACP.md`; the tests below check every entry is complete.
//!
//! `confirmed: false` means no source shows the agent speaking ACP: it is
//! listed so the user sees it was looked for, but it cannot be added with a
//! click. The `mcp` column is documentation only: at run time the agent's
//! own `initialize` decides between HTTP and the stdio bridge.

/// How an agent starts in ACP mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaunchSpec {
    /// The CLI speaks ACP itself: `<cli> <args…>`.
    Native { args: &'static [&'static str] },
    /// A separate adapter program: its own binary when installed (`bins`),
    /// else the package through a runner (npx/bunx, or uvx).
    Adapter {
        bins: &'static [&'static str],
        package: Package,
        args: &'static [&'static str],
        /// Offered through the runner only when the CLI itself is installed
        /// (its login is what the adapter uses).
        needs_cli: bool,
    },
}

/// Where an adapter is published.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Package {
    /// npm: `npx -y <pkg>` (or `bunx <pkg>`).
    Npm(&'static str),
    /// PyPI: `uvx <pkg>`.
    #[allow(dead_code)]
    Uvx(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CatalogEntry {
    /// Stable key, also the id of the agent once added.
    pub id: &'static str,
    pub name: &'static str,
    /// Two letters for the icon tile.
    pub mark: &'static str,
    /// The CLI's binary names (the first found wins).
    pub bins: &'static [&'static str],
    pub launch: LaunchSpec,
    /// MCP transports it accepts in `session/new`, as documented or seen.
    pub mcp: &'static str,
    /// A source shows the agent speaking ACP.
    pub confirmed: bool,
    /// How to sign in (shown when Test finds it signed out).
    pub sign_in: &'static str,
    /// How to install it: a command to copy, or a sentence.
    pub install: &'static str,
    pub homepage: &'static str,
    /// The page that documents the ACP entry point.
    pub source: &'static str,
    /// Anything the user should know (empty for none).
    pub note: &'static str,
}

pub(crate) const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "claude",
        name: "Claude Code",
        mark: "Cl",
        bins: &["claude"],
        launch: LaunchSpec::Adapter {
            bins: &["claude-agent-acp", "claude-code-acp"],
            package: Package::Npm("@agentclientprotocol/claude-agent-acp"),
            args: &[],
            needs_cli: true,
        },
        mcp: "http, sse, stdio",
        confirmed: true,
        sign_in: "Run `claude` once in a terminal and log in, or set ANTHROPIC_API_KEY.",
        install: "npm install -g @agentclientprotocol/claude-agent-acp",
        homepage: "https://docs.anthropic.com/en/docs/claude-code/overview",
        source: "https://github.com/agentclientprotocol/claude-agent-acp",
        note: "Runs through its ACP adapter (formerly @zed-industries/claude-code-acp), which uses your Claude Code login.",
    },
    CatalogEntry {
        id: "codex",
        name: "Codex",
        mark: "Cx",
        bins: &["codex"],
        launch: LaunchSpec::Adapter {
            bins: &["codex-acp"],
            package: Package::Npm("@agentclientprotocol/codex-acp"),
            args: &[],
            needs_cli: true,
        },
        mcp: "http, stdio",
        confirmed: true,
        sign_in: "Run `codex` once in a terminal and log in, or set OPENAI_API_KEY (or CODEX_API_KEY).",
        install: "npm install -g @agentclientprotocol/codex-acp",
        homepage: "https://developers.openai.com/codex/cli",
        source: "https://github.com/agentclientprotocol/codex-acp",
        note: "Runs through the codex-acp adapter (formerly @zed-industries/codex-acp).",
    },
    CatalogEntry {
        id: "gemini",
        name: "Gemini CLI",
        mark: "Ge",
        bins: &["gemini"],
        launch: LaunchSpec::Native { args: &["--acp"] },
        mcp: "http, sse, stdio",
        confirmed: true,
        sign_in: "Run `gemini` once in a terminal and log in, or set GEMINI_API_KEY.",
        install: "npm install -g @google/gemini-cli",
        homepage: "https://github.com/google-gemini/gemini-cli",
        source: "https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/acp-mode.md",
        note: "Versions before --acp used --experimental-acp.",
    },
    CatalogEntry {
        id: "agy",
        name: "Antigravity CLI",
        mark: "Ag",
        bins: &["agy"],
        launch: LaunchSpec::Native { args: &["acp"] },
        mcp: "stdio",
        confirmed: true,
        sign_in: "Run `agy` once in a terminal and log in with Google, or run `agy acp --login`.",
        install: "curl -fsSL https://antigravity.google/cli/install.sh | bash",
        homepage: "https://antigravity.google",
        source: "https://antigravity.google/docs/cli/acp/",
        note: "`agy acp` needs agy 1.5 or later. It takes no HTTP MCP servers, so it reaches Mystique through the stdio bridge.",
    },
    CatalogEntry {
        id: "grok",
        name: "Grok Build",
        mark: "Gx",
        bins: &["grok"],
        launch: LaunchSpec::Native {
            args: &["agent", "stdio"],
        },
        mcp: "http, sse, stdio",
        confirmed: true,
        sign_in: "Run `grok login` in a terminal, or set XAI_API_KEY.",
        install: "npm install -g @xai-official/grok",
        homepage: "https://docs.x.ai/build/cli/headless-scripting",
        source: "https://docs.x.ai/build/cli/headless-scripting",
        note: "xAI's official CLI. The community grok-cli also installs `grok` but does not speak ACP.",
    },
    CatalogEntry {
        id: "cursor",
        name: "Cursor CLI",
        mark: "Cu",
        bins: &["cursor-agent"],
        launch: LaunchSpec::Native { args: &["acp"] },
        mcp: "unknown (reads .cursor/mcp.json)",
        confirmed: true,
        sign_in: "Run `cursor-agent login` in a terminal, or set CURSOR_API_KEY.",
        install: "curl https://cursor.com/install -fsS | bash",
        homepage: "https://cursor.com/cli",
        source: "https://cursor.com/docs/cli/acp",
        note: "Found by `cursor-agent` only: Grok Build also installs a binary called `agent`.",
    },
    CatalogEntry {
        id: "opencode",
        name: "OpenCode",
        mark: "Oc",
        bins: &["opencode"],
        launch: LaunchSpec::Native { args: &["acp"] },
        mcp: "http, stdio",
        confirmed: true,
        sign_in: "Run `opencode auth login` in a terminal.",
        install: "npm install -g @opencode/cli",
        homepage: "https://opencode.ai",
        source: "https://github.com/anomalyco/opencode",
        note: "",
    },
    CatalogEntry {
        id: "goose",
        name: "goose",
        mark: "Go",
        bins: &["goose"],
        launch: LaunchSpec::Native { args: &["acp"] },
        mcp: "http, stdio",
        confirmed: true,
        sign_in: "Run `goose configure` in a terminal to set up a provider.",
        install: "curl -fsSL https://github.com/aaif-goose/goose/releases/download/stable/download_cli.sh | bash",
        homepage: "https://goose-docs.ai",
        source: "https://github.com/aaif-goose/goose/blob/main/documentation/docs/guides/zed.md",
        note: "",
    },
    CatalogEntry {
        id: "kiro",
        name: "Kiro CLI",
        mark: "Ki",
        bins: &["kiro-cli"],
        launch: LaunchSpec::Native { args: &["acp"] },
        mcp: "unknown",
        confirmed: true,
        sign_in: "Run `kiro-cli login` in a terminal.",
        install: "curl -fsSL https://cli.kiro.dev/install | bash",
        homepage: "https://kiro.dev/cli/",
        source: "https://kiro.dev/docs/cli/acp/",
        note: "",
    },
    CatalogEntry {
        id: "qwen",
        name: "Qwen Code",
        mark: "Qw",
        bins: &["qwen"],
        launch: LaunchSpec::Native { args: &["--acp"] },
        mcp: "http, sse, stdio",
        confirmed: true,
        sign_in: "Run `qwen` once in a terminal and log in, or set OPENAI_API_KEY.",
        install: "npm install -g @qwen-code/qwen-code",
        homepage: "https://github.com/QwenLM/qwen-code",
        source: "https://github.com/QwenLM/qwen-code/blob/main/docs/users/integration-zed.md",
        note: "",
    },
    CatalogEntry {
        id: "auggie",
        name: "Auggie",
        mark: "Au",
        bins: &["auggie"],
        launch: LaunchSpec::Native { args: &["--acp"] },
        mcp: "unknown",
        confirmed: true,
        sign_in: "Run `auggie login` in a terminal.",
        install: "npm install -g @augmentcode/auggie",
        homepage: "https://www.augmentcode.com",
        source: "https://docs.augmentcode.com/cli/acp",
        note: "",
    },
    CatalogEntry {
        id: "copilot",
        name: "GitHub Copilot CLI",
        mark: "Gh",
        bins: &["copilot"],
        launch: LaunchSpec::Native { args: &["--acp"] },
        mcp: "unknown",
        confirmed: true,
        sign_in: "Run `copilot` once in a terminal and use /login, or set COPILOT_GITHUB_TOKEN.",
        install: "npm install -g @github/copilot",
        homepage: "https://github.com/github/copilot-cli",
        source: "https://docs.github.com/en/copilot/reference/acp-server",
        note: "ACP mode is in public preview.",
    },
    CatalogEntry {
        id: "kimi",
        name: "Kimi Code",
        mark: "Km",
        bins: &["kimi"],
        launch: LaunchSpec::Native { args: &["acp"] },
        mcp: "http, sse, stdio",
        confirmed: true,
        sign_in: "Run `kimi` once in a terminal and use /login.",
        install: "npm install -g @moonshot-ai/kimi-code",
        homepage: "https://github.com/MoonshotAI/kimi-code",
        source: "https://moonshotai.github.io/kimi-code/en/reference/kimi-acp",
        note: "",
    },
    CatalogEntry {
        id: "vibe",
        name: "Mistral Vibe",
        mark: "Mv",
        bins: &["vibe-acp"],
        launch: LaunchSpec::Native { args: &[] },
        mcp: "stdio",
        confirmed: true,
        sign_in: "Run `vibe --setup` in a terminal, or set MISTRAL_API_KEY.",
        install: "uv tool install mistral-vibe",
        homepage: "https://mistral.ai",
        source: "https://github.com/mistralai/mistral-vibe/blob/main/docs/acp-setup.md",
        note: "Advertises no HTTP MCP, so it reaches Mystique through the stdio bridge.",
    },
    CatalogEntry {
        id: "cline",
        name: "Cline",
        mark: "Cn",
        bins: &["cline"],
        launch: LaunchSpec::Native { args: &["--acp"] },
        mcp: "unknown",
        confirmed: true,
        sign_in: "Run `cline auth` in a terminal, or set CLINE_API_KEY.",
        install: "npm install -g cline",
        homepage: "https://cline.bot",
        source: "https://docs.cline.bot/usage/acp",
        note: "",
    },
    CatalogEntry {
        id: "junie",
        name: "Junie",
        mark: "Ju",
        bins: &["junie"],
        launch: LaunchSpec::Native {
            args: &["--acp=true"],
        },
        mcp: "unknown",
        confirmed: true,
        sign_in: "Run `junie` once in a terminal and log in.",
        install: "curl -fsSL https://junie.jetbrains.com/install.sh | bash",
        homepage: "https://junie.jetbrains.com",
        source: "https://junie.jetbrains.com/docs/junie-cli-acp.html",
        note: "",
    },
    CatalogEntry {
        id: "droid",
        name: "Factory Droid",
        mark: "Dr",
        bins: &["droid"],
        launch: LaunchSpec::Native {
            args: &["exec", "--output-format", "acp"],
        },
        mcp: "unknown",
        confirmed: true,
        sign_in: "Run `droid` once in a terminal and log in, or set FACTORY_API_KEY.",
        install: "curl -fsSL https://app.factory.ai/cli | sh",
        homepage: "https://factory.ai",
        source: "https://docs.factory.com/integrations/zed.md",
        note: "",
    },
    CatalogEntry {
        id: "qoder",
        name: "Qoder CLI",
        mark: "Qo",
        bins: &["qodercli", "qoder"],
        launch: LaunchSpec::Native { args: &["--acp"] },
        mcp: "http, sse, stdio",
        confirmed: true,
        sign_in: "Run `qoder login` in a terminal, or set QODER_PERSONAL_ACCESS_TOKEN.",
        install: "npm install -g @qoder-ai/qodercli",
        homepage: "https://qoder.com",
        source: "https://docs.qoder.com/cli/acp",
        note: "",
    },
    CatalogEntry {
        id: "amp",
        name: "Amp",
        mark: "Am",
        bins: &["amp"],
        launch: LaunchSpec::Adapter {
            bins: &["amp-acp"],
            package: Package::Npm("amp-acp"),
            args: &[],
            needs_cli: true,
        },
        mcp: "unknown",
        confirmed: true,
        sign_in: "Run `amp login` in a terminal, or set AMP_API_KEY.",
        install: "npm install -g amp-acp",
        homepage: "https://ampcode.com",
        source: "https://github.com/tao12345666333/amp-acp",
        note: "Runs through a community adapter; Amp has no ACP mode of its own.",
    },
    CatalogEntry {
        id: "openhands",
        name: "OpenHands",
        mark: "Oh",
        bins: &["openhands"],
        launch: LaunchSpec::Native { args: &["acp"] },
        mcp: "unknown",
        confirmed: true,
        sign_in: "Run `openhands` once in a terminal and set it up with /settings.",
        install: "uv tool install openhands",
        homepage: "https://openhands.dev",
        source: "https://docs.openhands.dev/openhands/usage/run-openhands/acp",
        note: "Its docs say the OpenHands CLI is no longer actively maintained.",
    },
];

pub(crate) fn find(id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_every_entry_is_complete() {
        let mut ids = HashSet::new();
        let mut bins = HashSet::new();
        for e in CATALOG {
            assert!(ids.insert(e.id), "duplicate id {}", e.id);
            assert!(
                e.id.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{}",
                e.id
            );
            assert_ne!(e.id, crate::settings::BUILTIN);
            assert!(!e.id.starts_with("custom"), "{}", e.id);
            assert!(!e.name.trim().is_empty(), "{}", e.id);
            assert_eq!(e.mark.chars().count(), 2, "{}", e.id);
            assert!(!e.bins.is_empty(), "{}: no binary to detect", e.id);
            for bin in e.bins {
                assert!(bins.insert(*bin), "{}: `{bin}` is claimed twice", e.id);
            }
            assert!(e.source.starts_with("https://"), "{}: no source URL", e.id);
            assert!(e.homepage.starts_with("https://"), "{}", e.id);
            assert!(!e.install.trim().is_empty(), "{}: no install hint", e.id);
            assert!(!e.sign_in.trim().is_empty(), "{}", e.id);
            assert!(!e.mcp.trim().is_empty(), "{}", e.id);
            if let LaunchSpec::Adapter {
                bins: adapters,
                package,
                ..
            } = e.launch
            {
                assert!(!adapters.is_empty(), "{}", e.id);
                let (Package::Npm(p) | Package::Uvx(p)) = package;
                assert!(!p.trim().is_empty(), "{}", e.id);
            }
            if !e.confirmed {
                assert!(
                    e.note.contains("not confirmed"),
                    "{}: say plainly that ACP support is not confirmed",
                    e.id
                );
            }
        }
    }

    #[test]
    fn the_user_s_examples_are_listed() {
        for id in ["claude", "agy", "codex", "grok", "cursor", "opencode"] {
            assert!(find(id).is_some(), "{id}");
        }
    }
}
