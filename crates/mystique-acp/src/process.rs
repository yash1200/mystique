//! The agent's process: spawned without a shell, in its own process group,
//! stdin/stdout for ACP, stderr to the debug log (masked).
//!
//! Every run owns one [`AgentProcess`]. Dropping it kills the whole process
//! group (`npx` starts node, which starts the Claude CLI). [`Children`]
//! remembers live groups so the app can kill them on quit, when async
//! cleanup no longer runs.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::task::JoinHandle;

use crate::scrub::Scrubber;

/// Lines of stderr kept for an error message.
const STDERR_TAIL: usize = 12;

/// What to run.
#[derive(Debug, Clone, Default)]
pub(crate) struct Launch {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
}

/// Live agent process groups, by pid. One per app.
#[derive(Clone, Default)]
pub struct Children {
    live: Arc<Mutex<HashMap<u32, ()>>>,
}

impl Children {
    fn add(&self, pid: u32) {
        if let Ok(mut live) = self.live.lock() {
            live.insert(pid, ());
        }
    }

    fn remove(&self, pid: u32) {
        if let Ok(mut live) = self.live.lock() {
            live.remove(&pid);
        }
    }

    /// Kill every live agent now (app quit). Returns how many there were.
    pub fn kill_all(&self) -> usize {
        let pids: Vec<u32> = self
            .live
            .lock()
            .map(|mut live| live.drain().map(|(pid, _)| pid).collect())
            .unwrap_or_default();
        for pid in &pids {
            kill_group(*pid);
        }
        pids.len()
    }

    pub fn count(&self) -> usize {
        self.live.lock().map(|l| l.len()).unwrap_or(0)
    }
}

#[cfg(unix)]
fn kill_group(pid: u32) {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return;
    };
    // SAFETY: killpg only sends a signal; the group id is the child's pid
    // (it was started with process_group(0)), never 0 or our own group.
    if pid > 1 {
        unsafe {
            libc::killpg(pid, libc::SIGKILL);
        }
    }
}

#[cfg(not(unix))]
fn kill_group(_pid: u32) {}

/// A running agent. Dropping it kills the process group.
pub(crate) struct AgentProcess {
    child: Child,
    pid: Option<u32>,
    children: Children,
    stderr: Arc<Mutex<VecDeque<String>>>,
    stderr_task: Option<JoinHandle<()>>,
}

/// Spawn the agent. Returns its stdout (ACP in) and stdin (ACP out).
pub(crate) fn spawn(
    launch: &Launch,
    scrub: Scrubber,
    children: Children,
) -> std::io::Result<(AgentProcess, ChildStdout, ChildStdin)> {
    let mut cmd = Command::new(&launch.program);
    cmd.args(&launch.args)
        .current_dir(&launch.cwd)
        .envs(launch.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    let mut child = cmd.spawn()?;
    let pid = child.id();
    if let Some(pid) = pid {
        children.add(pid);
    }
    let missing = || std::io::Error::other("the agent's stdio is not piped");
    let stdout = child.stdout.take().ok_or_else(missing)?;
    let stdin = child.stdin.take().ok_or_else(missing)?;
    let stderr_lines: Arc<Mutex<VecDeque<String>>> = Arc::default();
    let stderr_task = child.stderr.take().map(|stderr| {
        let tail = Arc::clone(&stderr_lines);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = scrub.text(&line);
                tracing::debug!(target: "mystique_acp::agent_stderr", "{line}");
                if let Ok(mut tail) = tail.lock() {
                    if tail.len() == STDERR_TAIL {
                        tail.pop_front();
                    }
                    tail.push_back(line);
                }
            }
        })
    });
    Ok((
        AgentProcess {
            child,
            pid,
            children,
            stderr: stderr_lines,
            stderr_task,
        },
        stdout,
        stdin,
    ))
}

impl AgentProcess {
    /// The last stderr lines, masked, for an error message.
    pub(crate) fn stderr_tail(&self) -> String {
        self.stderr
            .lock()
            .map(|t| t.iter().cloned().collect::<Vec<_>>().join("\n"))
            .unwrap_or_default()
    }

    /// Give the agent `grace` to exit on its own (stdin is closed by then),
    /// then kill the group. Waits until the process is gone.
    pub(crate) async fn stop(mut self, grace: Duration) {
        let _ = tokio::time::timeout(grace, self.child.wait()).await;
        // The group too: a helper the agent started may outlive it. The
        // leader was reaped at most `grace` ago, so its id is not reused.
        self.kill_now();
        let _ = tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await;
        if let Some(pid) = self.pid.take() {
            self.children.remove(pid);
        }
        if let Some(task) = self.stderr_task.take() {
            let _ = tokio::time::timeout(Duration::from_millis(200), task).await;
        }
    }

    fn kill_now(&mut self) {
        if let Some(pid) = self.pid {
            kill_group(pid);
        }
        let _ = self.child.start_kill();
    }
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        // Without `stop` the child was never reaped, so its id still names
        // our group.
        if let Some(pid) = self.pid.take() {
            kill_group(pid);
            self.children.remove(pid);
        }
        if let Some(task) = self.stderr_task.take() {
            task.abort();
        }
    }
}
