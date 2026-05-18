use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::config::SprfConfig;
use crate::store::SprfStore;
use crate::{Coord, Cursor, FileId, RepoId, RevId};

#[derive(Clone)]
pub struct SourceReader {
    root: PathBuf,
    store: Option<Arc<SprfStore>>,
    config: Option<Arc<SprfConfig>>,
}

#[derive(Clone, Debug)]
pub struct SourceBytes {
    pub bytes: Vec<u8>,
    pub coord: Coord,
    pub path: Arc<str>,
    pub file: FileId,
}

impl SourceReader {
    pub fn new(
        root: impl Into<PathBuf>,
        store: Option<Arc<SprfStore>>,
        config: Option<Arc<SprfConfig>>,
    ) -> Self {
        Self {
            root: root.into(),
            store,
            config,
        }
    }

    pub fn read_cursor(&self, c: &Cursor) -> Option<SourceBytes> {
        self.read_cursor_with_intern(c, true)
    }

    pub fn read_cursor_uninterned(&self, c: &Cursor) -> Option<SourceBytes> {
        self.read_cursor_with_intern(c, false)
    }

    fn read_cursor_with_intern(&self, c: &Cursor, intern: bool) -> Option<SourceBytes> {
        if c.value.is_empty() {
            return self.read_cursor_coord(c, intern);
        }
        let parent = self
            .store
            .as_ref()
            .and_then(|s| s.coord_of(c.at))
            .unwrap_or_default();
        if parent.rev != 0 {
            return self.read_git_path(c.value.as_ref(), parent, intern);
        }
        self.read_worktree_path(c.value.as_ref(), parent.repo, parent.rev, intern)
    }

    pub fn read_path(&self, path: &str) -> Option<SourceBytes> {
        self.read_worktree_path(path, 0, 0, true)
    }

    /// Memo content-fingerprint: resolve `c` exactly as
    /// `read_cursor_uninterned` does, read the bytes, and return
    /// `(absolute_resolved_path, blake3_hex)`. The hash is byte-for-byte
    /// the same digest `SprfStore::intern_file` mints for the same
    /// content, so it is the store's content id without a second
    /// hashing scheme. The absolute path lets the staleness check
    /// re-read the SAME file on a later run with no `root`/clock state.
    /// `None` ⇒ the cursor has no readable file source.
    pub fn fingerprint_cursor(&self, c: &Cursor) -> Option<(String, String)> {
        let resolved = self.resolve_cursor_path(c)?;
        let bytes = std::fs::read(&resolved).ok()?;
        let hex = blake3::hash(&bytes).to_hex().to_string();
        Some((resolved.to_string_lossy().into_owned(), hex))
    }

    /// The absolute filesystem path `read_cursor_uninterned` would read
    /// for `c`. Worktree only (rev == 0); git-backed sources fall back
    /// to the raw `value` string (a stable per-rev key — its bytes do
    /// not change under a worktree edit, which is the case the CLI
    /// incremental run exercises).
    /// The absolute filesystem path `c` resolves to, WITHOUT reading
    /// it. Same resolution as `fingerprint_cursor`/`read_cursor_*`; used
    /// by the no-read memo staleness oracle (`source_index`) so deciding
    /// "this file did not move" never opens the file.
    pub fn resolve_abs(&self, c: &Cursor) -> Option<PathBuf> {
        self.resolve_cursor_path(c)
    }

    fn resolve_cursor_path(&self, c: &Cursor) -> Option<PathBuf> {
        if c.value.is_empty() {
            let store = self.store.as_ref()?;
            let coord = store.coord_of(c.at)?;
            if coord.fs == 0 || coord.rev != 0 {
                return None;
            }
            let path = store.path_of(coord.fs)?;
            return Some(resolve_source_path(&self.root, path.as_ref()));
        }
        let parent = self
            .store
            .as_ref()
            .and_then(|s| s.coord_of(c.at))
            .unwrap_or_default();
        if parent.rev != 0 {
            return None;
        }
        Some(resolve_source_path(&self.root, c.value.as_ref()))
    }

    fn read_cursor_coord(&self, c: &Cursor, intern: bool) -> Option<SourceBytes> {
        let store = self.store.as_ref()?;
        let coord = store.coord_of(c.at)?;
        if coord.fs == 0 {
            return None;
        }
        let path = store.path_of(coord.fs)?;
        if coord.rev != 0 {
            return self.read_git_path(path.as_ref(), coord, intern);
        }
        self.read_worktree_path(path.as_ref(), coord.repo, coord.rev, intern)
    }

    fn read_worktree_path(
        &self,
        path: &str,
        repo: RepoId,
        rev: RevId,
        intern: bool,
    ) -> Option<SourceBytes> {
        let resolved = resolve_source_path(&self.root, path);
        let bytes = std::fs::read(&resolved).ok()?;
        let path_arc: Arc<str> = Arc::from(path);
        let file = intern
            .then(|| self.store.as_ref())
            .flatten()
            .map(|s| {
                let file = s.intern_file(&bytes, path_arc.as_ref());
                let _ = s.intern_path(repo, rev, file, path_arc.as_ref());
                file
            })
            .unwrap_or(0);
        let coord = Coord {
            repo,
            rev,
            fs: file,
            lo: 0,
            hi: bytes.len() as u32,
        };
        Some(SourceBytes {
            bytes,
            coord,
            path: path_arc,
            file,
        })
    }

    fn read_git_path(&self, path: &str, coord: Coord, intern: bool) -> Option<SourceBytes> {
        let store = self.store.as_ref()?;
        let config = self.config.as_ref()?;
        let Some((slug, _remote)) = store.lookup_repo(coord.repo) else {
            return None;
        };
        let root = config
            .repos
            .iter()
            .find(|r| r.slug == slug.as_ref())
            .map(|r| r.root.clone())?;
        let Some((_repo_id, oid, _ts)) = store.lookup_rev(coord.rev) else {
            return None;
        };
        let spec = format!("{}:{}", oid.as_ref(), path);
        let out = std::process::Command::new("git")
            .args(["show", spec.as_str()])
            .current_dir(root)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let file = if intern {
            let file = store.intern_file(&out.stdout, path);
            let _ = store.intern_path(coord.repo, coord.rev, file, path);
            file
        } else {
            0
        };
        let full = Coord {
            repo: coord.repo,
            rev: coord.rev,
            fs: file,
            lo: 0,
            hi: out.stdout.len() as u32,
        };
        Some(SourceBytes {
            bytes: out.stdout,
            coord: full,
            path: Arc::from(path),
            file,
        })
    }
}

pub fn resolve_path_text(root: impl AsRef<Path>, raw: &str) -> PathBuf {
    let expanded = expand_vars(expand_home(raw).as_ref());
    let path = PathBuf::from(expanded);
    let joined = if path.is_absolute() {
        path
    } else {
        root.as_ref().join(path)
    };
    normalize_lexical(&joined)
}

fn resolve_source_path(root: impl AsRef<Path>, raw: &str) -> PathBuf {
    let expanded = expand_vars(expand_home(raw).as_ref());
    let path = PathBuf::from(expanded);
    if path.is_absolute() {
        return normalize_lexical(&path);
    }
    let root = normalize_lexical(root.as_ref());
    if path.starts_with(&root) {
        return normalize_lexical(&path);
    }
    resolve_path_text(root, raw)
}

fn expand_home(raw: &str) -> String {
    if raw == "~" {
        return std::env::var("HOME").unwrap_or_else(|_| raw.to_string());
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    raw.to_string()
}

fn expand_vars(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'$' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            let start = i + 2;
            let Some(end_rel) = raw[start..].find('}') else {
                out.push('$');
                i += 1;
                continue;
            };
            let end = start + end_rel;
            let name = &raw[start..end];
            if let Ok(value) = std::env::var(name) {
                out.push_str(&value);
            }
            i = end + 1;
            continue;
        }
        let start = i + 1;
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        if end == start {
            out.push('$');
            i += 1;
            continue;
        }
        if let Ok(value) = std::env::var(&raw[start..end]) {
            out.push_str(&value);
        }
        i = end;
    }
    out
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}
