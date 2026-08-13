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

/// Descendant-tree sums for one root pid: resident set bytes, cpu percent, and
/// the root's start time, the anchoring instant for the tree's uptime.
#[derive(Clone, Debug)]
pub struct TreeSum {
    pub rss_bytes: u64,
    pub cpu_percent: f32,
    pub start_time_secs: u64,
}

/// The root process plus every descendant, summed over the tree. `None` when
/// the root pid is gone. RSS and CPU are tree-wide; start time is the root's.
pub fn tree_sum_of(reader: &dyn ProcReader, pid: u32) -> Option<TreeSum> {
    let mut acc = reader.process(pid)?;
    for child in reader.descendants(pid) {
        if let Some(process) = reader.process(child) {
            acc.rss_bytes += process.rss_bytes;
            acc.cpu_percent += process.cpu_percent;
        }
    }
    Some(TreeSum {
        rss_bytes: acc.rss_bytes,
        cpu_percent: acc.cpu_percent,
        start_time_secs: acc.start_time_secs,
    })
}

/// The real uptime: now minus a start time, never the epoch instant itself.
pub fn uptime_secs(start_time_secs: u64, now: u64) -> u64 {
    now.saturating_sub(start_time_secs)
}

/// The wall clock in whole seconds since the epoch.
pub fn sys_now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
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

    /// Descendant-tree sums for one pid, `None` if the pid is gone.
    pub fn tree_sum(&self, pid: u32) -> Option<TreeSum> {
        tree_sum_of(self, pid)
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::{tree_sum_of, uptime_secs, ProcReader, ProcessInfo};

    /// A tiny scripted process tree: root 1 whose child 2 is itself a parent
    /// of 3, so the sum must reach all three depths.
    struct FakeTree(HashMap<u32, (u32, u64, f32, u64)>);

    impl FakeTree {
        fn new() -> Self {
            let mut map = HashMap::new();
            map.insert(1, (0, 100, 10.0, 500));
            map.insert(2, (1, 200, 20.0, 600));
            map.insert(3, (2, 400, 30.0, 700));
            FakeTree(map)
        }
    }

    impl ProcReader for FakeTree {
        fn is_alive(&self, pid: u32) -> bool {
            self.0.contains_key(&pid)
        }
        fn process(&self, pid: u32) -> Option<ProcessInfo> {
            let (parent, rss, cpu, start) = *self.0.get(&pid)?;
            Some(ProcessInfo {
                pid,
                parent: (parent != 0).then_some(parent),
                name: "fake".into(),
                rss_bytes: rss,
                cpu_percent: cpu,
                start_time_secs: start,
                cwd: Some(PathBuf::from("/w")),
            })
        }
        fn children(&self, pid: u32) -> Vec<u32> {
            let mut out: Vec<u32> = self
                .0
                .iter()
                .filter(|(_, (parent, _, _, _))| *parent == pid)
                .map(|(child, _)| *child)
                .collect();
            out.sort();
            out
        }
        fn descendants(&self, pid: u32) -> Vec<u32> {
            let mut out = Vec::new();
            let mut stack = self.children(pid);
            while let Some(current) = stack.pop() {
                out.push(current);
                stack.extend(self.children(current));
            }
            out
        }
        fn descendent_count(&self, pid: u32) -> usize {
            self.descendants(pid).len()
        }
    }

    /// RECEIPT (Job 2). The tree sum over a root with children is larger than
    /// the pane-only (root-only) figure, and reaches every depth.
    #[test]
    fn tree_sum_exceeds_pane_only_on_a_fixture_tree() {
        let tree = FakeTree::new();
        let sum = tree_sum_of(&tree, 1).unwrap();
        assert_eq!(sum.rss_bytes, 100 + 200 + 400, "all three depths summed");
        assert_eq!(tree.descendent_count(1), 2);
        let root_only = tree.process(1).unwrap();
        assert!(sum.rss_bytes > root_only.rss_bytes, "tree > pane-only");
        assert!(sum.cpu_percent > root_only.cpu_percent);
    }

    /// RECEIPT (Job 2). Uptime is now minus a start time: a small duration,
    /// never the epoch start instant itself.
    #[test]
    fn uptime_is_a_duration_not_an_epoch_stamp() {
        let now = 1_800_000_000u64;
        let start = 1_799_999_500u64;
        let uptime = uptime_secs(start, now);
        assert_eq!(uptime, 500, "a small real duration");
        assert!(uptime < 1_000, "never epoch-sized: {uptime}");
    }
}
