use std::path::{Path, PathBuf};

use anyhow::Result;
use sprefa_config::CompiledFilter;

/// Result of diffing two git commits.
pub struct DiffResult {
    /// (absolute_path, blob_oid) of files added or modified (need extraction).
    pub changed: Vec<(PathBuf, String)>,
    /// Relative paths of files deleted (need rev_files cleanup).
    pub deleted: Vec<String>,
    /// Pure renames where content is identical (old_rel_path, new_rel_path).
    /// These only need a path update on the files row, no re-extraction.
    pub renamed: Vec<(String, String)>,
    /// HEAD sha after the diff.
    pub new_sha: String,
}

/// Compute files changed between `old_sha` and current HEAD.
/// Returns absolute paths for changed files, relative paths for deleted.
/// Errors if `old_sha` cannot be resolved (e.g. garbage collected after force push).
pub fn diff_files(
    repo_path: &Path,
    old_sha: &str,
    filter: Option<&CompiledFilter>,
) -> Result<DiffResult> {
    let repo = git2::Repository::open(repo_path)?;
    let old_oid = git2::Oid::from_str(old_sha)?;
    let old_commit = repo.find_commit(old_oid)?;
    let old_tree = old_commit.tree()?;

    let head = repo.head()?.peel_to_commit()?;
    let new_tree = head.tree()?;
    let new_sha = head.id().to_string();

    let diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)?;

    let mut changed = Vec::new();
    let mut deleted = Vec::new();
    let mut renamed = Vec::new();

    for delta in diff.deltas() {
        match delta.status() {
            git2::Delta::Added | git2::Delta::Modified | git2::Delta::Copied => {
                if let Some(path) = delta.new_file().path() {
                    let rel = path.to_string_lossy();
                    if filter.map(|f| f.allows(&rel)).unwrap_or(true) {
                        let oid = delta.new_file().id().to_string();
                        changed.push((repo_path.join(path), oid));
                    }
                }
            }
            git2::Delta::Deleted => {
                if let Some(path) = delta.old_file().path() {
                    let rel = path.to_string_lossy().to_string();
                    if filter.map(|f| f.allows(&rel)).unwrap_or(true) {
                        deleted.push(rel);
                    }
                }
            }
            git2::Delta::Renamed => {
                let old_rel = delta
                    .old_file()
                    .path()
                    .map(|p| p.to_string_lossy().to_string());
                let new_rel = delta
                    .new_file()
                    .path()
                    .map(|p| p.to_string_lossy().to_string());
                let same_content = delta.old_file().id() == delta.new_file().id();

                match (old_rel, new_rel) {
                    (Some(old), Some(new)) if same_content => {
                        let old_ok = filter.map(|f| f.allows(&old)).unwrap_or(true);
                        let new_ok = filter.map(|f| f.allows(&new)).unwrap_or(true);
                        if old_ok && new_ok {
                            renamed.push((old, new));
                        } else {
                            // Filter mismatch: treat as delete + add independently.
                            if old_ok {
                                deleted.push(old);
                            }
                            if new_ok {
                                let oid = delta.new_file().id().to_string();
                                changed.push((repo_path.join(&new), oid));
                            }
                        }
                    }
                    (Some(old), Some(new)) => {
                        // Content changed during rename: delete old, extract new.
                        if filter.map(|f| f.allows(&old)).unwrap_or(true) {
                            deleted.push(old);
                        }
                        if filter.map(|f| f.allows(&new)).unwrap_or(true) {
                            let oid = delta.new_file().id().to_string();
                            changed.push((repo_path.join(&new), oid));
                        }
                    }
                    (Some(old), None) => {
                        if filter.map(|f| f.allows(&old)).unwrap_or(true) {
                            deleted.push(old);
                        }
                    }
                    (None, Some(new)) => {
                        if filter.map(|f| f.allows(&new)).unwrap_or(true) {
                            let oid = delta.new_file().id().to_string();
                            changed.push((repo_path.join(&new), oid));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    Ok(DiffResult {
        changed,
        deleted,
        renamed,
        new_sha,
    })
}

/// List all indexable files under `repo_path`.
/// Returns (absolute_path, content_hash) pairs. Git repos use blob OIDs (free
/// from the tree walk). Non-git fallback uses mmap + xxh3.
/// Applies `filter` if provided.
pub fn list_files(repo_path: &Path, filter: Option<&CompiledFilter>) -> Result<Vec<(PathBuf, String)>> {
    let files = git2_list_files(repo_path).unwrap_or_else(|_| {
        walkdir_files(repo_path)
            .into_iter()
            .filter_map(|p| {
                let file = std::fs::File::open(&p).ok()?;
                let mmap = unsafe { memmap2::Mmap::map(&file).ok()? };
                let hash = format!("{:x}", xxhash_rust::xxh3::xxh3_128(&mmap));
                Some((p, hash))
            })
            .collect()
    });

    let filtered = files
        .into_iter()
        .filter(|(p, _)| {
            let rel = p.strip_prefix(repo_path).unwrap_or(p);
            let rel_str = rel.to_string_lossy();
            filter
                .map(|f: &CompiledFilter| f.allows(&rel_str))
                .unwrap_or(true)
        })
        .collect();

    Ok(filtered)
}

fn git2_list_files(repo_path: &Path) -> Result<Vec<(PathBuf, String)>> {
    let repo = git2::Repository::open(repo_path)?;
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;
    let tree = commit.tree()?;

    let mut paths = Vec::new();
    tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            let rel = if root.is_empty() {
                PathBuf::from(entry.name().unwrap_or(""))
            } else {
                PathBuf::from(root).join(entry.name().unwrap_or(""))
            };
            let oid = entry.id().to_string();
            paths.push((repo_path.join(rel), oid));
        }
        git2::TreeWalkResult::Ok
    })?;

    Ok(paths)
}

/// A git revision (branch or tag) with its commit hash.
#[derive(Debug, Clone)]
pub struct GitRev {
    pub name: String,
    pub commit_hash: String,
    pub is_tag: bool,
}

/// Read all branches and tags from a git repository.
/// Annotated tags are peeled to their commit.
pub fn read_git_revs(repo_path: &Path) -> Result<Vec<GitRev>> {
    let repo = git2::Repository::open(repo_path)?;
    let mut revs = Vec::new();

    for reference in repo.references()? {
        let reference = match reference {
            Ok(r) => r,
            Err(_) => continue,
        };
        let refname = match reference.name() {
            Some(n) => n.to_string(),
            None => continue,
        };

        let (name, is_tag) = if let Some(branch) = refname.strip_prefix("refs/heads/") {
            (branch.to_string(), false)
        } else if let Some(tag) = refname.strip_prefix("refs/tags/") {
            (tag.to_string(), true)
        } else {
            continue;
        };

        let commit_hash = reference
            .peel_to_commit()
            .map(|c| c.id().to_string())
            .unwrap_or_default();

        if !commit_hash.is_empty() {
            revs.push(GitRev {
                name,
                commit_hash,
                is_tag,
            });
        }
    }

    Ok(revs)
}

/// Check whether a tag name looks like semver (v?MAJOR.MINOR.PATCH with optional suffix).
pub fn is_semver(name: &str) -> bool {
    let s = name.strip_prefix('v').unwrap_or(name);
    let mut parts = s.splitn(4, |c: char| c == '.' || c == '-' || c == '+');
    let major = parts.next().and_then(|p| p.parse::<u64>().ok());
    let minor = parts.next().and_then(|p| p.parse::<u64>().ok());
    let patch = parts.next().and_then(|p| p.parse::<u64>().ok());
    major.is_some() && minor.is_some() && patch.is_some()
}

/// Read file contents from a git tree at an arbitrary revision (tag, branch, sha).
/// Returns `(relative_path, blob_oid, blob_bytes)` triples. No checkout needed.
pub fn list_blobs_at_rev(
    repo_path: &Path,
    rev: &str,
    filter: Option<&CompiledFilter>,
) -> Result<Vec<(String, String, Vec<u8>)>> {
    let repo = git2::Repository::open(repo_path)?;
    let obj = repo.revparse_single(rev)?;
    let tree = obj.peel_to_tree()?;

    let mut blobs = Vec::new();
    tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
        if entry.kind() != Some(git2::ObjectType::Blob) {
            return git2::TreeWalkResult::Ok;
        }
        let name = match entry.name() {
            Some(n) => n,
            None => return git2::TreeWalkResult::Ok,
        };
        let rel = if root.is_empty() {
            name.to_string()
        } else {
            format!("{root}{name}")
        };
        if let Some(f) = filter {
            if !f.allows(&rel) {
                return git2::TreeWalkResult::Ok;
            }
        }
        let oid = entry.id().to_string();
        if let Ok(obj) = entry.to_object(&repo) {
            if let Ok(blob) = obj.peel_to_blob() {
                blobs.push((rel, oid, blob.content().to_vec()));
            }
        }
        git2::TreeWalkResult::Ok
    })?;

    Ok(blobs)
}

fn walkdir_files(repo_path: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(repo_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !(e.depth() == 1 && name.starts_with('.'))
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Oid, Repository, Signature};
    use std::fs;

    fn make_git_repo(dir: &Path) -> Repository {
        let repo = Repository::init(dir).unwrap();
        // Need an initial commit so HEAD exists.
        let sig = Signature::now("test", "test@test.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        {
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
                .unwrap();
        }
        repo
    }

    fn commit_file(repo: &Repository, path: &str, content: &[u8]) -> Oid {
        let root = repo.workdir().unwrap();
        let abs = root.join(path);
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&abs, content).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(path)).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = Signature::now("test", "test@test.com").unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &format!("add {path}"),
            &tree,
            &[&parent],
        )
        .unwrap()
    }

    fn delete_file(repo: &Repository, path: &str) -> Oid {
        let root = repo.workdir().unwrap();
        fs::remove_file(root.join(path)).unwrap();
        let mut index = repo.index().unwrap();
        index.remove_path(Path::new(path)).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = Signature::now("test", "test@test.com").unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &format!("rm {path}"),
            &tree,
            &[&parent],
        )
        .unwrap()
    }

    #[test]
    fn diff_detects_added_file() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = make_git_repo(tmp.path());
        let old_sha = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        commit_file(&repo, "src/a.ts", b"hello");

        let result = diff_files(tmp.path(), &old_sha, None).unwrap();
        assert_eq!(result.changed.len(), 1);
        assert!(result.changed[0].0.ends_with("src/a.ts"));
        assert!(!result.changed[0].1.is_empty()); // blob OID
        assert!(result.deleted.is_empty());
        assert!(result.renamed.is_empty());
    }

    #[test]
    fn diff_detects_deleted_file() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = make_git_repo(tmp.path());
        commit_file(&repo, "src/a.ts", b"hello");
        let old_sha = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        delete_file(&repo, "src/a.ts");

        let result = diff_files(tmp.path(), &old_sha, None).unwrap();
        assert!(result.changed.is_empty());
        assert_eq!(result.deleted, vec!["src/a.ts"]);
    }

    #[test]
    fn diff_detects_modified_file() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = make_git_repo(tmp.path());
        commit_file(&repo, "src/a.ts", b"v1");
        let old_sha = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        commit_file(&repo, "src/a.ts", b"v2");

        let result = diff_files(tmp.path(), &old_sha, None).unwrap();
        assert_eq!(result.changed.len(), 1);
        assert!(result.changed[0].0.ends_with("src/a.ts"));
    }

    #[test]
    fn diff_bad_sha_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        make_git_repo(tmp.path());
        assert!(diff_files(tmp.path(), "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef", None).is_err());
    }

    #[test]
    fn is_semver_valid() {
        assert!(is_semver("1.0.0"));
        assert!(is_semver("v1.0.0"));
        assert!(is_semver("v2.3.4-rc1"));
        assert!(is_semver("0.1.0+build.123"));
    }

    #[test]
    fn is_semver_invalid() {
        assert!(!is_semver("latest"));
        assert!(!is_semver("v1"));
        assert!(!is_semver("1.0"));
        assert!(!is_semver("release-2024"));
    }

    #[test]
    fn read_revs_from_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = make_git_repo(tmp.path());
        commit_file(&repo, "a.txt", b"data");

        // Create a lightweight tag.
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.tag_lightweight("v1.0.0", head.as_object(), false)
            .unwrap();

        let revs = read_git_revs(tmp.path()).unwrap();
        // Should have the branch (master or main) + the tag
        let tag = revs
            .iter()
            .find(|r| r.name == "v1.0.0")
            .expect("tag not found");
        assert!(tag.is_tag);
        assert!(!tag.commit_hash.is_empty());
        let branch = revs.iter().find(|r| !r.is_tag).expect("branch not found");
        assert!(!branch.commit_hash.is_empty());
    }

    #[test]
    fn diff_returns_new_sha() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = make_git_repo(tmp.path());
        let old_sha = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        commit_file(&repo, "x.txt", b"data");
        let expected = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();

        let result = diff_files(tmp.path(), &old_sha, None).unwrap();
        assert_eq!(result.new_sha, expected);
    }

    #[test]
    fn blobs_at_tag_returns_tagged_content_not_head() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = make_git_repo(tmp.path());
        commit_file(&repo, "src/lib.rs", b"fn v1() {}");
        commit_file(&repo, "Cargo.toml", b"[package]\nname = \"foo\"");

        // Tag current state as v1.0.0
        let v1_commit = repo.head().unwrap().peel_to_commit().unwrap();
        repo.tag_lightweight("v1.0.0", v1_commit.as_object(), false)
            .unwrap();

        // Modify files at HEAD (after the tag)
        commit_file(&repo, "src/lib.rs", b"fn v2() {}");
        commit_file(&repo, "src/new.rs", b"// added after tag");

        let blobs = list_blobs_at_rev(tmp.path(), "v1.0.0", None).unwrap();
        let paths: Vec<&str> = blobs.iter().map(|(p, _, _)| p.as_str()).collect();

        // Should see the files as they were at v1.0.0
        assert!(paths.contains(&"src/lib.rs"));
        assert!(paths.contains(&"Cargo.toml"));
        // new.rs was added after the tag
        assert!(!paths.contains(&"src/new.rs"));

        // Content should be the v1 version
        let lib_content = blobs.iter().find(|(p, _, _)| p == "src/lib.rs").unwrap();
        assert_eq!(lib_content.2, b"fn v1() {}");
        assert!(!lib_content.1.is_empty()); // blob OID present
    }

    #[test]
    fn blobs_at_rev_respects_filter() {
        use sprefa_config::{FilterConfig, FilterMode};

        let tmp = tempfile::tempdir().unwrap();
        let repo = make_git_repo(tmp.path());
        commit_file(&repo, "src/lib.rs", b"code");
        commit_file(&repo, "docs/readme.md", b"readme");

        let filter = CompiledFilter::compile(&FilterConfig {
            mode: FilterMode::Include,
            include: Some(vec!["src/**".into()]),
            exclude: None,
        })
        .unwrap();
        let blobs = list_blobs_at_rev(tmp.path(), "HEAD", Some(&filter)).unwrap();
        let paths: Vec<&str> = blobs.iter().map(|(p, _, _)| p.as_str()).collect();

        assert!(paths.contains(&"src/lib.rs"));
        assert!(!paths.contains(&"docs/readme.md"));
    }

    #[test]
    fn blobs_at_rev_bad_rev_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        make_git_repo(tmp.path());
        assert!(list_blobs_at_rev(tmp.path(), "nonexistent-tag", None).is_err());
    }
}
