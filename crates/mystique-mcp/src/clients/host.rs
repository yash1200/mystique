//! Filesystem view used to detect AI tools and write their MCP configs.

use std::env;
use std::path::{Path, PathBuf};

/// Isolated view of the machine: home, PATH, app bundles, and user-config root.
#[derive(Debug, Clone)]
pub struct Host {
    pub home: PathBuf,
    pub path_dirs: Vec<PathBuf>,
    pub app_dirs: Vec<PathBuf>,
    /// macOS `Application Support`, Windows `%APPDATA%`, Linux `~/.config`.
    pub user_config: PathBuf,
}

impl Host {
    pub fn current() -> Self {
        let home = home_dir();
        let mut path_dirs: Vec<PathBuf> = env::var_os("PATH")
            .map(|p| env::split_paths(&p).collect())
            .unwrap_or_default();

        // Finder-launched Tauri apps see a stripped PATH. Check the usual bins too.
        for extra in extra_bin_dirs(&home) {
            if extra.is_dir() && !path_dirs.iter().any(|d| d == &extra) {
                path_dirs.push(extra);
            }
        }

        let mut app_dirs = Vec::new();
        #[cfg(target_os = "macos")]
        {
            app_dirs.push(PathBuf::from("/Applications"));
            app_dirs.push(home.join("Applications"));
        }

        Self {
            user_config: user_config_dir(&home),
            home,
            path_dirs,
            app_dirs,
        }
    }

    #[cfg(test)]
    pub fn for_test(root: &Path) -> Self {
        Self {
            home: root.join("home"),
            path_dirs: vec![root.join("bin")],
            app_dirs: vec![root.join("Applications")],
            user_config: root.join("AppSupport"),
        }
    }

    pub fn bin_on_path(&self, name: &str) -> bool {
        self.path_dirs.iter().any(|dir| executable_in(dir, name))
    }

    pub fn app_installed(&self, bundle: &str) -> bool {
        self.app_dirs.iter().any(|dir| dir.join(bundle).exists())
    }

    pub fn home_path(&self, rel: &str) -> PathBuf {
        join_rel(&self.home, rel)
    }

    pub fn user_config_path(&self, rel: &str) -> PathBuf {
        join_rel(&self.user_config, rel)
    }

    pub fn display_path(&self, path: &Path) -> String {
        match path.strip_prefix(&self.home) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        }
    }
}

fn join_rel(base: &Path, rel: &str) -> PathBuf {
    rel.split('/')
        .filter(|s| !s.is_empty())
        .fold(base.to_path_buf(), |p, s| p.join(s))
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn user_config_dir(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home.join("Library").join("Application Support")
    }
    #[cfg(target_os = "windows")]
    {
        env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Roaming"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        home.join(".config")
    }
}

fn extra_bin_dirs(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".local").join("bin"),
        home.join(".grok").join("bin"),
        home.join(".cargo").join("bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]
}

fn executable_in(dir: &Path, name: &str) -> bool {
    if dir.join(name).is_file() {
        return true;
    }
    #[cfg(windows)]
    {
        for ext in ["exe", "cmd", "bat"] {
            if dir.join(format!("{name}.{ext}")).is_file() {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn display_path_uses_tilde_for_home() {
        let host = Host {
            home: PathBuf::from("/Users/ada"),
            path_dirs: vec![],
            app_dirs: vec![],
            user_config: PathBuf::from("/Users/ada/Library/Application Support"),
        };
        assert_eq!(
            host.display_path(Path::new("/Users/ada/.claude.json")),
            "~/.claude.json"
        );
    }

    #[test]
    fn bin_on_path_finds_file_in_path_dir() {
        let root = tempfile::tempdir().unwrap();
        let host = Host::for_test(root.path());
        fs::create_dir_all(&host.path_dirs[0]).unwrap();
        fs::write(host.path_dirs[0].join("claude"), "").unwrap();
        assert!(host.bin_on_path("claude"));
        assert!(!host.bin_on_path("codex"));
    }
}
