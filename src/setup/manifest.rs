//! The setup journal is the only door through which setup writes user files.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum SetupKind { Symlink, FileCreate, JsonMerge, MarkedAppend, HookRegister }
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum SetupDetail {
    Symlink { points_to: PathBuf },
    FileCreate { content_blake3: String },
    JsonMerge { pointer: String, added: Value },
    MarkedAppend { begin_marker: String, end_marker: String },
    HookRegister { event: String, command_substring: String },
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SetupEntry { pub root: Option<PathBuf>, pub target: PathBuf, pub kind: SetupKind, pub detail: SetupDetail, pub wrote_at: i64, pub dl_version: String }
pub struct SetupJournal { path: PathBuf, entries: Vec<SetupEntry> }

impl SetupJournal {
    pub fn load() -> Result<Self> {
        let path = state_home().join("sprefa/setup-manifest.json");
        let entries = match fs::read_to_string(&path) { Ok(s) => serde_json::from_str(&s).context("parse setup manifest")?, Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![], Err(e) => return Err(e.into()) };
        Ok(Self { path, entries })
    }
    fn record(&mut self, entry: SetupEntry) { self.entries.retain(|old| !(old.root == entry.root && old.target == entry.target && old.kind == entry.kind)); self.entries.push(entry); }
    pub fn save(&self) -> Result<()> { atomic(&self.path, &serde_json::to_vec_pretty(&self.entries)?) }
    fn entry(&mut self, root: Option<&Path>, target: &Path, kind: SetupKind, detail: SetupDetail) { self.record(SetupEntry { root: root.map(Path::to_path_buf), target: target.to_path_buf(), kind, detail, wrote_at: now(), dl_version: env!("CARGO_PKG_VERSION").into() }); }
    pub fn create_file(&mut self, root: Option<&Path>, target: &Path, bytes: &[u8]) -> Result<bool> {
        safe(target, root)?;
        match fs::read(target) { Ok(old) if old == bytes => return Ok(false), Ok(_) => { println!("[dl setup] exists, not ours, left alone: {}", target.display()); return Ok(false) }, Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}, Err(e) => return Err(e.into()) }
        parent(target)?; atomic(target, bytes)?; self.entry(root, target, SetupKind::FileCreate, SetupDetail::FileCreate { content_blake3: hash(bytes) }); Ok(true)
    }
    pub fn append_marked(&mut self, root: Option<&Path>, target: &Path, body: &str, begin: &str, end: &str) -> Result<bool> {
        safe(target, root)?; let old = fs::read_to_string(target).unwrap_or_default();
        if old.contains(begin) { if !old.contains(end) { println!("[dl setup] marker tampered, left alone: {}", target.display()); } return Ok(false); }
        let mut new = old; new.push_str(body); parent(target)?; atomic(target, new.as_bytes())?;
        self.entry(root, target, SetupKind::MarkedAppend, SetupDetail::MarkedAppend { begin_marker: begin.into(), end_marker: end.into() }); Ok(true)
    }
    pub fn merge_json(&mut self, root: Option<&Path>, target: &Path, pointer: &str, added: Value) -> Result<bool> {
        safe(target, root)?; let text = match fs::read_to_string(target) { Ok(s) => s, Err(e) if e.kind() == std::io::ErrorKind::NotFound => "{}".into(), Err(e) => return Err(e.into()) };
        let mut value: Value = match serde_json::from_str(&text) { Ok(v) => v, Err(_) => { println!("[dl setup] malformed JSON, left alone: {}", target.display()); return Ok(false) } };
        let Some(obj) = value.as_object_mut() else { println!("[dl setup] JSON is not an object, left alone: {}", target.display()); return Ok(false) };
        let key = pointer.trim_start_matches('/'); let arr = obj.entry(key).or_insert_with(|| Value::Array(vec![])); let Some(arr) = arr.as_array_mut() else { println!("[dl setup] JSON node is not an array, left alone: {}", target.display()); return Ok(false) };
        if arr.contains(&added) { return Ok(false); } arr.push(added.clone()); parent(target)?; atomic(target, &serde_json::to_vec_pretty(&value)?)?;
        self.entry(root, target, SetupKind::JsonMerge, SetupDetail::JsonMerge { pointer: pointer.into(), added }); Ok(true)
    }
    pub fn symlink(&mut self, root: Option<&Path>, target: &Path, points_to: &Path) -> Result<bool> {
        safe(target, root)?; if fs::symlink_metadata(target).is_ok() { println!("[dl setup] exists, not ours, left alone: {}", target.display()); return Ok(false); }
        parent(target)?;
        #[cfg(unix)] { std::os::unix::fs::symlink(points_to, target)?; }
        #[cfg(not(unix))] { fs::copy(points_to, target)?; }
        self.entry(root, target, SetupKind::Symlink, SetupDetail::Symlink { points_to: points_to.into() }); Ok(true)
    }
    pub fn list(&self) -> Result<i32> { for e in &self.entries { println!("{:?} {} ({})", e.kind, e.target.display(), e.wrote_at); } Ok(0) }
    pub fn undo(&mut self, root: Option<&Path>, dry: bool) -> Result<i32> {
        let mut kept = Vec::new(); for entry in self.entries.iter().rev() { if root.is_some_and(|r| entry.root.as_deref() != Some(r)) { kept.push(entry.clone()); continue; } if !safe(&entry.target, entry.root.as_deref()).is_ok() { println!("[dl setup] SKIP unsafe path: {}", entry.target.display()); kept.push(entry.clone()); continue; }
            let action = match &entry.detail {
                SetupDetail::FileCreate { content_blake3 } => match fs::read(&entry.target) { Ok(b) if hash(&b) == *content_blake3 => Some("remove"), Ok(_) => { println!("[dl setup] modified since install, left in place: {}", entry.target.display()); None }, Err(_) => None },
                SetupDetail::Symlink { points_to } => match fs::read_link(&entry.target) { Ok(p) if p == *points_to => Some("remove"), _ => { println!("[dl setup] symlink changed, left in place: {}", entry.target.display()); None } },
                SetupDetail::MarkedAppend { begin_marker, end_marker } => match fs::read_to_string(&entry.target) { Ok(s) if s.contains(begin_marker) && s.contains(end_marker) => Some("strip markers"), _ => { println!("[dl setup] marker changed, left in place: {}", entry.target.display()); None } },
                SetupDetail::JsonMerge { pointer, added } => { if remove_json(&entry.target, pointer, added, dry)? { Some("remove JSON node") } else { None } },
                SetupDetail::HookRegister { .. } => None,
            };
            if let Some(action) = action { println!("[dl setup] {} {}", action, entry.target.display()); if !dry { match &entry.detail { SetupDetail::MarkedAppend { begin_marker, end_marker } => { let s = fs::read_to_string(&entry.target)?; let a = s.find(begin_marker).unwrap(); let z = s[a..].find(end_marker).unwrap() + a + end_marker.len(); atomic(&entry.target, format!("{}{}", &s[..a], &s[z..]).as_bytes())? }, SetupDetail::JsonMerge { .. } => {}, _ => { fs::remove_file(&entry.target)?; } } } } else { kept.push(entry.clone()); }
        } kept.reverse(); self.entries = kept; if !dry { self.save()?; } Ok(0)
    }
    pub fn adopt(&mut self) -> Result<i32> { println!("[dl setup] adopt only records verifiable entries; rerun setup to backfill known wiring."); self.save()?; Ok(0) }
}
fn remove_json(target: &Path, pointer: &str, added: &Value, dry: bool) -> Result<bool> { let s = match fs::read_to_string(target) { Ok(s) => s, Err(_) => return Ok(false) }; let mut v: Value = match serde_json::from_str(&s) { Ok(v) => v, Err(_) => { println!("[dl setup] malformed JSON, left in place: {}", target.display()); return Ok(false) } }; let Some(a) = v.get_mut(pointer.trim_start_matches('/')).and_then(Value::as_array_mut) else { return Ok(false) }; if let Some(i) = a.iter().position(|x| x == added) { if !dry { a.remove(i); atomic(target, &serde_json::to_vec_pretty(&v)?)?; } Ok(true) } else { Ok(false) } }
fn state_home() -> PathBuf { std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state"))).unwrap_or_else(|| PathBuf::from(".")) }
fn parent(path: &Path) -> Result<()> { if let Some(p) = path.parent() { fs::create_dir_all(p)?; } Ok(()) }
fn safe(target: &Path, root: Option<&Path>) -> Result<()> { let expected = root.map(Path::to_path_buf).unwrap_or_else(|| std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))); let parent = target.parent().context("target has no parent")?; let canonical = parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf()); let allowed = canonical.starts_with(&expected) || expected.starts_with(&canonical); if !allowed { anyhow::bail!("target escapes expected root: {}", target.display()) } Ok(()) }
fn atomic(path: &Path, bytes: &[u8]) -> Result<()> { parent(path)?; let tmp = path.with_extension(format!("dl-tmp-{}", std::process::id())); fs::write(&tmp, bytes)?; fs::rename(&tmp, path)?; Ok(()) }
fn hash(bytes: &[u8]) -> String { blake3::hash(bytes).to_hex().to_string() }
fn now() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64 }
