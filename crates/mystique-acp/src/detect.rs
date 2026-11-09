//! Which ACP agents are on this machine.
//!
//! An app started from Finder gets a minimal `PATH`, so the search also
//! looks where installers put CLIs (Homebrew, npm, bun, cargo, volta, nvm,
//! deno, pnpm, `~/.local/bin`, …) and, once per app session, asks the
//! user's login shell for its `PATH`. Every helper process (the login
//! shell, `npm prefix -g`, `<cli> --version`) runs without a shell of ours,
//! with stdin closed, in its own process group, under a hard time limit.
//! Nothing is ever installed.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::catalog::{self, CatalogEntry, LaunchSpec, Package};

/// The login-shell `PATH` probe.
pub(crate) const SHELL_TIMEOUT: Duration = Duration::from_secs(2);
/// `<cli> --version` and `npm prefix -g`.
pub(crate) const VERSION_TIMEOUT: Duration = Duration::from_millis(1500);

const PATH_MARK: &str = "__MYSTIQUE_PATH__";

/// Where an agent stands on this machine.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum AcpAgentStatus {
    /// Its ACP entry point is a local binary.
    Installed,
    /// The CLI is here; its ACP adapter runs through npx, bunx or uvx.
    Adapter,
    /// The CLI is here, but no source shows it speaking ACP.
    Unconfirmed,
    /// Not found.
    Missing,
}

/// One catalog agent, as found on this machine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct DetectedAcpAgent {
    pub catalog_id: String,
    pub name: String,
    /// Two letters for the icon tile.
    pub mark: String,
    pub status: AcpAgentStatus,
    /// The binary found (the CLI or its adapter), full path.
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    /// What Add saves: the full path of the program and its arguments.
    #[serde(default)]
    pub command: Option<String>,
    pub args: Vec<String>,
    /// MCP transports the agent is documented to accept.
    pub mcp: String,
    pub confirmed: bool,
    /// How to sign in.
    pub sign_in: String,
    /// How to install it (a command to copy, or a sentence).
    pub install: String,
    pub homepage: String,
    pub source: String,
    #[serde(default)]
    pub note: Option<String>,
    /// The id of the added agent made from this entry, if any.
    #[serde(default)]
    pub added_id: Option<String>,
}

/// The machine as detection sees it. Tests point it at a fake home.
#[derive(Debug, Clone)]
pub(crate) struct Machine {
    pub home: Option<PathBuf>,
    pub path: Option<OsString>,
    /// The user's login shell (`$SHELL`).
    pub shell: Option<PathBuf>,
    /// Windows `%APPDATA%` / `%LOCALAPPDATA%`.
    pub app_data: Option<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub shell_timeout: Duration,
    pub version_timeout: Duration,
}

impl Machine {
    pub(crate) fn current() -> Self {
        Self {
            home: home_dir(),
            path: std::env::var_os("PATH"),
            shell: std::env::var_os("SHELL").map(PathBuf::from),
            app_data: std::env::var_os("APPDATA").map(PathBuf::from),
            local_app_data: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
            shell_timeout: SHELL_TIMEOUT,
            version_timeout: VERSION_TIMEOUT,
        }
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

/// The login shell's `PATH`, asked once per app session.
fn cached_login_path(machine: &Machine) -> Option<String> {
    // Unit tests never start the developer's own shell.
    if cfg!(test) {
        return None;
    }
    static LOGIN_PATH: OnceLock<Option<String>> = OnceLock::new();
    LOGIN_PATH.get_or_init(|| login_shell_path(machine)).clone()
}

/// `$SHELL -lic 'printf …"$PATH"…'`, read between markers so a chatty
/// profile cannot corrupt it. Only ever used as a list of folders. `None`
/// on any failure, including the time limit.
pub(crate) fn login_shell_path(machine: &Machine) -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    let shell = machine.shell.as_ref().filter(|s| s.is_absolute())?;
    let script = format!("printf '{PATH_MARK}%s{PATH_MARK}' \"$PATH\"");
    let mut cmd = Command::new(shell);
    cmd.args(["-l", "-i", "-c", &script]);
    if let Some(home) = &machine.home {
        cmd.current_dir(home);
    }
    let out = run_captured(cmd, machine.shell_timeout)?;
    let start = out.find(PATH_MARK)? + PATH_MARK.len();
    let rest = out.get(start..)?;
    let end = rest.find(PATH_MARK)?;
    let path = rest.get(..end)?.trim().to_string();
    (!path.is_empty()).then_some(path)
}

/// Run `cmd` with stdin closed and stderr dropped, in its own process
/// group; return its stdout if it exits successfully within `limit`. On
/// timeout the whole group is killed and the reader thread is abandoned.
pub(crate) fn run_captured(mut cmd: Command, limit: Duration) -> Option<String> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.by_ref().take(256 * 1024).read_to_end(&mut out);
        let _ = tx.send(out);
    });
    let started = std::time::Instant::now();
    let out = rx.recv_timeout(limit);
    // stdout closed (or time is up): the process should be done.
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if out.is_ok() && started.elapsed() < limit => {
                std::thread::sleep(Duration::from_millis(5));
            }
            _ => break None,
        }
    };
    match (out, status) {
        (Ok(out), Some(status)) if status.success() => {
            kill_group(&mut child);
            Some(String::from_utf8_lossy(&out).to_string())
        }
        _ => {
            kill_group(&mut child);
            None
        }
    }
}

fn kill_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    if let Ok(pid) = libc::pid_t::try_from(child.id()) {
        // SAFETY: killpg only sends a signal; the group is the child's own
        // (process_group(0)), never 0 or ours.
        if pid > 1 {
            unsafe {
                libc::killpg(pid, libc::SIGKILL);
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn push_unique(out: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, dir: PathBuf) {
    if dir.as_os_str().is_empty() || !dir.is_absolute() {
        return;
    }
    if seen.insert(dir.clone()) {
        out.push(dir);
    }
}

/// `~/.nvm/versions/node/v<semver>/bin` of the newest version.
fn newest_nvm_bin(home: &Path) -> Option<PathBuf> {
    let root = home.join(".nvm").join("versions").join("node");
    let mut best: Option<(Vec<u64>, PathBuf)> = None;
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(version) = name.strip_prefix('v') else {
            continue;
        };
        let parts: Vec<u64> = version.split('.').map(|p| p.parse().unwrap_or(0)).collect();
        if best.as_ref().is_none_or(|(b, _)| parts > *b) {
            best = Some((parts, entry.path().join("bin")));
        }
    }
    best.map(|(_, path)| path)
}

/// Folders installers commonly use, beyond `PATH`.
fn well_known_dirs(machine: &Machine) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if cfg!(unix) {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
        dirs.push(PathBuf::from("/home/linuxbrew/.linuxbrew/bin"));
    }
    if let Some(home) = &machine.home {
        for rel in [
            ".local/bin",
            ".npm-global/bin",
            ".bun/bin",
            ".cargo/bin",
            ".volta/bin",
            ".deno/bin",
            "Library/pnpm",
            ".local/share/pnpm",
            ".opencode/bin",
            ".grok/bin",
        ] {
            dirs.push(home.join(rel));
        }
        if let Some(nvm) = newest_nvm_bin(home) {
            dirs.push(nvm);
        }
        for rel in [
            ".local/share/fnm/aliases/default/bin",
            "Library/Application Support/fnm/aliases/default/bin",
        ] {
            dirs.push(home.join(rel));
        }
    }
    if cfg!(windows) {
        if let Some(app_data) = &machine.app_data {
            dirs.push(app_data.join("npm"));
        }
        if let Some(local) = &machine.local_app_data {
            dirs.push(local.join("pnpm"));
            dirs.push(local.join("Volta").join("bin"));
        }
        if let Some(home) = &machine.home {
            dirs.push(home.join("scoop").join("shims"));
        }
    }
    dirs
}

/// Every folder to look in, in order: `PATH`, the login shell's `PATH`,
/// the well-known folders, then npm's global bin (asked only when `npm`
/// itself was found).
pub(crate) fn search_dirs(machine: &Machine, login_path: Option<&str>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    if let Some(path) = &machine.path {
        for dir in std::env::split_paths(path) {
            push_unique(&mut out, &mut seen, dir);
        }
    }
    if let Some(path) = login_path {
        for dir in std::env::split_paths(OsStr::new(path)) {
            push_unique(&mut out, &mut seen, dir);
        }
    }
    for dir in well_known_dirs(machine) {
        push_unique(&mut out, &mut seen, dir);
    }
    if let Some(npm) = find_in(&out, "npm") {
        if let Some(bin) = npm_global_bin(&npm, &out, machine.version_timeout) {
            push_unique(&mut out, &mut seen, bin);
        }
    }
    out
}

/// `npm prefix -g` → its `bin` folder (the prefix itself on Windows).
fn npm_global_bin(npm: &Path, dirs: &[PathBuf], limit: Duration) -> Option<PathBuf> {
    let mut cmd = Command::new(npm);
    cmd.args(["prefix", "-g"]);
    if let Ok(path) = std::env::join_paths(dirs) {
        // npm is a node script: it needs node on PATH.
        cmd.env("PATH", path);
    }
    let prefix = run_captured(cmd, limit)?;
    let prefix = PathBuf::from(prefix.trim());
    if !prefix.is_absolute() {
        return None;
    }
    Some(if cfg!(windows) {
        prefix
    } else {
        prefix.join("bin")
    })
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    true
}

/// The first `name` in `dirs` (with `.exe`/`.cmd`/`.bat` on Windows).
pub(crate) fn find_in(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    let names: Vec<String> = if cfg!(windows) {
        ["exe", "cmd", "bat"]
            .iter()
            .map(|ext| format!("{name}.{ext}"))
            .chain(std::iter::once(name.to_string()))
            .collect()
    } else {
        vec![name.to_string()]
    };
    dirs.iter()
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|candidate| is_executable(candidate))
}

/// `<binary> --version`, first line, if it answers in time.
fn version_of(binary: &Path, dirs: &[PathBuf], machine: &Machine) -> Option<String> {
    let mut cmd = Command::new(binary);
    cmd.arg("--version");
    if let Ok(path) = std::env::join_paths(dirs) {
        cmd.env("PATH", path);
    }
    if let Some(home) = &machine.home {
        cmd.current_dir(home);
    }
    let out = run_captured(cmd, machine.version_timeout)?;
    let line = out.lines().map(str::trim).find(|l| !l.is_empty())?;
    let line: String = line.chars().take(80).collect();
    Some(line)
}

fn display(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

/// How `entry` would start here, if it can.
fn resolve(entry: &CatalogEntry, dirs: &[PathBuf]) -> Resolved {
    let base = entry.bins.iter().find_map(|b| find_in(dirs, b));
    if !entry.confirmed {
        return match base {
            Some(path) => Resolved {
                status: AcpAgentStatus::Unconfirmed,
                found: Some(path),
                command: None,
                args: Vec::new(),
            },
            None => Resolved::missing(),
        };
    }
    match entry.launch {
        LaunchSpec::Native { args } => match base {
            Some(path) => Resolved {
                status: AcpAgentStatus::Installed,
                command: Some(path.clone()),
                found: Some(path),
                args: args.iter().map(|a| a.to_string()).collect(),
            },
            None => Resolved::missing(),
        },
        LaunchSpec::Adapter {
            bins,
            package,
            args,
            needs_cli,
        } => {
            let extra = args.iter().map(|a| a.to_string());
            if let Some(adapter) = bins.iter().find_map(|b| find_in(dirs, b)) {
                return Resolved {
                    status: AcpAgentStatus::Installed,
                    command: Some(adapter.clone()),
                    found: Some(adapter),
                    args: extra.collect(),
                };
            }
            if needs_cli && base.is_none() {
                return Resolved::missing();
            }
            let runner = match package {
                Package::Npm(pkg) => find_in(dirs, "npx")
                    .map(|npx| (npx, vec!["-y".to_string(), pkg.to_string()]))
                    .or_else(|| find_in(dirs, "bunx").map(|bunx| (bunx, vec![pkg.to_string()]))),
                Package::Uvx(pkg) => find_in(dirs, "uvx").map(|uvx| (uvx, vec![pkg.to_string()])),
            };
            match runner {
                Some((runner, mut runner_args)) => {
                    runner_args.extend(extra);
                    Resolved {
                        status: AcpAgentStatus::Adapter,
                        found: base,
                        command: Some(runner),
                        args: runner_args,
                    }
                }
                None => Resolved::missing(),
            }
        }
    }
}

struct Resolved {
    status: AcpAgentStatus,
    /// The binary to show (and ask for its version).
    found: Option<PathBuf>,
    command: Option<PathBuf>,
    args: Vec<String>,
}

impl Resolved {
    fn missing() -> Self {
        Self {
            status: AcpAgentStatus::Missing,
            found: None,
            command: None,
            args: Vec::new(),
        }
    }
}

/// Look for every catalog agent. Versions are asked in parallel.
pub(crate) fn detect_with(machine: &Machine, login_path: Option<&str>) -> Vec<DetectedAcpAgent> {
    let dirs = search_dirs(machine, login_path);
    let resolved: Vec<(&CatalogEntry, Resolved)> = catalog::CATALOG
        .iter()
        .map(|entry| (entry, resolve(entry, &dirs)))
        .collect();
    let versions: Vec<Option<String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = resolved
            .iter()
            .map(|(_, r)| {
                let found = r.found.clone();
                let dirs = &dirs;
                scope.spawn(move || found.and_then(|p| version_of(&p, dirs, machine)))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().ok().flatten())
            .collect()
    });
    resolved
        .into_iter()
        .zip(versions)
        .map(|((entry, r), version)| DetectedAcpAgent {
            catalog_id: entry.id.into(),
            name: entry.name.into(),
            mark: entry.mark.into(),
            status: r.status,
            path: r.found.as_deref().map(display),
            version,
            command: r.command.as_deref().map(display),
            args: r.args,
            mcp: entry.mcp.into(),
            confirmed: entry.confirmed,
            sign_in: entry.sign_in.into(),
            install: entry.install.into(),
            homepage: entry.homepage.into(),
            source: entry.source.into(),
            note: (!entry.note.is_empty()).then(|| entry.note.to_string()),
            added_id: None,
        })
        .collect()
}

/// Detection on this machine (the login-shell probe runs once per session).
pub(crate) fn detect() -> Vec<DetectedAcpAgent> {
    let machine = Machine::current();
    let login = cached_login_path(&machine);
    detect_with(&machine, login.as_deref())
}

/// What Add saves for `entry`: the program's full path and its arguments.
/// No version check, so it is quick.
pub(crate) fn launch_for(entry: &CatalogEntry) -> Result<(String, Vec<String>), String> {
    let machine = Machine::current();
    let login = cached_login_path(&machine);
    let dirs = search_dirs(&machine, login.as_deref());
    let r = resolve(entry, &dirs);
    match (r.status, r.command) {
        (AcpAgentStatus::Installed | AcpAgentStatus::Adapter, Some(command)) => {
            Ok((display(&command), r.args))
        }
        (AcpAgentStatus::Unconfirmed, _) => Err(format!(
            "{}: ACP support not confirmed, so it cannot be added with a click. Use Custom agent.",
            entry.name
        )),
        _ => Err(format!(
            "{} was not found on this machine. Install it ({}), then Rescan.",
            entry.name, entry.install
        )),
    }
}

/// `PATH` for a launched agent: `base` (the app's or the user's), then the
/// login shell's and the well-known folders, so `npx` finds `node`.
pub(crate) fn launch_path(base: &str, program_dir: Option<&Path>) -> String {
    let machine = Machine {
        path: Some(OsString::from(base)),
        ..Machine::current()
    };
    let login = cached_login_path(&machine);
    let mut dirs = Vec::new();
    let mut seen = HashSet::new();
    if let Some(dir) = program_dir {
        push_unique(&mut dirs, &mut seen, dir.to_path_buf());
    }
    for dir in std::env::split_paths(OsStr::new(base)) {
        push_unique(&mut dirs, &mut seen, dir);
    }
    if let Some(login) = login.as_deref() {
        for dir in std::env::split_paths(OsStr::new(login)) {
            push_unique(&mut dirs, &mut seen, dir);
        }
    }
    for dir in well_known_dirs(&machine) {
        if dir.is_dir() {
            push_unique(&mut dirs, &mut seen, dir);
        }
    }
    std::env::join_paths(dirs)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| base.to_string())
}

/// Find `program` (a bare name) the way detection would.
pub(crate) fn find_program(program: &str, path: &str) -> Option<PathBuf> {
    let dirs: Vec<PathBuf> = std::env::split_paths(OsStr::new(path)).collect();
    find_in(&dirs, program)
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Instant;

    fn stub(dir: &Path, name: &str, script: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn machine(home: &Path, path: &[&Path]) -> Machine {
        Machine {
            home: Some(home.to_path_buf()),
            path: Some(std::env::join_paths(path).unwrap()),
            shell: None,
            app_data: None,
            local_app_data: None,
            shell_timeout: Duration::from_millis(500),
            version_timeout: Duration::from_millis(1500),
        }
    }

    fn find<'a>(list: &'a [DetectedAcpAgent], id: &str) -> &'a DetectedAcpAgent {
        list.iter().find(|a| a.catalog_id == id).unwrap()
    }

    #[test]
    fn native_agents_adapters_and_missing_ones_are_told_apart() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let bin = root.path().join("bin");
        // On PATH: a native agent and a CLI whose adapter needs npx.
        stub(&bin, "gemini", "echo 'gemini 9.9.9'");
        stub(&bin, "claude", "echo '2.0.0 (Claude Code)'");
        // npx lives under the newest nvm version only, not on PATH.
        stub(
            &home.join(".nvm/versions/node/v18.2.0/bin"),
            "npx",
            "exit 0",
        );
        stub(
            &home.join(".nvm/versions/node/v20.11.1/bin"),
            "npx",
            "exit 0",
        );
        // An adapter binary in ~/.local/bin.
        stub(
            &home.join(".local/bin"),
            "codex-acp",
            "echo 'codex-acp 0.3.0'",
        );
        let list = detect_with(&machine(&home, &[&bin]), None);

        let gemini = find(&list, "gemini");
        assert_eq!(gemini.status, AcpAgentStatus::Installed);
        assert_eq!(
            gemini.command.as_deref(),
            Some(bin.join("gemini").to_str().unwrap())
        );
        assert_eq!(gemini.args, vec!["--acp".to_string()]);
        assert_eq!(gemini.version.as_deref(), Some("gemini 9.9.9"));

        let claude = find(&list, "claude");
        assert_eq!(claude.status, AcpAgentStatus::Adapter);
        let npx = home.join(".nvm/versions/node/v20.11.1/bin/npx");
        assert_eq!(claude.command.as_deref(), Some(npx.to_str().unwrap()));
        assert_eq!(claude.args[0], "-y");
        assert!(
            claude.args[1].contains("claude-agent-acp"),
            "{:?}",
            claude.args
        );
        assert_eq!(claude.version.as_deref(), Some("2.0.0 (Claude Code)"));

        let codex = find(&list, "codex");
        assert_eq!(codex.status, AcpAgentStatus::Installed);
        assert_eq!(
            codex.path.as_deref(),
            Some(home.join(".local/bin/codex-acp").to_str().unwrap())
        );

        let opencode = find(&list, "opencode");
        assert_eq!(opencode.status, AcpAgentStatus::Missing);
        assert!(opencode.command.is_none() && opencode.path.is_none());
        assert!(!opencode.install.is_empty());
        // Every entry is reported, found or not.
        assert_eq!(list.len(), catalog::CATALOG.len());
    }

    #[test]
    fn a_slow_version_is_skipped_and_a_non_executable_file_is_not_an_agent() {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        stub(&bin, "opencode", "sleep 5; echo never");
        std::fs::write(bin.join("goose"), "not executable").unwrap();
        let mut m = machine(root.path(), &[&bin]);
        m.version_timeout = Duration::from_millis(300);
        let started = Instant::now();
        let list = detect_with(&m, None);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
        let opencode = find(&list, "opencode");
        assert_eq!(opencode.status, AcpAgentStatus::Installed);
        assert_eq!(opencode.version, None);
        assert_eq!(find(&list, "goose").status, AcpAgentStatus::Missing);
    }

    #[test]
    fn the_login_shell_path_is_read_between_markers_and_a_hang_times_out() {
        let root = tempfile::tempdir().unwrap();
        let extra = root.path().join("from-profile");
        let shell = stub(
            &root.path().join("shells"),
            "chatty",
            &format!(
                "echo 'Welcome back!'; printf '{PATH_MARK}%s{PATH_MARK}' '{}:/usr/bin'; echo 'bye'",
                extra.display()
            ),
        );
        let mut m = machine(root.path(), &[]);
        m.shell = Some(shell);
        let path = login_shell_path(&m).unwrap();
        assert_eq!(path, format!("{}:/usr/bin", extra.display()));

        // A profile that never returns: given up at the limit, group killed.
        let hang = stub(&root.path().join("shells"), "hang", "sleep 30");
        m.shell = Some(hang);
        m.shell_timeout = Duration::from_millis(300);
        let started = Instant::now();
        assert_eq!(login_shell_path(&m), None);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );

        // A failing shell, or none: skipped silently.
        m.shell = Some(stub(&root.path().join("shells"), "broken", "exit 3"));
        assert_eq!(login_shell_path(&m), None);
        m.shell = None;
        assert_eq!(login_shell_path(&m), None);
    }

    #[test]
    fn the_login_shell_path_finds_agents_the_app_path_misses() {
        let root = tempfile::tempdir().unwrap();
        let hidden = root.path().join("only-in-shell");
        stub(&hidden, "goose", "echo 1.0");
        let m = machine(root.path(), &[]);
        let missing = detect_with(&m, None);
        assert_eq!(find(&missing, "goose").status, AcpAgentStatus::Missing);
        let login = hidden.to_string_lossy().to_string();
        let found = detect_with(&m, Some(&login));
        assert_eq!(find(&found, "goose").status, AcpAgentStatus::Installed);
    }

    #[test]
    fn npm_global_bin_is_searched_when_npm_is_found() {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        let prefix = root.path().join("npm-prefix");
        stub(&bin, "npm", &format!("echo '{}'", prefix.display()));
        stub(&prefix.join("bin"), "qwen", "echo 0.1");
        let list = detect_with(&machine(root.path(), &[&bin]), None);
        let qwen = find(&list, "qwen");
        assert_eq!(qwen.status, AcpAgentStatus::Installed);
        assert_eq!(
            qwen.path.as_deref(),
            Some(prefix.join("bin/qwen").to_str().unwrap())
        );
    }

    #[test]
    fn unconfirmed_agents_are_shown_but_never_launchable() {
        let entry = CatalogEntry {
            id: "maybe",
            confirmed: false,
            bins: &["maybe-agent"],
            note: "ACP support not confirmed.",
            ..catalog::CATALOG[0]
        };
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        stub(&bin, "maybe-agent", "echo 1");
        let found = resolve(&entry, std::slice::from_ref(&bin));
        assert_eq!(found.status, AcpAgentStatus::Unconfirmed);
        assert_eq!(found.found, Some(bin.join("maybe-agent")));
        assert!(found.command.is_none());
        let gone = resolve(&entry, &[root.path().join("empty")]);
        assert_eq!(gone.status, AcpAgentStatus::Missing);
    }

    #[test]
    fn search_dirs_keep_order_and_drop_duplicates() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("a");
        let m = machine(root.path(), &[&a, &a]);
        let dirs = search_dirs(&m, Some(&format!("{}:/usr/bin", a.display())));
        assert_eq!(dirs[0], a);
        assert_eq!(dirs.iter().filter(|d| **d == a).count(), 1);
        assert!(dirs.contains(&PathBuf::from("/usr/bin")));
        assert!(dirs.contains(&root.path().join(".local/bin")));
        assert!(dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
    }
}
