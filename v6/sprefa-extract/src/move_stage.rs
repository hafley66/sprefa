//! The soopy boundary an `extract move` run stages through: the state root, the
//! stage-and-commit pair, and the temp mirror a dry run commits into. Lang-free
//! and plan-free; it takes actions and returns previews.
//! @comment-ok: module header, the seam list every move file opens with

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::drain::{bind_action, source_rel};

/// `--state` as asked, else `$HOME/.agent/soopy-state`.
pub fn state_root(requested: Option<&Path>) -> Result<PathBuf, String> {
    let root = match requested {
        Some(path) => path.to_path_buf(),
        None => {
            let home = std::env::var_os("HOME")
                .ok_or_else(|| "HOME is unset and --state was not supplied".to_string())?;
            PathBuf::from(home).join(".agent").join("soopy-state")
        }
    };
    std::fs::create_dir_all(&root)
        .map_err(|error| format!("create state root {}: {error}", root.display()))?;
    root.canonicalize()
        .map_err(|error| format!("canonicalize state root {}: {error}", root.display()))
}

fn stage_into<S: soopy::StageStore>(
    source_root: &mut soopy::SourceRoot,
    request: &soopy::StageRequest,
    store: &mut S,
) -> Result<soopy::StagedSourceTransaction, String> {
    let sealed = soopy::stage_mutations(source_root, request, store)
        .map_err(|refused| format!("stage refused: {refused}"))?;
    // `save` returns the manifest; only `load` rehydrates the blobs commit writes.
    soopy::show_stage(store, sealed.id)
        .map_err(|error| format!("load stage {}: {error}", sealed.id))?
        .ok_or_else(|| format!("stage {} vanished from the store", sealed.id))
}

/// One stage's id and previews, committed against `root`.
pub fn stage_and_commit(
    root: &Path,
    state: &Path,
    actions: &[soopy::SourceAction],
    durability: soopy::Durability,
) -> Result<(String, Vec<soopy::FilePreview>), String> {
    let mut source_root = soopy::SourceRoot::open_directory(root)
        .map_err(|error| format!("open root {}: {error}", root.display()))?;
    let identity = source_root.directory().identity.clone();
    let root_id = soopy::SourceRootId::Directory {
        directory: identity.clone(),
    };
    let mut bound = Vec::with_capacity(actions.len());
    for action in actions {
        bound.push(bind_action(root, &identity, action)?);
    }
    let request = soopy::StageRequest::new(root_id, bound);
    // A dry run stages in memory and commits without device flushes: the mirror
    // is discarded whole, so durability would buy nothing.
    let stage = match durability {
        soopy::Durability::Durable => {
            let mut store = soopy::DurableStageStore::open(state.join("stages"))
                .map_err(|error| format!("open stage store: {error}"))?;
            stage_into(&mut source_root, &request, &mut store)?
        }
        soopy::Durability::DryRun => {
            let mut store = soopy::InMemoryStageStore::new();
            stage_into(&mut source_root, &request, &mut store)?
        }
    };
    let engine = match durability {
        soopy::Durability::Durable => soopy::CommitEngine::open(root, state.join("commits")),
        soopy::Durability::DryRun => soopy::CommitEngine::open_dry_run(root, state.join("commits")),
    }
    .map_err(|error| format!("open commit engine: {error}"))?;
    engine
        .commit(&stage)
        .map_err(|refused| format!("commit refused: {refused}"))?;
    Ok((stage.id.to_string(), stage.previews))
}

/// The identity of the file at `root/rel`, as soopy hashes it.
pub fn content_id(root: &Path, rel: &str) -> Result<soopy::ContentId, String> {
    let path = root.join(rel);
    let bytes = std::fs::read(&path).map_err(|error| format!("read {rel}: {error}"))?;
    Ok(soopy::ContentId::blake3(&bytes))
}

pub fn print_previews(previews: &[soopy::FilePreview]) {
    for preview in previews {
        let before = preview_path(preview.path_before.as_ref());
        let after = preview_path(preview.path_after.as_ref());
        println!(
            "{:<7} {before} -> {after}  {}",
            format!("{:?}", preview.kind).to_lowercase(),
            preview.summary
        );
        if let Some(unified) = preview.unified.as_ref().filter(|text| text.contains("@@")) {
            for line in unified.lines() {
                println!("    {line}");
            }
        }
    }
}

fn preview_path(path: Option<&soopy::SourcePath>) -> String {
    match path {
        Some(soopy::SourcePath::Directory { path }) => path.0.to_string(),
        Some(soopy::SourcePath::Git { path }) => path.0.to_string(),
        None => "-".to_string(),
    }
}

/// A temp root carrying only the files a plan touches. The dry run commits into
/// it, so a dry run walks the same soopy path a real run does.
pub struct Mirror {
    root: PathBuf,
}

impl Mirror {
    pub fn build(source_root: &Path, stages: &[Vec<soopy::SourceAction>]) -> Result<Self, String> {
        let root = std::env::temp_dir().join(format!(
            "extract-move-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&root)
            .map_err(|error| format!("create mirror {}: {error}", root.display()))?;
        let mut copied = BTreeSet::new();
        for stage in stages {
            for action in stage {
                let Some(rel) = source_rel(action) else {
                    continue;
                };
                if !copied.insert(rel.to_string()) {
                    continue;
                }
                let target = root.join(rel);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|error| format!("create {}: {error}", parent.display()))?;
                }
                std::fs::copy(source_root.join(rel), &target)
                    .map_err(|error| format!("mirror {rel}: {error}"))?;
            }
        }
        Ok(Self {
            root: root
                .canonicalize()
                .map_err(|error| format!("canonicalize mirror: {error}"))?,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for Mirror {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
