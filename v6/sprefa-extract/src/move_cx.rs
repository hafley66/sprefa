//! The ONE corpus read an `extract move` run makes: the walk, the file set, and
//! the batch every `Rehome` impl answers against. No language is named here.
//! @comment-ok: module header, the seam list every move file opens with
//!
//! BUY NOTE. `ignore` 0.4.33 is already in this lock (an ast-grep transitive)
//! and its `WalkBuilder` is the walker soopy itself uses
//! (`soopy/src/_3a_files.rs:6`). soopy's own `DirectoryRoot::snapshot` hashes
//! every file it visits (`soopy/src/_3a_files.rs:29-80`), which a path-only
//! corpus scan must not pay, so the walker is taken directly and the hashing is
//! left to the actions that need an `expected` content id.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use ignore::WalkBuilder;

use crate::types::Rehome;

/// Whether the roster hands `rel` to `rehome`.
pub fn owned_by<R: Rehome + ?Sized>(rel: &str, rehome: &R) -> bool {
    crate::lang::rehome_for(rel).is_some_and(|owner| owner.name() == rehome.name())
}

/// Directories a move never reads. Git's own store, build output that is not
/// the corpus, and the worktree pool a lane runs in.
pub const SKIP_DIRS: [&str; 4] = [".git", "target", "node_modules", ".boop-worktrees"];

/// One `extract move` run's corpus view, built once and borrowed by every
/// `Rehome` impl. NOT `ProjectCx` (`types.rs:1389`): that one is content-local
/// by contract and carries no worktree root, while a move resolves against
/// on-disk truth through `oxc_resolver`.
/// @comment-ok: the ProjectCx split is a decision the signature cannot show
pub struct MoveCx {
    root: PathBuf,
    files: Vec<String>,
    present: BTreeSet<String>,
    moved: BTreeMap<String, String>,
    shim: bool,
    relocate_mod: bool,
}

impl MoveCx {
    /// One walk of `root`. `root` is taken canonicalized; every path this type
    /// hands out is root-relative and forward-slashed.
    pub fn open(root: &Path) -> Result<Self, String> {
        let mut files = Vec::new();
        let walk = WalkBuilder::new(root)
            .hidden(false)
            .ignore(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .filter_entry(|entry| {
                !SKIP_DIRS.contains(&entry.file_name().to_string_lossy().as_ref())
            })
            .build();
        for entry in walk {
            let entry = entry.map_err(|error| format!("walk {}: {error}", root.display()))?;
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let Some(rel) = rel_of(root, entry.path()) else {
                continue;
            };
            files.push(rel);
        }
        files.sort();
        let present = files.iter().cloned().collect();
        Ok(Self {
            root: root.to_path_buf(),
            files,
            present,
            moved: BTreeMap::new(),
            shim: false,
            relocate_mod: false,
        })
    }

    /// The batch this run applies, old rel -> new rel, plus whether the run
    /// leaves a shim behind. Set once, before any impl is asked anything.
    pub fn with_batch(mut self, moved: BTreeMap<String, String>, shim: bool) -> Self {
        self.moved = moved;
        self.shim = shim;
        self
    }

    /// Opt in to relocating a moved Rust module's `mod` declaration into its
    /// new parent and respelling `use` paths, instead of a `#[path]` attribute.
    pub fn with_relocate_mod(mut self, relocate_mod: bool) -> Self {
        self.relocate_mod = relocate_mod;
        self
    }

    pub fn relocate_mod(&self) -> bool {
        self.relocate_mod
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn files(&self) -> &[String] {
        &self.files
    }

    /// Every corpus file `rehome` owns, in path order. Ownership is the roster's
    /// first-match law, never an extension test here.
    pub fn files_of(&self, rehome: &dyn Rehome) -> Vec<&str> {
        self.files
            .iter()
            .map(String::as_str)
            .filter(|rel| owned_by(rel, rehome))
            .collect()
    }

    /// Whether the pre-move tree holds `rel`.
    pub fn contains(&self, rel: &str) -> bool {
        self.present.contains(rel)
    }

    pub fn read(&self, rel: &str) -> Option<Vec<u8>> {
        std::fs::read(self.abs(rel)).ok()
    }

    pub fn text(&self, rel: &str) -> Option<String> {
        String::from_utf8(self.read(rel)?).ok()
    }

    pub fn moved(&self) -> &BTreeMap<String, String> {
        &self.moved
    }

    /// Where `rel` lands, when this run moves it.
    pub fn destination(&self, rel: &str) -> Option<&str> {
        self.moved.get(rel).map(String::as_str)
    }

    /// `rel`'s path once the batch lands: its destination when it moves, itself
    /// when it stays.
    pub fn after<'a>(&'a self, rel: &'a str) -> &'a str {
        self.destination(rel).unwrap_or(rel)
    }

    pub fn shim(&self) -> bool {
        self.shim
    }

    pub fn abs(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// `path`'s root-relative spelling, or None when it sits outside the root.
    pub fn rel(&self, path: &Path) -> Option<String> {
        rel_of(&self.root, path)
    }
}

fn rel_of(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let text = relative.to_string_lossy().replace('\\', "/");
    (!text.is_empty()).then_some(text)
}

/// `from_dir` -> `target` as a relative path, both root-relative and
/// forward-slashed. A directory of `""` is the root itself.
pub fn relative_between(from_dir: &str, target: &str) -> String {
    let from: Vec<&str> = from_dir
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let to: Vec<&str> = target.split('/').filter(|part| !part.is_empty()).collect();
    let mut shared = 0;
    while shared < from.len() && shared < to.len() && from[shared] == to[shared] {
        shared += 1;
    }
    let mut parts: Vec<&str> = vec![".."; from.len() - shared];
    parts.extend(&to[shared..]);
    parts.join("/")
}

/// The directory part of a root-relative path, `""` for a file at the root.
pub fn dirname(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("")
}

/// File name without directory or extension: "src/a/b.rs" -> "b".
pub(crate) fn stem(rel: &str) -> String {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.rsplit_once('.')
        .map(|(head, _)| head)
        .unwrap_or(name)
        .to_string()
}

/// `dir` joined with `rel`, both root-relative and forward-slashed, with `.`
/// and `..` steps folded out.
pub fn join_rel(dir: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// A path with `.` and `..` steps folded out, kept absolute when it was.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}
