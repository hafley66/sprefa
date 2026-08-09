//! The normalized event shape shared by every harness adapter.
//!
//! The field set is vendored by hand from `agent-session`'s `types.rs`; that
//! crate is not a dependency here. Its transport (full-file re-reads) is the
//! thing this lane replaces; its types are the part worth keeping.

use serde::Serialize;

/// One normalized event decoded from a harness transcript line.
#[derive(Clone, Debug, Serialize)]
pub struct AgentEvent {
    pub harness: &'static str,
    pub session_id: String,
    pub ts_ms: u64,
    pub uuid: Option<String>,
    /// Present on most records: the conversation is already a DAG on disk.
    pub parent_uuid: Option<String>,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub record_type: String,
    pub tool_name: Option<String>,
    pub paths: Vec<ToolPath>,
    pub urls: Vec<String>,
    /// Byte offset of the source line in the transcript file.
    pub raw_line_offset: u64,
}

/// A file a tool touched, with the direction of access.
#[derive(Clone, Debug, Serialize)]
pub struct ToolPath {
    pub path: String,
    pub access: Access,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[allow(dead_code)]
pub enum Access {
    Read,
    Write,
    Create,
    Delete,
    Rename,
}
