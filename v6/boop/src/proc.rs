//! Layer 0: process facts, from the OS only. This layer must not know tmux
//! exists.
#![allow(dead_code)]

use std::path::PathBuf;

use anyhow::Result;
use sysinfo::{Pid, ProcessesToUpdate, System};

/// A snapshot of one process.
#[derive(Clone, Debug)]
pub struct ProcessInfo {
    pub pid: u32,
    pub parent: Option<u32>,
    pub name: String,
    /// Resident set size in bytes.
    pub rss_bytes: u64,
    pub cpu_percent: f32,
    /// Process start time, seconds since the epoch.
    pub start_time_secs: u64,
    pub cwd: Option<PathBuf>,
}

/// The layer-0 process reader. A lane is alive at layer 1 (its tmux session
/// exists) OR at layer 0 (its pid is alive); those are different questions.
pub trait ProcReader {
    /// Whether a pid is alive right now.
    fn is_alive(&self, pid: u32) -> bool;

    /// One process snapshot, `None` if the pid is gone.
    fn process(&self, pid: u32) -> Option<ProcessInfo>;

    /// Direct children of `pid`, by pid.
    fn children(&self, pid: u32) -> Vec<u32>;

    /// Every descendant of `pid`, all depths, in discovery order.
    fn descendants(&self, pid: u32) -> Vec<u32>;

    /// Total descendants of `pid` (all depths).
    fn descendent_count(&self, pid: u32) -> usize;
}

/// A `Sysinfo`-backed snapshot. Refresh once, query many times.
pub struct SysinfoSnapshot {
    system: System,
}

impl SysinfoSnapshot {
    pub fn capture() -> Result<Self> {
        let mut system = System::new_all();
        system.refresh_processes(ProcessesToUpdate::All, true);
        Ok(SysinfoSnapshot { system })
    }
}

const MAX_DESCENDANTS: usize = 1_000_000;

impl ProcReader for SysinfoSnapshot {
    fn is_alive(&self, pid: u32) -> bool {
        self.system.process(Pid::from_u32(pid)).is_some()
    }

    fn process(&self, pid: u32) -> Option<ProcessInfo> {
        let process = self.system.process(Pid::from_u32(pid))?;
        Some(ProcessInfo {
            pid,
            parent: process.parent().map(|parent| parent.as_u32()),
            name: process.name().to_string_lossy().into_owned(),
            rss_bytes: process.memory(),
            cpu_percent: process.cpu_usage(),
            start_time_secs: process.start_time(),
            cwd: process.cwd().map(PathBuf::from),
        })
    }

    fn children(&self, pid: u32) -> Vec<u32> {
        self.system
            .processes()
            .iter()
            .filter(|(_, process)| {
                process
                    .parent()
                    .is_some_and(|parent| parent.as_u32() == pid)
            })
            .map(|(child_pid, _)| child_pid.as_u32())
            .collect()
    }

    fn descendants(&self, pid: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut stack: Vec<u32> = self.children(pid);
        let mut visited = std::collections::HashSet::new();
        while let Some(current) = stack.pop() {
            if !visited.insert(current) || out.len() >= MAX_DESCENDANTS {
                continue;
            }
            out.push(current);
            stack.extend(self.children(current));
        }
        out
    }

    fn descendent_count(&self, pid: u32) -> usize {
        self.descendants(pid).len()
    }
}
