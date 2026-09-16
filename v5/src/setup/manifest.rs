//! The setup journal is the only door through which setup writes user files.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

mod actions;
mod json_edit;
mod write;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum SetupKind {
    Symlink,
    FileCreate,
    DirCreate,
    JsonMerge,
    MarkedAppend,
    HookRegister,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum SetupDetail {
    Symlink {
        points_to: PathBuf,
    },
    FileCreate {
        content_blake3: String,
    },
    DirCreate,
    JsonMerge {
        pointer: String,
        added: Value,
    },
    MarkedAppend {
        begin_marker: String,
        end_marker: String,
        #[serde(default)]
        created: bool,
    },
    HookRegister {
        event: String,
        command_substring: String,
    },
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SetupEntry {
    pub root: Option<PathBuf>,
    pub target: PathBuf,
    pub kind: SetupKind,
    pub detail: SetupDetail,
    pub wrote_at: i64,
    pub dl_version: String,
}
pub struct SetupJournal {
    path: PathBuf,
    entries: Vec<SetupEntry>,
}

impl SetupJournal {
    pub fn stage_file(&mut self, target: &Path, bytes: &[u8]) -> Result<()> {
        let temp = std::env::temp_dir().canonicalize()?;
        self.create_file(Some(&temp), target, bytes)?;
        Ok(())
    }
    pub fn record_staged(&mut self, target: &Path) -> Result<()> {
        let temp = std::env::temp_dir().canonicalize()?;
        safe(target, Some(&temp))?;
        let bytes = fs::read(target)?;
        self.entry(
            Some(&temp),
            target,
            SetupKind::FileCreate,
            SetupDetail::FileCreate {
                content_blake3: hash(&bytes),
            },
        );
        Ok(())
    }
    pub fn finish_staged(&mut self, target: &Path) -> Result<()> {
        let owned = self.entries.iter().any(|entry| entry.target == target && matches!(entry.detail,
            SetupDetail::FileCreate { ref content_blake3 } if fs::read(target).ok().is_some_and(|bytes| hash(&bytes) == *content_blake3)));
        if owned {
            fs::remove_file(target)?;
            self.entries.retain(|entry| entry.target != target);
        }
        Ok(())
    }
    pub fn load() -> Result<Self> {
        let path = state_home().join("setup-manifest.json");
        let entries = match fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).context("parse setup manifest")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
            Err(e) => return Err(e.into()),
        };
        Ok(Self { path, entries })
    }
    fn record(&mut self, entry: SetupEntry) {
        self.entries.retain(|old| {
            !(old.root == entry.root && old.target == entry.target && same_slot(old, &entry))
        });
        self.entries.push(entry);
    }
    pub fn save(&self) -> Result<()> {
        atomic(&self.path, &serde_json::to_vec_pretty(&self.entries)?)
    }
    fn entry(&mut self, root: Option<&Path>, target: &Path, kind: SetupKind, detail: SetupDetail) {
        self.record(SetupEntry {
            root: root.map(Path::to_path_buf),
            target: target.to_path_buf(),
            kind,
            detail,
            wrote_at: now(),
            dl_version: env!("CARGO_PKG_VERSION").into(),
        });
    }
    pub fn list(&self) -> Result<i32> {
        for e in &self.entries {
            println!("{:?} {} ({})", e.kind, e.target.display(), e.wrote_at);
        }
        Ok(0)
    }
}

fn json_array_mut<'a>(value: &'a mut Value, pointer: &str) -> Option<&'a mut Vec<Value>> {
    let mut parts = pointer.trim_start_matches('/').split('/').peekable();
    let mut current = value;
    while let Some(key) = parts.next() {
        if parts.peek().is_none() {
            let object = current.as_object_mut()?;
            return object
                .entry(key)
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut();
        }
        let object = current.as_object_mut()?;
        current = object
            .entry(key)
            .or_insert_with(|| Value::Object(Default::default()));
    }
    None
}

fn same_slot(old: &SetupEntry, new: &SetupEntry) -> bool {
    if old.kind != new.kind {
        return false;
    }
    match (&old.detail, &new.detail) {
        (
            SetupDetail::HookRegister { event: a, .. },
            SetupDetail::HookRegister { event: b, .. },
        ) => a == b,
        (
            SetupDetail::JsonMerge {
                pointer: a,
                added: av,
            },
            SetupDetail::JsonMerge {
                pointer: b,
                added: bv,
            },
        ) => a == b && av == bv,
        _ => true,
    }
}

/// The sprefa state dir the setup manifest lives in. Delegates to the single
/// resolver (`daemon::daemon_home`: DL_STATE_DIR > XDG_STATE_HOME > platform
/// default) so the manifest path can never disagree with the roots db home. The
/// cfg(test) branch keeps lib-unit-test isolation when NEITHER env is set —
/// concurrent tests must not share one real home — mirroring the pre-2026-07-21
/// per-test temp dir.
fn state_home() -> PathBuf {
    #[cfg(test)]
    {
        if std::env::var_os("DL_STATE_DIR").is_none()
            && std::env::var_os("XDG_STATE_HOME").is_none()
        {
            return std::env::temp_dir().join(format!(
                "dl-test-state-{}-{:?}/sprefa",
                std::process::id(),
                std::thread::current().id()
            ));
        }
    }
    crate::daemon::daemon_home()
}
fn parent(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let mut missing = Vec::new();
    let mut cursor = parent;
    while !cursor.exists() {
        missing.push(cursor.to_path_buf());
        cursor = cursor.parent().context("path has no existing ancestor")?;
    }
    for dir in missing.iter().rev() {
        fs::create_dir(dir)?;
    }
    Ok(())
}
fn has_hook(value: &Value, event: &str, command: &str) -> bool {
    value
        .get("hooks")
        .and_then(|v| v.get(event))
        .and_then(Value::as_array)
        .is_some_and(|entries| {
            entries.iter().any(|entry| {
                entry
                    .get("hooks")
                    .and_then(Value::as_array)
                    .is_some_and(|hooks| {
                        hooks.iter().any(|hook| {
                            hook.get("command")
                                .and_then(Value::as_str)
                                .is_some_and(|c| c.contains(command))
                        })
                    })
            })
        })
}
fn remove_hook(target: &Path, event: &str, command: &str, dry: bool) -> Result<bool> {
    let text = match fs::read_to_string(target) {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };
    if !has_hook(&value, event, command) {
        return Ok(false);
    }
    if !dry {
        let output = json_edit::remove_hook_command(&text, event, command)
            .context("remove exact hook JSON node")?;
        atomic(target, output.as_bytes())?;
    }
    Ok(true)
}
fn safe(target: &Path, root: Option<&Path>) -> Result<()> {
    let expected = root.map(Path::to_path_buf).unwrap_or_else(|| {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    });
    let expected = expected.canonicalize().unwrap_or(expected);
    if target
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        anyhow::bail!("target contains parent traversal: {}", target.display())
    }
    let parent = target.parent().context("target has no parent")?;
    let existing = parent
        .ancestors()
        .find(|p| p.exists())
        .context("target has no existing ancestor")?;
    let canonical = existing.canonicalize()?;
    if !canonical.starts_with(&expected) {
        anyhow::bail!("target escapes expected root: {}", target.display())
    }
    Ok(())
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    parent(path)?;
    static NEXT_TMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nonce = NEXT_TMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("dl-tmp-{}-{nonce}", std::process::id()));
    fs::write(&tmp, bytes)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&tmp);
            Err(error.into())
        }
    }
}
fn hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
