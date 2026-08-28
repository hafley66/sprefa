//! The ONE corpus read an `extract rename` run makes: the walk, the file set,
//! and the batch every `Rename` impl answers against. No language is named here.
//! @comment-ok: module header, the seam list every rename file opens with
//!
//! `MoveCx`'s twin, not an extension of it: a move batch is `path -> path` and a
//! rename batch is `(anchor, old) -> new`, so one context per verb keeps every
//! method free of a field it ignores. Same walker, same `SKIP_DIRS`, same
//! root-relative spelling law (`move_cx.rs:26,45,158`).

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::move_cx::SKIP_DIRS;
use crate::types::Rename;

/// Whether the roster hands `rel` to `rename`.
pub fn owned_by<R: Rename + ?Sized>(rel: &str, rename: &R) -> bool {
    crate::lang::rename_for(rel).is_some_and(|owner| owner.name() == rename.name())
}

/// One symbol this run renames. The anchor names the DECLARING file; the
/// declaration in it is found by name, or by `at` when the name is declared twice.
pub struct RenameRequest {
    /// Project-relative path of the declaring file.
    pub anchor: String,
    /// The identifier as written today.
    pub old: String,
    /// What it becomes.
    pub new: String,
    /// Byte offset INSIDE the declaration, when `old` is ambiguous.
    pub at: Option<u32>,
}

/// One `extract rename` run's corpus view, built once and borrowed by every
/// `Rename` impl.
pub struct RenameCx {
    root: PathBuf,
    files: Vec<String>,
    batch: Vec<RenameRequest>,
}

impl RenameCx {
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
        Ok(Self {
            root: root.to_path_buf(),
            files,
            batch: Vec::new(),
        })
    }

    /// The batch this run applies. Set once, before any impl is asked anything.
    pub fn with_batch(mut self, batch: Vec<RenameRequest>) -> Self {
        self.batch = batch;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn files(&self) -> &[String] {
        &self.files
    }

    /// Every corpus file `arm` owns, in path order. Ownership is the roster's
    /// first-match law, never an extension test here.
    pub fn files_of(&self, arm: &dyn Rename) -> Vec<&str> {
        self.files
            .iter()
            .map(String::as_str)
            .filter(|rel| owned_by(rel, arm))
            .collect()
    }

    pub fn read(&self, rel: &str) -> Option<Vec<u8>> {
        std::fs::read(self.abs(rel)).ok()
    }

    pub fn text(&self, rel: &str) -> Option<String> {
        String::from_utf8(self.read(rel)?).ok()
    }

    pub fn batch(&self) -> &[RenameRequest] {
        &self.batch
    }

    pub fn abs(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}

fn rel_of(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let text = relative.to_string_lossy().replace('\\', "/");
    (!text.is_empty()).then_some(text)
}
