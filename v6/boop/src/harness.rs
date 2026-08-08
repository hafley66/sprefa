//! The trait every harness adapter implements; the CLI never names a harness.

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
