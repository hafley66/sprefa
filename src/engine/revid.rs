//! Rev identity: the resolved form of a `scan` rule's rev coordinate.
//!
//! `scan("WORK", …)` stays the surface syntax users write. `WORK` is an ALIAS
//! that resolves, at the `Engine::resolve_rev` seam, to the repo's actual
//! revision identity: the HEAD oid, with a `+` suffix when the working tree
//! differs from it. Before this type existed the alias itself was stored, so
//! `_file.rev`, every `_rev` twin, and every extract digest key carried the
//! text `WORK` instead of a revision.
//!
//! INV-1: no rev column in any table ever holds an alias. Every stored rev
//! matches `^[0-9a-f]{40}\+?$`.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The rev coordinate users write for "the working tree". Surface syntax, and
/// the one spelling of it in the engine: compare PROGRAM TEXT against this,
/// never stored data.
pub(crate) const WORK_ALIAS: &str = "WORK";

/// git's own "no object" oid. A directory with no HEAD (a non-git corpus, or a
/// repo with no commits) resolves to this: 40 hex chars, so it interns, sorts,
/// and parses like any other rev, and it can never collide with a real commit.
pub(crate) const NO_HEAD_OID: &str = "0000000000000000000000000000000000000000";

/// The resolved identity of a repo's revision at one point in a tick.
///
/// Lifetime: VALUE type, no state. Built per resolution, copied into
/// `ScanBinding`, dropped with it.
///
/// `from_alias` is carried in memory and deliberately NOT stored. It records
/// HOW the rev was read (the working tree's files, versus a git object read),
/// while the stored text records WHAT the rows are. A user may write
/// `scan("HEAD", …)` meaning the committed tree, which on a clean tree resolves
/// to the same oid while still wanting the git read path, so the two must stay
/// distinguishable in memory.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RevId {
    oid: String,
    dirty: bool,
    from_alias: bool,
}

impl RevId {
    /// A committed rev, resolved from an explicit rev literal (a sha, tag,
    /// branch, or `HEAD`). Read through git, never the filesystem.
    pub(crate) fn commit(oid: impl Into<String>) -> Self {
        Self {
            oid: oid.into(),
            dirty: false,
            from_alias: false,
        }
    }

    /// The working tree of a repo whose HEAD is `oid`.
    pub(crate) fn worktree(oid: impl Into<String>, dirty: bool) -> Self {
        Self {
            oid: oid.into(),
            dirty,
            from_alias: true,
        }
    }

    /// The working tree of a directory with no HEAD.
    pub(crate) fn no_head() -> Self {
        Self::worktree(NO_HEAD_OID, true)
    }

    /// Stored form: `"<oid>"` or `"<oid>+"`.
    pub(crate) fn text(&self) -> String {
        if self.dirty {
            format!("{}+", self.oid)
        } else {
            self.oid.clone()
        }
    }

    /// THE worktree predicate: read this rev's content from the filesystem
    /// rather than from git.
    pub(crate) fn is_worktree(&self) -> bool {
        self.from_alias
    }

    pub(crate) fn dirty(&self) -> bool {
        self.dirty
    }

    /// Trace form: `WORK@<oid8>+` for a resolved alias, `<oid8>` for a commit.
    /// Keeps the working-tree signal in the `[scan slug@rev]` lines an operator
    /// reads, which a bare oid prefix would drop.
    pub(crate) fn short(&self) -> String {
        let head = &self.oid[..self.oid.len().min(8)];
        match (self.from_alias, self.dirty) {
            (true, true) => format!("{WORK_ALIAS}@{head}+"),
            (true, false) => format!("{WORK_ALIAS}@{head}"),
            (false, _) => head.to_string(),
        }
    }

    /// Recover a `RevId` from its stored text. The one place string surgery on
    /// a rev is permitted, because the storage form is a display encoding read
    /// back at exactly one seam, never a key component.
    ///
    /// `from_alias` is false: it is not stored, so a parsed rev claims nothing
    /// about how it was read.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let (oid, dirty) = match text.strip_suffix('+') {
            Some(base) => (base, true),
            None => (text, false),
        };
        if oid.len() == 40 && oid.chars().all(|ch| ch.is_ascii_hexdigit()) {
            Some(Self {
                oid: oid.to_string(),
                dirty,
                from_alias: false,
            })
        } else {
            None
        }
    }
}

/// A rev text a git subprocess can resolve: an object name carrying no `+`.
///
/// STRUCTURAL enforcement of the same principle section 6 states for cache
/// identity. `<sha>+` is a display encoding, not a git object name, so
/// `git cat-file <sha>+:<path>` can only ever fail. Every git-shelling helper
/// that takes a rev takes THIS, so handing one the stored display text does not
/// typecheck, and the failure mode is a compile error instead of an
/// intermittent runtime error on whichever file happened to miss its cache.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GitOid<'a>(&'a str);

impl<'a> GitOid<'a> {
    /// A STORED rev text git can resolve: a bare 40-char hex oid.
    ///
    /// `None` for the dirty form (`<sha>+`), whose bytes live only in the
    /// working tree, and `None` for anything that is not an oid at all, so an
    /// unexpected value degrades to a filesystem read rather than a git
    /// failure. Not for ref NAMES; those never reach a stored rev column.
    pub(crate) fn of(rev: &'a str) -> Option<Self> {
        let is_oid = rev.len() == 40 && rev.chars().all(|ch| ch.is_ascii_hexdigit());
        is_oid.then_some(Self(rev))
    }

    pub(crate) fn as_str(&self) -> &'a str {
        self.0
    }
}

impl RevId {
    /// This rev's git-resolvable object name. Always the bare oid, never the
    /// `+` form, so it is a `GitOid` by construction.
    pub(crate) fn git_oid(&self) -> GitOid<'_> {
        GitOid(&self.oid)
    }
}

/// Per-Engine cache of the working-tree resolution, one entry per repo root.
///
/// Lifetime: ONE per `Engine`, lives as long as the `Engine`. Cleared at the
/// top of each tick (alongside `rev_cache`) and by `invalidate` on a git event,
/// so a tick spanning a `git commit` uses ONE oid throughout: re-resolving
/// mid-tick would write `_file` rows straddling two revs.
///
/// Deliberately separate from `Engine::rev_sha_cache`. A CLEAN alias resolution
/// yields a bare 40-hex oid, which passes `Engine::is_immutable_rev`, so routing
/// it through `cache_rev` would file it in the CROSS-TICK cache and pin a stale
/// HEAD until process restart.
#[derive(Default)]
pub(crate) struct WorktreeRev {
    cached: HashMap<PathBuf, RevId>,
}

impl WorktreeRev {
    pub(crate) fn resolve(&mut self, repo_root: &Path) -> Result<RevId> {
        if let Some(hit) = self.cached.get(repo_root) {
            return Ok(hit.clone());
        }
        let resolved = Self::probe(repo_root)?;
        self.cached
            .insert(repo_root.to_path_buf(), resolved.clone());
        Ok(resolved)
    }

    pub(crate) fn invalidate(&mut self) {
        self.cached.clear();
    }

    fn probe(repo_root: &Path) -> Result<RevId> {
        // Determinism seam, mirroring `DL_EXE_STAMP` (extract/mod.rs): a test
        // suite pins the working-tree rev so its assertions do not depend on
        // whether the developer's tree is clean. Scoped to the ALIAS path only,
        // so an explicit `scan("HEAD", …)` in the same process still resolves
        // through git and stays distinguishable from `scan("WORK", …)`.
        if let Ok(forced) = std::env::var("DL_REV_OVERRIDE") {
            let parsed = RevId::parse(forced.trim()).ok_or_else(|| {
                anyhow::anyhow!(
                    "DL_REV_OVERRIDE={forced:?} is not a 40-char hex oid with an optional '+'"
                )
            })?;
            return Ok(RevId::worktree(parsed.oid, parsed.dirty));
        }
        let Some(oid) = head_oid(repo_root)? else {
            return Ok(RevId::no_head());
        };
        Ok(RevId::worktree(oid, probe_dirty(repo_root)?))
    }
}

fn head_oid(repo_root: &Path) -> Result<Option<String>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-parse", "HEAD"])
        .output();
    let Ok(out) = out else { return Ok(None) };
    if !out.status.success() {
        return Ok(None);
    }
    let oid = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if oid.len() == 40 && oid.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Ok(Some(oid))
    } else {
        Ok(None)
    }
}

/// Two probes, cheapest first. `diff-index --quiet` is ~8x cheaper than
/// `status --porcelain` because it skips untracked enumeration, and for the
/// same reason it cannot see untracked files, so a clean result still needs
/// `ls-files --others`.
fn probe_dirty(repo_root: &Path) -> Result<bool> {
    let tracked = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["diff-index", "--quiet", "HEAD", "--"])
        .output()?;
    if !tracked.status.success() {
        return Ok(true);
    }
    let untracked = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["ls-files", "--others", "--exclude-standard"])
        .output()?;
    Ok(!untracked
        .stdout
        .iter()
        .all(|byte| byte.is_ascii_whitespace()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OID: &str = "1234567890abcdef1234567890abcdef12345678";

    #[test]
    fn clean_rev_round_trips_without_a_marker() {
        let rev = RevId::worktree(OID, false);
        assert_eq!(rev.text(), OID);
        let back = RevId::parse(&rev.text()).unwrap();
        assert_eq!(back.git_oid().as_str(), OID);
        assert!(!back.dirty());
    }

    #[test]
    fn dirty_rev_round_trips_with_a_marker() {
        let rev = RevId::worktree(OID, true);
        assert_eq!(rev.text(), format!("{OID}+"));
        let back = RevId::parse(&rev.text()).unwrap();
        assert_eq!(back.git_oid().as_str(), OID);
        assert!(back.dirty());
    }

    #[test]
    fn zero_sentinel_round_trips() {
        let rev = RevId::no_head();
        assert_eq!(rev.text(), format!("{NO_HEAD_OID}+"));
        assert_eq!(
            RevId::parse(&rev.text()).unwrap().git_oid().as_str(),
            NO_HEAD_OID
        );
        assert!(rev.is_worktree());
    }

    #[test]
    fn parse_rejects_the_alias_and_other_non_oids() {
        assert!(RevId::parse(WORK_ALIAS).is_none());
        assert!(RevId::parse("HEAD").is_none());
        assert!(RevId::parse("").is_none());
        assert!(RevId::parse(&OID[..39]).is_none());
        assert!(RevId::parse(&format!("{OID}z")).is_none());
        assert!(RevId::parse(&format!("{OID}++")).is_none());
    }

    #[test]
    fn from_alias_survives_a_clean_tree_and_is_never_stored() {
        let work = RevId::worktree(OID, false);
        let head = RevId::commit(OID);
        assert_eq!(work.text(), head.text());
        assert!(work.is_worktree());
        assert!(!head.is_worktree());
        // Parsed-back storage text claims nothing about how it was read.
        assert!(!RevId::parse(&work.text()).unwrap().is_worktree());
    }

    #[test]
    fn short_keeps_the_worktree_signal() {
        assert_eq!(RevId::worktree(OID, true).short(), "WORK@12345678+");
        assert_eq!(RevId::worktree(OID, false).short(), "WORK@12345678");
        assert_eq!(RevId::commit(OID).short(), "12345678");
    }

    #[test]
    fn git_oid_refuses_the_dirty_form() {
        assert!(GitOid::of(&format!("{OID}+")).is_none());
        assert!(GitOid::of(WORK_ALIAS).is_none());
        assert!(GitOid::of("HEAD").is_none());
        assert_eq!(GitOid::of(OID).unwrap().as_str(), OID);
        // A dirty rev still has a git-resolvable oid; it is just not its text.
        let dirty = RevId::worktree(OID, true);
        assert_eq!(dirty.git_oid().as_str(), OID);
        assert_ne!(dirty.git_oid().as_str(), dirty.text());
    }

    #[test]
    fn revid_is_usable_as_a_map_key() {
        let mut map = std::collections::BTreeMap::new();
        map.insert(RevId::worktree(OID, true), 1);
        map.insert(RevId::commit(OID), 2);
        assert_eq!(map.len(), 2);
    }
}
