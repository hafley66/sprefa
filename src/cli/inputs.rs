//! Resolving a positional into something the engine can run. A positional is one
//! of three things, disambiguated without a flag:
//!
//! - a **directory** → merge every `*.dl` inside it (an ad-hoc rail folder),
//! - an existing **file** → run it as given,
//! - otherwise **inline dl source** (`head <- body.` / `? rel(x).`), materialized
//!   to a temp file so the normal file path runs it.
//!
//! Inline source and folders are *ephemeral*: they run in-process (the daemon
//! serves its own `.dl/*.dl` set, never an ad-hoc positional), so the caller
//! forces the in-process path when [`expand`] reports `ephemeral`.

use anyhow::{bail, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The expanded program file list plus whether any input was ephemeral (inline
/// source or a folder merge), which forces the in-process run path.
pub struct Inputs {
    pub files: Vec<String>,
    pub ephemeral: bool,
}

/// Expand each positional into concrete program-file paths. Empty input passes
/// straight through (discovery mode handles it downstream).
pub fn expand(programs: &[String]) -> Result<Inputs> {
    let mut files = Vec::new();
    let mut ephemeral = false;
    for arg in programs {
        let path = Path::new(arg);
        if path.is_dir() {
            let mut dls = dl_files_in(path)?;
            if dls.is_empty() {
                bail!("{arg}: directory has no .dl files");
            }
            dls.sort();
            files.extend(dls);
            ephemeral = true;
        } else if path.exists() {
            files.push(arg.clone());
        } else if looks_like_source(arg) {
            files.push(materialize_inline(arg)?);
            ephemeral = true;
        } else {
            bail!(
                "{arg}: no such file or directory, and not parseable as inline dl \
                 (inline needs a rule `head <- body.` or a query `? rel(x).`)"
            );
        }
    }
    Ok(Inputs { files, ephemeral })
}

/// Every `*.dl` file directly under `dir` (non-recursive), as path strings.
fn dl_files_in(dir: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().map(|e| e == "dl").unwrap_or(false) {
            out.push(path.to_string_lossy().into_owned());
        }
    }
    Ok(out)
}

/// A positional that is not a path on disk: is it inline dl source? A rule has
/// `<-`, a query starts with `?`, and any program has a trailing `.`; a stray
/// mistyped filename (no such punctuation) is NOT source and errors as a missing
/// file instead of a confusing parse error.
pub fn looks_like_source(arg: &str) -> bool {
    let trimmed = arg.trim();
    trimmed.contains("<-")
        || trimmed.starts_with('?')
        || trimmed.contains("? ")
        || trimmed.ends_with('.')
        || trimmed.contains('\n')
}

/// Write inline source to a uniquely-named temp `.dl` file and return its path.
/// Left on disk (the OS reaps the temp dir); a persistent daemon reading it
/// keeps the path valid for the run.
pub fn materialize_inline(source: &str) -> Result<String> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    let name = format!("dl-inline-{}-{:x}.dl", std::process::id(), hasher.finish());
    let path: PathBuf = std::env::temp_dir().join(name);
    let mut file = std::fs::File::create(&path)?;
    file.write_all(source.as_bytes())?;
    // A trailing newline keeps the lexer happy if the source omitted one.
    if !source.ends_with('\n') {
        file.write_all(b"\n")?;
    }
    Ok(path.to_string_lossy().into_owned())
}
