//! The soopy boundary an `extract move` run stages through: the state root, the
//! stage-and-commit pair, and the temp mirror a dry run commits into. Lang-free
//! and plan-free; it takes actions and returns previews.
//! @comment-ok: module header, the seam list every move file opens with

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::drain::{bind_action, directory_path, directory_source, source_rel};

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

/// A hard ceiling on a `--verify` command, per the move-verify brief.
pub const VERIFY_TIMEOUT_SECS: u64 = 300;

/// Run `<cmd>` through `sh -c` in `root`, output inherited. `None` on timeout.
pub fn run_verify_command(root: &Path, command: &str) -> Result<Option<i32>, String> {
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(root)
        .spawn()
        .map_err(|error| format!("spawn verify command: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(VERIFY_TIMEOUT_SECS);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.code()),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(format!("wait verify command: {error}")),
        }
    }
}

/// The pre-run state a `--verify` rollback undoes to: every path the committed
/// move touches, captured before any stage commits. Byte restores go through
/// soopy Replace, never `git checkout` (the root may not be a git tree).
pub struct VerifyJournal {
    moves: Vec<(String, String)>,
    shims: Vec<String>,
    existing: BTreeMap<String, Vec<u8>>,
}

impl VerifyJournal {
    /// Pre-run bytes of every moved file, every shim path, and every file a
    /// respell edit touches. A shim sits at an old path, so it is both a
    /// delete-before-move-back and a byte restore; an edited importer only
    /// needs its bytes back.
    pub fn capture(
        root: &Path,
        moves: &[(String, String)],
        shims: &[String],
        edited: &[String],
    ) -> Result<Self, String> {
        let mut existing = BTreeMap::new();
        for rel in moves
            .iter()
            .flat_map(|(old, _)| [old])
            .chain(shims.iter())
            .chain(edited.iter())
        {
            let path = root.join(rel);
            if !path.is_file() {
                continue;
            }
            let bytes = std::fs::read(&path).map_err(|error| format!("read {rel}: {error}"))?;
            existing.insert(rel.clone(), bytes);
        }
        Ok(Self {
            moves: moves.to_vec(),
            shims: shims.to_vec(),
            existing,
        })
    }

    /// The inverse stage sequence: shims deleted, moves walked back, pre-run
    /// bytes restored over whole files. Swept directories re-created first, so
    /// a move-back has somewhere to land. Returns the count of restored paths.
    pub fn restore(&self, root: &Path, state: &Path, swept: &[String]) -> Result<usize, String> {
        for directory in swept {
            std::fs::create_dir_all(root.join(directory))
                .map_err(|error| format!("re-create {directory}: {error}"))?;
        }
        let identity = soopy::SourceRoot::open_directory(root)
            .map_err(|error| format!("open root {}: {error}", root.display()))?
            .directory()
            .identity
            .clone();
        let mut undo: Vec<Vec<soopy::SourceAction>> = Vec::new();
        let mut deletions: Vec<soopy::SourceAction> = Vec::new();
        for rel in &self.shims {
            if !root.join(rel).is_file() {
                continue;
            }
            deletions.push(soopy::SourceAction::Delete {
                source: directory_source(&identity, rel),
                expected: content_id(root, rel)?,
            });
        }
        if !deletions.is_empty() {
            undo.push(deletions);
        }
        let mut back = Vec::new();
        for (old, new) in &self.moves {
            if !root.join(new).is_file() {
                continue;
            }
            back.push(soopy::SourceAction::Move {
                source: directory_source(&identity, new),
                expected: content_id(root, new)?,
                destination: directory_path(old),
            });
        }
        if !back.is_empty() {
            undo.push(back);
        }
        let producer = soopy::ActionProducer::unordered("extract-move");
        for (rel, pre) in &self.existing {
            let Ok(current) = std::fs::read(root.join(rel)) else {
                continue;
            };
            if current == *pre {
                continue;
            }
            let source = directory_source(&identity, rel);
            undo.push(vec![soopy::SourceAction::Replace {
                source,
                expected: soopy::ContentId::blake3(&current),
                edits: vec![soopy::TextEdit {
                    range: soopy::ActionSpan {
                        source: directory_source(&identity, rel),
                        start: 0,
                        end: current.len() as u64,
                    },
                    replacement: pre.clone(),
                    producer: producer.clone(),
                }],
            }]);
        }
        for stage in &undo {
            stage_and_commit(root, state, stage, soopy::Durability::Durable)?;
        }
        Ok(self.existing.len())
    }
}

/// The identity of the file at `root/rel`, as soopy hashes it.
pub fn content_id(root: &Path, rel: &str) -> Result<soopy::ContentId, String> {
    let path = root.join(rel);
    let bytes = std::fs::read(&path).map_err(|error| format!("read {rel}: {error}"))?;
    Ok(soopy::ContentId::blake3(&bytes))
}

pub fn print_previews(previews: &[soopy::FilePreview], prefix: &str) {
    for preview in previews {
        let before = preview_path(preview.path_before.as_ref());
        let after = preview_path(preview.path_after.as_ref());
        println!(
            "{prefix}{:<7} {before} -> {after}  {}",
            format!("{:?}", preview.kind).to_lowercase(),
            preview.summary
        );
        if let Some(unified) = preview.unified.as_ref().filter(|text| text.contains("@@")) {
            for line in unified.lines() {
                println!("{prefix}    {line}");
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
