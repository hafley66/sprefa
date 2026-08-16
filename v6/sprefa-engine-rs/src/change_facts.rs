// The rev-pair change plane. Column constants live beside the mechanics, the
// way dep_resolve.rs holds the crawl's.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{bail, Context, Result};

pub const GIT_CHANGE_COLUMNS: &[&str] = &["change_kind", "path"];
pub const GIT_RENAME_COLUMNS: &[&str] = &["path_from", "path_to"];
pub const GIT_CHANGED_LINE_COLUMNS: &[&str] = &["path", "line_number"];

/// Git's own binary window: a NUL in the first 8000 bytes
/// (`xdiff-interface.c`, `buffer_is_binary`).
const FIRST_FEW_BYTES: usize = 8000;

/// The three single-path kinds. A rename carries two paths and is a separate
/// relation rather than a variant here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChangeKind {
    Created,
    Deleted,
    Modified,
}

impl ChangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ChangeKind::Created => "created",
            ChangeKind::Deleted => "deleted",
            ChangeKind::Modified => "modified",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathChange {
    pub kind: ChangeKind,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathRename {
    pub path_from: String,
    pub path_to: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChangedLine {
    pub path: String,
    pub line_number: i64,
}

/// One answer per (repository, base, head). The four kinds PARTITION the diff:
/// a renamed path is in `renames` and in neither `changes` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RevisionDiff {
    pub changes: Vec<PathChange>,
    pub renames: Vec<PathRename>,
    pub changed_lines: Vec<ChangedLine>,
}

pub trait IRevisionDiffer: Sync {
    fn diff(&self, repository_root: &str, rev_base: &str, rev_head: &str) -> Result<RevisionDiff>;
}

/// Two Soopy tracked-file listings for the tree shape, one `cat-file --batch`
/// for every blob the line diff reads.
#[derive(Default)]
pub struct SoopyRevisionDiffer;

type Listing = BTreeMap<String, String>;

fn listing_at(tree: &mut soopy::SourceTree, revision: &str) -> Result<Listing> {
    let entries = tree
        .git_files(&soopy::GitFilesQuery {
            revision: soopy::Revision::Named(Arc::from(revision)),
            pathspecs: Vec::new(),
        })
        .with_context(|| format!("enumerate tracked files at {revision}"))?;
    let mut listing = Listing::new();
    for entry in entries {
        let soopy::ContentId::GitBlob(oid) = entry.content else {
            bail!("the tracked-file feed returned a non-Git content id at {revision}");
        };
        listing.insert(entry.source.path.0.to_string(), oid.0.to_string());
    }
    Ok(listing)
}

/// Exact-content renames only, the `git diff --name-status -M100%` spelling.
/// Surplus arrivals of one blob stay creations, so order cannot move the answer.
fn take_renames(
    created: &mut Vec<String>,
    deleted: &mut Vec<String>,
    base: &Listing,
    head: &Listing,
) -> Vec<PathRename> {
    let group = |paths: &[String], at: &Listing| -> BTreeMap<String, Vec<String>> {
        let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for path in paths {
            if let Some(oid) = at.get(path) {
                grouped.entry(oid.clone()).or_default().push(path.clone());
            }
        }
        grouped
    };
    let arrived = group(created, head);
    let departed = group(deleted, base);
    let mut renames = Vec::new();
    let mut paired_from: Vec<String> = Vec::new();
    let mut paired_to: Vec<String> = Vec::new();
    for (oid, from_paths) in &departed {
        let Some(to_paths) = arrived.get(oid) else {
            continue;
        };
        for (from, to) in from_paths.iter().zip(to_paths) {
            renames.push(PathRename {
                path_from: from.clone(),
                path_to: to.clone(),
            });
            paired_from.push(from.clone());
            paired_to.push(to.clone());
        }
    }
    created.retain(|path| !paired_to.contains(path));
    deleted.retain(|path| !paired_from.contains(path));
    renames.sort();
    renames
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(FIRST_FEW_BYTES)].contains(&0)
}

/// Head-side line numbers, 1-based. Myers plus `postprocess_lines` is Git's own
/// pair; a pure-removal hunk has an empty head-side range and emits nothing.
pub fn changed_lines_of(base: &[u8], head: &[u8]) -> Vec<i64> {
    if is_binary(base) || is_binary(head) {
        return Vec::new();
    }
    let input = imara_diff::InternedInput::new(base, head);
    let mut diff = imara_diff::Diff::compute(imara_diff::Algorithm::Myers, &input);
    diff.postprocess_lines(&input);
    diff.hunks()
        .flat_map(|hunk| hunk.after.start..hunk.after.end)
        .map(|line| i64::from(line) + 1)
        .collect()
}

impl IRevisionDiffer for SoopyRevisionDiffer {
    fn diff(&self, repository_root: &str, rev_base: &str, rev_head: &str) -> Result<RevisionDiff> {
        let repository = soopy::discover(std::path::PathBuf::from(repository_root))
            .with_context(|| format!("open repository {repository_root}"))?;
        let root = repository.root.clone();
        let mut tree = soopy::SourceTree::open(repository);
        let base = listing_at(&mut tree, rev_base)?;
        let head = listing_at(&mut tree, rev_head)?;

        let mut created: Vec<String> = head
            .keys()
            .filter(|path| !base.contains_key(*path))
            .cloned()
            .collect();
        let mut deleted: Vec<String> = base
            .keys()
            .filter(|path| !head.contains_key(*path))
            .cloned()
            .collect();
        let modified: Vec<String> = head
            .iter()
            .filter(|(path, oid)| matches!(base.get(*path), Some(was) if was != *oid))
            .map(|(path, _)| path.clone())
            .collect();

        let renames = take_renames(&mut created, &mut deleted, &base, &head);

        let mut changes: Vec<PathChange> =
            Vec::with_capacity(created.len() + deleted.len() + modified.len());
        for (kind, paths) in [
            (ChangeKind::Created, &created),
            (ChangeKind::Deleted, &deleted),
            (ChangeKind::Modified, &modified),
        ] {
            changes.extend(paths.iter().map(|path| PathChange {
                kind,
                path: path.clone(),
            }));
        }
        changes.sort();

        // One batch process drives every blob read, never one per path.
        let mut batch = soopy::GitBatch::open(&root)
            .with_context(|| format!("open a blob batch in {repository_root}"))?;
        let empty: Arc<[u8]> = Arc::from(Vec::new());
        let mut changed_lines = Vec::new();
        for path in created.iter().chain(modified.iter()) {
            let head_bytes = batch
                .read(&soopy::ObjectId(Arc::from(head[path].as_str())))
                .with_context(|| format!("read {path} at {rev_head}"))?;
            let base_bytes = match base.get(path) {
                None => empty.clone(),
                Some(oid) => batch
                    .read(&soopy::ObjectId(Arc::from(oid.as_str())))
                    .with_context(|| format!("read {path} at {rev_base}"))?,
            };
            changed_lines.extend(changed_lines_of(&base_bytes, &head_bytes).into_iter().map(
                |line_number| ChangedLine {
                    path: path.clone(),
                    line_number,
                },
            ));
        }
        changed_lines.sort();

        Ok(RevisionDiff {
            changes,
            renames,
            changed_lines,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(base: &str, head: &str) -> Vec<i64> {
        changed_lines_of(base.as_bytes(), head.as_bytes())
    }

    fn at(pairs: &[(&str, &str)]) -> Listing {
        pairs
            .iter()
            .map(|(path, oid)| ((*path).to_string(), (*oid).to_string()))
            .collect()
    }

    fn owned(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|path| (*path).to_string()).collect()
    }

    #[test]
    fn a_creation_names_every_head_line() {
        assert_eq!(lines("", "one\ntwo\nthree\n"), vec![1, 2, 3]);
    }

    #[test]
    fn an_empty_head_blob_names_no_line() {
        assert_eq!(lines("", ""), Vec::<i64>::new());
    }

    /// The V5 contract: a hunk that only removes lines has no head side.
    #[test]
    fn a_pure_removal_names_no_line() {
        assert_eq!(lines("a\nb\nc\n", "a\nc\n"), Vec::<i64>::new());
    }

    #[test]
    fn an_appended_line_is_the_last_head_line() {
        assert_eq!(lines("a\nb\n", "a\nb\nc\n"), vec![3]);
    }

    /// `git diff -U0` prints `@@ -3 +3 @@` here, a modification of line 3.
    #[test]
    fn a_dropped_trailing_newline_names_the_last_line() {
        assert_eq!(lines("a\nb\nc\n", "a\nb\nc"), vec![3]);
    }

    #[test]
    fn a_binary_blob_names_no_line() {
        assert_eq!(changed_lines_of(b"a\nb\n", b"a\n\0b\n"), Vec::<i64>::new());
        assert_eq!(changed_lines_of(b"a\n\0b\n", b"a\nb\n"), Vec::<i64>::new());
    }

    /// Non-UTF-8 with no NUL is TEXT to Git, so the diff runs over bytes.
    #[test]
    fn latin1_bytes_are_text_and_keep_their_lines() {
        assert_eq!(
            changed_lines_of(b"caf\xe9\n", b"caf\xe9\nth\xe9\n"),
            vec![2]
        );
    }

    #[test]
    fn a_rename_pairs_one_departure_with_one_arrival() {
        let base = at(&[("old.txt", "oid-a")]);
        let head = at(&[("new.txt", "oid-a")]);
        let (mut created, mut deleted) = (owned(&["new.txt"]), owned(&["old.txt"]));
        let renames = take_renames(&mut created, &mut deleted, &base, &head);
        assert_eq!(
            renames,
            vec![PathRename {
                path_from: "old.txt".to_string(),
                path_to: "new.txt".to_string(),
            }]
        );
        assert!(created.is_empty() && deleted.is_empty());
    }

    /// A departure and an arrival carrying DIFFERENT blobs are a deletion and a
    /// creation: the exact-content decision, stated as a test.
    #[test]
    fn different_content_is_never_a_rename() {
        let base = at(&[("old.txt", "oid-b")]);
        let head = at(&[("new.txt", "oid-a")]);
        let (mut created, mut deleted) = (owned(&["new.txt"]), owned(&["old.txt"]));
        assert!(take_renames(&mut created, &mut deleted, &base, &head).is_empty());
        assert_eq!(created, owned(&["new.txt"]));
        assert_eq!(deleted, owned(&["old.txt"]));
    }

    /// One departure, two arrivals of that blob: one rename, and the surplus
    /// arrival stays a creation.
    #[test]
    fn a_surplus_arrival_stays_a_creation() {
        let base = at(&[("old.txt", "oid-a")]);
        let head = at(&[("a_new.txt", "oid-a"), ("b_new.txt", "oid-a")]);
        let (mut created, mut deleted) = (owned(&["a_new.txt", "b_new.txt"]), owned(&["old.txt"]));
        let renames = take_renames(&mut created, &mut deleted, &base, &head);
        assert_eq!(renames.len(), 1);
        assert_eq!(renames[0].path_to, "a_new.txt");
        assert_eq!(created, owned(&["b_new.txt"]));
        assert!(deleted.is_empty());
    }
}
