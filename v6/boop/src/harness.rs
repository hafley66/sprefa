//! The trait every harness adapter implements; the CLI never names a harness.
#![allow(dead_code)]

use std::path::PathBuf;

use crate::event::AgentEvent;

pub mod claude;

/// One agent harness that writes transcripts to this machine.
pub trait Harness {
    /// Stable short id used in CLI output and as the `--harness` filter value.
    fn id(&self) -> &'static str;

    /// Every session this harness has on disk, newest last. No cap.
    fn sessions(&self) -> anyhow::Result<Vec<SessionRef>>;

    /// Read forward from `offset` bytes. Returns the events decoded and the
    /// new offset to resume from. A partial trailing line is NOT consumed and
    /// NOT counted in the returned offset.
    fn read_from(&self, session: &SessionRef, offset: u64) -> anyhow::Result<ReadChunk>;

    // facet 3: control. Defaults are the honest all-false / Unsupported shape,
    // so any adapter without control support is safe and explicit.

    /// What this harness can control. `true` only where a test confirms it.
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    /// Spawn a session per `spec`, returning a handle to it.
    fn spawn(&self, _spec: &SpawnSpec) -> anyhow::Result<SessionRef> {
        anyhow::bail!("harness `{}` has no spawn support", self.id())
    }

    /// Send `text` to a live session.
    fn send(&self, _session: &SessionRef, _text: &str) -> anyhow::Result<SendOutcome> {
        Ok(SendOutcome::Unsupported)
    }

    /// Stop a live session.
    fn stop(&self, _session: &SessionRef) -> anyhow::Result<()> {
        anyhow::bail!("harness `{}` has no stop support", self.id())
    }
}

/// One transcript on disk that belongs to a harness.
#[derive(Clone, Debug)]
pub struct SessionRef {
    pub harness: &'static str,
    pub session_id: String,
    pub path: PathBuf,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    /// Last modified time in milliseconds since the epoch.
    pub modified_ms: u64,
    /// Size of the file in bytes.
    pub size: u64,
    /// The tmux session that runs this harness (a transport handle the
    /// control facet targets); `None` when there is no live pane.
    pub tmux: Option<String>,
    /// The tmux socket the session lives on (throwaway sockets in tests).
    pub tmux_socket: Option<String>,
    /// The session id that spawned this one, when the harness records it.
    pub parent: Option<String>,
}

/// The decoded events from one forward read, plus where to resume.
#[derive(Clone, Debug)]
pub struct ReadChunk {
    pub events: Vec<AgentEvent>,
    pub next_offset: u64,
    /// True when the file was shorter than the requested offset (truncated or
    /// rotated); the read restarted from byte 0.
    pub reset: bool,
    /// Lines skipped because they failed to parse as JSON.
    pub skipped: usize,
}

/// What a harness can do, for the control facet. A capability is `true` only
/// when a test exercises it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Capabilities {
    pub send_midflight: bool,
    pub resume: bool,
    pub spawn: bool,
    pub subagent_visible: bool,
}

/// The result of sending text to a live session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    Injected,
    QueuedForNextSpawn,
    Unsupported,
}

/// What a spawn should create.
#[derive(Clone, Debug)]
pub struct SpawnSpec {
    pub harness: &'static str,
    pub branch: String,
    pub base_sha: String,
    pub main_tree: bool,
    /// Worktree gap steps (install, build) run in order before the prompt.
    pub setup: Vec<String>,
    pub prompt: String,
    /// Resume an existing transcript under this session id.
    pub resume_session: Option<String>,
    /// The tmux socket to spawn on (`None` is the default server).
    pub socket: Option<String>,
    /// The directory to run the harness in (the worktree, once created).
    pub worktree_dir: Option<std::path::PathBuf>,
    /// The git checkout a worktree branches from (or the main-tree working
    /// dir when `main_tree` is true).
    pub repo: std::path::PathBuf,
}
