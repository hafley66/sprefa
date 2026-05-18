//! No-read source change signal (the memo staleness oracle's input).
//!
//! The prior oracle read every dependency file in full and blake3-hashed
//! it to decide staleness — 1.34 GB of `std::fs::read` per run on the
//! linux fixture, just to learn nothing moved. This replaces that with a
//! signal that NEVER opens an unchanged file:
//!
//! - git-tracked & clean → the blob OID git already computed
//!   (`git ls-files -s`, ~40 ms for the whole tree, zero worktree reads).
//!   The OID *is* a content hash, so it is trusted outright.
//! - git-dirty / untracked / non-git → a `stat()` tuple
//!   (mtime + size). One `stat`, no read. Standard make/ninja-class
//!   heuristic; combined size+mtime, conservative direction (a touched
//!   file re-runs, it does not falsely replay anything but a
//!   mtime-AND-size preserved rewrite — the same risk every build tool
//!   accepts).
//! - `stat` itself fails → read + blake3 (the old behavior, now the
//!   rare last resort).
//!
//! The `SourceIndex` is built ONCE per run (two `git` invocations) and
//! shared by both sides of the memo: the dispatch-side dep recorder and
//! the probe-side staleness check. The identity rides the EXISTING
//! `_memo_deps.content_id` text column with a 2-char tag, so there is no
//! schema change and pre-tag rows (bare 64-hex blake3) still decode.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A no-read change signal for one source file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceIdentity {
    /// git blob OID hex. Authoritative content hash (git already
    /// computed it); trusted outright on the git path.
    Git(String),
    /// `stat()` tuple for git-dirty / untracked / non-git files. No
    /// read. `mtime_ns` is nanoseconds since the unix epoch.
    Stat { mtime_ns: i128, size: u64 },
    /// blake3 hex of the bytes. Only minted when `stat` fails; also the
    /// decode target for legacy untagged rows.
    Bytes(String),
}

impl SourceIdentity {
    /// Tagged text form for the `_memo_deps.content_id` column.
    pub fn encode(&self) -> String {
        match self {
            SourceIdentity::Git(o) => format!("g:{o}"),
            SourceIdentity::Stat { mtime_ns, size } => format!("s:{mtime_ns}:{size}"),
            SourceIdentity::Bytes(h) => format!("b:{h}"),
        }
    }

    /// Inverse of `encode`. An untagged string is a pre-this-change
    /// blake3 row → `Bytes` (it will compare unequal to any current
    /// git/stat identity, i.e. conservatively STALE, forcing one honest
    /// recompute that then records the new tagged identity).
    pub fn decode(s: &str) -> SourceIdentity {
        if let Some(o) = s.strip_prefix("g:") {
            return SourceIdentity::Git(o.to_string());
        }
        if let Some(rest) = s.strip_prefix("s:") {
            if let Some((m, sz)) = rest.split_once(':') {
                if let (Ok(m), Ok(sz)) = (m.parse::<i128>(), sz.parse::<u64>()) {
                    return SourceIdentity::Stat {
                        mtime_ns: m,
                        size: sz,
                    };
                }
            }
        }
        if let Some(h) = s.strip_prefix("b:") {
            return SourceIdentity::Bytes(h.to_string());
        }
        SourceIdentity::Bytes(s.to_string())
    }
}

/// `stat()`-derived identity. One metadata call, no file read.
fn stat_identity(abs: &Path) -> Option<SourceIdentity> {
    let m = std::fs::metadata(abs).ok()?;
    let ns = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos() as i128;
    Some(SourceIdentity::Stat {
        mtime_ns: ns,
        size: m.len(),
    })
}

/// Per-run snapshot of git's view of the tree. Built once; queried per
/// memo dependency with zero file reads on the common (unchanged) path.
#[derive(Clone)]
pub struct SourceIndex {
    /// `git rev-parse --show-toplevel`, or `None` when the hint is not
    /// inside a work tree (non-git target → everything goes to `stat`).
    git_root: Option<PathBuf>,
    /// rel-path → blob OID hex, from `git ls-files -s`.
    oid: HashMap<String, String>,
    /// rel-paths git considers modified/staged (`git status`). These do
    /// NOT trust the index OID; they fall through to `stat`.
    dirty: HashSet<String>,
    /// `git rev-parse HEAD` of the toplevel. The run-to-run repo-state
    /// token: a tracked-clean file's content cannot move while HEAD is
    /// fixed (clean ⇒ matches the HEAD tree), so one OID compare stands
    /// in for 63k per-file identity rows. `None` off-git.
    head_oid: Option<String>,
}

impl SourceIndex {
    /// Build from any path inside the target tree (the scan root, or a
    /// recorded absolute dependency path — both resolve to the same git
    /// toplevel, so the `OnceLock` that caches this is order-independent
    /// between the probe side and the dispatch side).
    pub fn build(hint: &Path) -> Self {
        let start = if hint.is_dir() {
            hint.to_path_buf()
        } else {
            hint.parent().unwrap_or(Path::new(".")).to_path_buf()
        };
        let git_root = Command::new("git")
            .arg("-C")
            .arg(&start)
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                PathBuf::from(String::from_utf8_lossy(&o.stdout).trim().to_string())
            });

        let mut oid = HashMap::new();
        let mut dirty = HashSet::new();
        let mut head_oid = None;

        if let Some(gr) = &git_root {
            head_oid = Command::new("git")
                .arg("-C")
                .arg(gr)
                .args(["rev-parse", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty());
            // OIDs for every tracked file. NUL-delimited, no worktree
            // read. Entry: "<mode> <oid> <stage>\t<relpath>".
            if let Ok(o) = Command::new("git")
                .arg("-C")
                .arg(gr)
                .args(["ls-files", "-s", "-z"])
                .output()
            {
                for ent in o.stdout.split(|b| *b == 0).filter(|s| !s.is_empty()) {
                    let s = String::from_utf8_lossy(ent);
                    if let Some((meta, path)) = s.split_once('\t') {
                        if let Some(oidhex) = meta.split_whitespace().nth(1) {
                            oid.insert(path.to_string(), oidhex.to_string());
                        }
                    }
                }
            }
            // Worktree-modified / staged set. Entry: "XY <path>".
            // `-uno` drops untracked (they are absent from `oid` anyway
            // and fall to `stat`); `--no-renames` keeps one path/entry.
            if let Ok(o) = Command::new("git")
                .arg("-C")
                .arg(gr)
                .args(["status", "--porcelain", "-z", "-uno", "--no-renames"])
                .output()
            {
                for ent in o.stdout.split(|b| *b == 0).filter(|s| !s.is_empty()) {
                    let s = String::from_utf8_lossy(ent);
                    if s.len() > 3 {
                        dirty.insert(s[3..].to_string());
                    }
                }
            }
        }

        Self {
            git_root,
            oid,
            dirty,
            head_oid,
        }
    }

    /// The repo-state token component: `git rev-parse HEAD`. `None`
    /// off-git (then the token degrades to the dirty set only).
    pub fn head_oid(&self) -> Option<&str> {
        self.head_oid.as_deref()
    }

    /// Absolute paths of tracked files whose content differs between
    /// `prev_head` and the current HEAD (`git diff --name-only A B`).
    /// Empty when `prev_head` is empty or equals the current HEAD (the
    /// fixed-HEAD common case: no tracked-clean file can have moved).
    pub fn diff_names_since(&self, prev_head: &str) -> Vec<PathBuf> {
        let (Some(gr), Some(cur)) = (&self.git_root, &self.head_oid) else {
            return Vec::new();
        };
        if prev_head.is_empty() || prev_head == cur {
            return Vec::new();
        }
        Command::new("git")
            .arg("-C")
            .arg(gr)
            .args(["diff", "--name-only", "-z", prev_head, cur.as_str()])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                o.stdout
                    .split(|b| *b == 0)
                    .filter(|s| !s.is_empty())
                    .map(|s| gr.join(String::from_utf8_lossy(s).as_ref()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// True when this index has a git toplevel (the OID fast path is
    /// live). Telemetry/tests only.
    pub fn is_git(&self) -> bool {
        self.git_root.is_some()
    }

    /// Absolute paths git reports as worktree-modified/staged
    /// (`git status --porcelain -uno`). Used by the warm dirty-slice to
    /// enumerate changed tracked files WITHOUT a tree walk. Untracked
    /// files are deliberately absent (`-uno`); brand-new files on a
    /// warm run are a flagged limitation, not covered here.
    pub fn dirty_abs(&self) -> Vec<PathBuf> {
        match &self.git_root {
            Some(gr) => self.dirty.iter().map(|rel| gr.join(rel)).collect(),
            None => Vec::new(),
        }
    }

    /// No-read identity for an absolute path.
    ///   tracked & clean        → `Git(oid)`   (NO read, NO stat)
    ///   dirty/untracked/non-git→ `Stat{..}`   (one `stat`, NO read)
    ///   `stat` fails           → `Bytes(..)`  (read + blake3, rare)
    pub fn identity(&self, abs: &Path) -> SourceIdentity {
        if let Some(gr) = &self.git_root {
            if let Ok(rel) = abs.strip_prefix(gr) {
                let rel = rel.to_string_lossy();
                if !self.dirty.contains(rel.as_ref()) {
                    if let Some(o) = self.oid.get(rel.as_ref()) {
                        return SourceIdentity::Git(o.clone());
                    }
                }
            }
        }
        stat_identity(abs).unwrap_or_else(|| {
            match std::fs::read(abs) {
                Ok(b) => SourceIdentity::Bytes(blake3::hash(&b).to_hex().to_string()),
                Err(_) => SourceIdentity::Bytes(String::new()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_round_trips_and_legacy_decodes_as_bytes() {
        for id in [
            SourceIdentity::Git("abc123".into()),
            SourceIdentity::Stat {
                mtime_ns: 1_700_000_000_000_000_000,
                size: 42,
            },
            SourceIdentity::Bytes("deadbeef".into()),
        ] {
            assert_eq!(SourceIdentity::decode(&id.encode()), id);
        }
        // A pre-tag 64-hex blake3 row decodes as Bytes (→ conservatively
        // stale vs any git/stat identity, forcing one clean recompute).
        let legacy = "f".repeat(64);
        assert_eq!(
            SourceIdentity::decode(&legacy),
            SourceIdentity::Bytes(legacy)
        );
    }

    #[test]
    fn non_git_dir_uses_stat_and_detects_edits_without_reading() {
        let dir = std::env::temp_dir().join(format!("sprf_si_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("x.rs");
        std::fs::write(&f, b"a").unwrap();

        let idx = SourceIndex::build(&dir);
        let i0 = idx.identity(&f);
        assert!(matches!(i0, SourceIdentity::Stat { .. }));

        // Re-stat unchanged → identical identity (would REPLAY).
        assert_eq!(idx.identity(&f), i0);

        // Edit changes size (and mtime) → identity moves (would be
        // STALE) with no file read.
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&f, b"aa").unwrap();
        assert_ne!(idx.identity(&f), i0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
