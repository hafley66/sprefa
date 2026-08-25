//! `extract move <old.pl> <new.pl>`: rehome one prolog file and repair every
//! specifier that named it, through soopy's staged mutation boundary.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use clap::Parser;
use sprefa_extract::{FamilyMask, PrologSource, Source, SpecifierKind};

const PRODUCER: &str = "extract-move";

#[derive(Parser)]
#[command(
    name = "extract move",
    about = "move one prolog file and repair every specifier that named it"
)]
struct MoveCli {
    /// The file to rehome.
    old: PathBuf,
    /// Where it lands.
    new: PathBuf,
    /// Corpus root. Defaults to the git root containing `old`.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Soopy state root. Must sit outside the corpus root.
    #[arg(long)]
    state: Option<PathBuf>,
    /// Apply the plan to the real tree instead of rehearsing it.
    #[arg(long)]
    commit: bool,
    /// Leave a reexport shim behind at `old` instead of rewriting importers.
    #[arg(long)]
    shim: bool,
}

pub fn run<I>(args: I) -> Result<(), String>
where
    I: IntoIterator,
    I::Item: Into<std::ffi::OsString> + Clone,
{
    let cli = MoveCli::try_parse_from(args).map_err(|error| error.to_string())?;
    let plan = Plan::build(&cli)?;
    let state = state_root(cli.state.as_deref())?;

    println!("root {}", plan.root.display());
    println!("plan {} -> {}", plan.old_rel, plan.new_rel);

    if cli.commit {
        for stage in &plan.stages {
            let (id, previews) = stage_and_commit(&plan.root, &state, stage)?;
            print_previews(&previews);
            println!("stage {id} committed");
        }
    } else {
        let mirror = Mirror::build(&plan)?;
        for stage in &plan.stages {
            let (id, previews) = stage_and_commit(mirror.root(), &state, stage)?;
            print_previews(&previews);
            println!("stage {id} rehearsed (dry run, tree untouched)");
        }
    }
    Ok(())
}

/// Soopy accepts ONE operation per source file (`_7d_mutation_plan.rs`
/// `insert_non_replace`), so edits, the Move, and the shim Create are separate stages.
enum Act {
    Replace {
        rel: String,
        edits: Vec<(usize, usize, String)>,
    },
    Move {
        from: String,
        to: String,
    },
    Create {
        rel: String,
        bytes: Vec<u8>,
    },
}

impl Act {
    fn source_rel(&self) -> Option<&str> {
        match self {
            Act::Replace { rel, .. } => Some(rel),
            Act::Move { from, .. } => Some(from),
            Act::Create { .. } => None,
        }
    }
}

struct Plan {
    root: PathBuf,
    old_rel: String,
    new_rel: String,
    stages: Vec<Vec<Act>>,
}

impl Plan {
    fn build(cli: &MoveCli) -> Result<Self, String> {
        let old = absolute(&cli.old)?;
        if !old.is_file() {
            return Err(format!("move source is not a file: {}", old.display()));
        }
        let old = old
            .canonicalize()
            .map_err(|error| format!("canonicalize {}: {error}", old.display()))?;
        let root = match cli.root.as_deref() {
            Some(root) => absolute(root)?,
            None => {
                let parent = old.parent().unwrap_or(&old);
                soopy::discover(parent)
                    .map_err(|error| format!("discover root for {}: {error}", old.display()))?
                    .root
            }
        };
        let root = root
            .canonicalize()
            .map_err(|error| format!("canonicalize root {}: {error}", root.display()))?;
        let new = canonical_unborn(&absolute(&cli.new)?);
        if new.exists() {
            return Err(format!("move destination already exists: {}", new.display()));
        }
        let old_rel = within_root(&root, &old)?;
        let new_rel = within_root(&root, &new)?;
        let new_dir = new.parent().unwrap_or(&root).to_path_buf();
        let old_dir = old.parent().unwrap_or(&root).to_path_buf();

        let corpus = prolog_files(&root);
        let mut edits: BTreeMap<String, Vec<(usize, usize, String)>> = BTreeMap::new();
        let mut module_name: Option<String> = None;

        let mut parsed = 0usize;
        let mut skipped = 0usize;

        for file in &corpus {
            let Ok(bytes) = std::fs::read(file) else {
                continue;
            };
            let is_old = *file == old;
            // `old` is parsed unconditionally: its own module name is read off it.
            if !is_old && !carries_specifier(&bytes) {
                skipped += 1;
                continue;
            }
            let Ok(text) = String::from_utf8(bytes) else {
                continue;
            };
            parsed += 1;
            let rows = specifiers(file, &text);
            let dir = file.parent().unwrap_or(&root).to_path_buf();
            if is_old {
                module_name = rows.module.clone();
            }
            for spec in rows.paths {
                let Some(target) = resolve(&dir, &spec.raw) else {
                    continue;
                };
                // Inside the moved file every relative spec is re-aimed from the
                // destination dir; elsewhere only the ones naming the moved file.
                let (from_dir, aimed) = if is_old {
                    if target == old {
                        continue;
                    }
                    (new_dir.as_path(), target)
                } else if target == old {
                    (dir.as_path(), new.clone())
                } else {
                    continue;
                };
                let replacement = spec_text(from_dir, &aimed, &spec.raw);
                if replacement == spec.raw {
                    continue;
                }
                let rel = within_root(&root, file)?;
                edits
                    .entry(rel)
                    .or_default()
                    .push((spec.start, spec.end, replacement));
            }
        }
        tracing::debug!(parsed, skipped, corpus = corpus.len(), "move prescan");
        if cli.shim {
            edits.retain(|rel, _| rel == &old_rel);
        }

        let mut edit_stage = Vec::new();
        for (rel, mut spans) in edits {
            spans.sort_by_key(|(start, end, _)| (*start, *end));
            spans.dedup();
            edit_stage.push(Act::Replace { rel, edits: spans });
        }

        let mut stages = Vec::new();
        if !edit_stage.is_empty() {
            stages.push(edit_stage);
        }
        stages.push(vec![Act::Move {
            from: old_rel.clone(),
            to: new_rel.clone(),
        }]);
        if cli.shim {
            let module = module_name.unwrap_or_else(|| stem(&old));
            let target = spec_text(&old_dir, &new, "''");
            let body = format!(":- module({module}_shim, []).\n:- reexport({target}).\n");
            stages.push(vec![Act::Create {
                rel: old_rel.clone(),
                bytes: body.into_bytes(),
            }]);
        }
        Ok(Plan {
            root,
            old_rel,
            new_rel,
            stages,
        })
    }
}

/// The directive names that can carry a file spec, matching the extractor's own
/// arm list (`lang/prolog/_0_source.rs:379-383`). A file naming none of them
/// yields no specifier row, so its parse buys nothing. Bare words, not
/// `include(`: a quoted-atom call (`'include'(...)`) still has to match, and a
/// missed rewrite is a silently broken import. Measured cost of the wider net on
/// the repo corpus is 10 files out of 284.
const SPEC_NEEDLES: [&str; 5] = [
    "use_module",
    "ensure_loaded",
    "consult",
    "include",
    "reexport",
];

fn carries_specifier(bytes: &[u8]) -> bool {
    SPEC_NEEDLES
        .iter()
        .any(|needle| memchr::memmem::find(bytes, needle.as_bytes()).is_some())
}

/// One file-spec occurrence, with the byte range of the term as written.
struct PathSpec {
    raw: String,
    start: usize,
    end: usize,
}

struct SpecRows {
    paths: Vec<PathSpec>,
    module: Option<String>,
}

/// `use_module(Path, [f/1])` spans the indicator and carries the path on
/// `module`, so that range is recovered by scanning back for the last whole token.
fn specifiers(path: &Path, text: &str) -> SpecRows {
    let display = path.display().to_string();
    // Only the call family carries specifier rows; the cst, type and df
    // projections cost a third of the corpus wall and feed nothing here.
    let mask = FamilyMask { call: true, ..FamilyMask::NONE };
    let output = PrologSource.extract(&display, text.as_bytes(), mask);
    let mut paths: Vec<PathSpec> = Vec::new();
    let mut module = None;
    let Some(call) = output.call.as_ref() else {
        return SpecRows { paths, module };
    };
    for spec in &call.aux.specifiers {
        let start = spec.span.start as usize;
        let end = start + spec.span.len as usize;
        match (spec.kind, spec.module) {
            (SpecifierKind::Reexport, Some(name)) => {
                module.get_or_insert_with(|| output.strings.lookup(name).to_string());
            }
            (SpecifierKind::SideEffect | SpecifierKind::Include, _)
            | (SpecifierKind::ReexportModule, None) => {
                if end <= text.len() {
                    paths.push(PathSpec {
                        raw: output.strings.lookup(spec.name).to_string(),
                        start,
                        end,
                    });
                }
            }
            (SpecifierKind::Named | SpecifierKind::ReexportModule, Some(name)) => {
                let raw = output.strings.lookup(name).to_string();
                if let Some((start, end)) = last_token_before(text, &raw, start) {
                    paths.push(PathSpec { raw, start, end });
                }
            }
            _ => {}
        }
    }
    paths.sort_by_key(|spec| (spec.start, spec.end));
    paths.dedup_by(|left, right| left.start == right.start && left.end == right.end);
    SpecRows { paths, module }
}

fn last_token_before(text: &str, needle: &str, limit: usize) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }
    let window = text.get(..limit.min(text.len()))?;
    let mut found = None;
    let mut from = 0usize;
    while let Some(offset) = window[from..].find(needle) {
        let start = from + offset;
        let end = start + needle.len();
        if !is_word_byte(window.as_bytes(), start.checked_sub(1))
            && !is_word_byte(window.as_bytes(), Some(end))
        {
            found = Some((start, end));
        }
        from = start + 1;
    }
    found
}

fn is_word_byte(bytes: &[u8], index: Option<usize>) -> bool {
    match index {
        Some(index) => bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_'),
        None => false,
    }
}

fn prolog_files(root: &Path) -> Vec<PathBuf> {
    const SKIP: [&str; 4] = [".git", "target", "node_modules", ".boop-worktrees"];
    let mut queue = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = queue.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().to_string();
            if kind.is_dir() {
                if !SKIP.contains(&name.as_str()) {
                    queue.push(path);
                }
            } else if kind.is_file() && (name.ends_with(".pl") || name.ends_with(".plt")) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// A prolog file spec resolves against the loading file's directory and takes
/// `.pl` when bare; `library(...)` and every other alias term names no file here.
fn resolve(dir: &Path, raw: &str) -> Option<PathBuf> {
    let (_, bare) = unquote(raw);
    if bare.is_empty() || bare.contains('(') || Path::new(bare).is_absolute() {
        return None;
    }
    let joined = normalize(&dir.join(bare));
    if joined.is_file() {
        return Some(joined);
    }
    let with_extension = PathBuf::from(format!("{}.pl", joined.display()));
    with_extension.is_file().then_some(with_extension)
}

fn spec_text(from_dir: &Path, target: &Path, original: &str) -> String {
    let (quote, bare) = unquote(original);
    let relative = relative_from(from_dir, target);
    let trimmed = if bare.ends_with(".pl") {
        relative
    } else {
        relative
            .strip_suffix(".pl")
            .map(str::to_string)
            .unwrap_or(relative)
    };
    requote(quote, &trimmed)
}

fn unquote(raw: &str) -> (Option<char>, &str) {
    let bytes = raw.as_bytes();
    let quoted = bytes.len() >= 2
        && (bytes[0] == b'\'' || bytes[0] == b'"')
        && bytes[bytes.len() - 1] == bytes[0];
    if quoted {
        return (Some(bytes[0] as char), &raw[1..raw.len() - 1]);
    }
    (None, raw)
}

/// An unquoted spec stays unquoted only while it is still a plain atom; a
/// relative path reads as a compound term, so it takes quotes.
fn requote(quote: Option<char>, text: &str) -> String {
    match quote {
        Some(quote) => format!("{quote}{text}{quote}"),
        None if is_plain_atom(text) => text.to_string(),
        None => format!("'{text}'"),
    }
}

fn is_plain_atom(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn relative_from(from_dir: &Path, target: &Path) -> String {
    let from: Vec<_> = from_dir.components().collect();
    let to: Vec<_> = target.components().collect();
    let mut shared = 0;
    while shared < from.len() && shared < to.len() && from[shared] == to[shared] {
        shared += 1;
    }
    let mut parts: Vec<String> = vec!["..".to_string(); from.len() - shared];
    parts.extend(
        to[shared..]
            .iter()
            .map(|part| part.as_os_str().to_string_lossy().to_string()),
    );
    parts.join("/")
}

fn normalize(path: &Path) -> PathBuf {
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

/// The destination does not exist yet, so only its deepest existing ancestor can
/// be canonicalized; the tail is re-appended so root-relative stripping still holds.
fn canonical_unborn(path: &Path) -> PathBuf {
    let path = normalize(path);
    let mut tail = Vec::new();
    let mut probe = path.as_path();
    loop {
        if let Ok(real) = probe.canonicalize() {
            let mut out = real;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return out;
        }
        let Some(parent) = probe.parent() else {
            return path;
        };
        let Some(name) = probe.file_name() else {
            return path;
        };
        tail.push(name.to_os_string());
        probe = parent;
    }
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(normalize(path));
    }
    let cwd = std::env::current_dir().map_err(|error| format!("current directory: {error}"))?;
    Ok(normalize(&cwd.join(path)))
}

fn within_root(root: &Path, path: &Path) -> Result<String, String> {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .map_err(|_| format!("{} is outside root {}", path.display(), root.display()))
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn state_root(requested: Option<&Path>) -> Result<PathBuf, String> {
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

fn stage_and_commit(
    root: &Path,
    state: &Path,
    acts: &[Act],
) -> Result<(String, Vec<soopy::FilePreview>), String> {
    let mut source_root = soopy::SourceRoot::open_directory(root)
        .map_err(|error| format!("open root {}: {error}", root.display()))?;
    let identity = source_root.directory().identity.clone();
    let root_id = soopy::SourceRootId::Directory {
        directory: identity.clone(),
    };
    let mut actions = Vec::with_capacity(acts.len());
    for act in acts {
        actions.push(action(root, &identity, act)?);
    }
    let request = soopy::StageRequest::new(root_id, actions);
    let mut store = soopy::DurableStageStore::open(state.join("stages"))
        .map_err(|error| format!("open stage store: {error}"))?;
    let sealed = soopy::stage_mutations(&mut source_root, &request, &mut store)
        .map_err(|refusal| format!("stage refused: {refusal}"))?;
    // `save` returns the manifest; only `load` rehydrates the blobs commit writes.
    let stage = soopy::show_stage(&store, sealed.id)
        .map_err(|error| format!("load stage {}: {error}", sealed.id))?
        .ok_or_else(|| format!("stage {} vanished from the store", sealed.id))?;
    let engine = soopy::CommitEngine::open(root, state.join("commits"))
        .map_err(|error| format!("open commit engine: {error}"))?;
    engine
        .commit(&stage)
        .map_err(|refusal| format!("commit refused: {refusal}"))?;
    Ok((stage.id.to_string(), stage.previews))
}

fn action(
    root: &Path,
    identity: &soopy::DirectoryId,
    act: &Act,
) -> Result<soopy::SourceAction, String> {
    let path = |rel: &str| soopy::SourcePath::Directory {
        path: soopy::RootPath(Arc::from(rel)),
    };
    let source = |rel: &str| soopy::ActionSource::Directory {
        file: soopy::FileRef {
            directory: identity.clone(),
            path: soopy::RootPath(Arc::from(rel)),
        },
    };
    let expected = |rel: &str| -> Result<soopy::ContentId, String> {
        let bytes = std::fs::read(root.join(rel))
            .map_err(|error| format!("read {}: {error}", root.join(rel).display()))?;
        Ok(soopy::ContentId::blake3(&bytes))
    };
    Ok(match act {
        Act::Create { rel, bytes } => soopy::SourceAction::Create {
            path: path(rel),
            bytes: bytes.clone(),
        },
        Act::Move { from, to } => soopy::SourceAction::Move {
            source: source(from),
            expected: expected(from)?,
            destination: path(to),
        },
        Act::Replace { rel, edits } => {
            let source = source(rel);
            soopy::SourceAction::Replace {
                source: source.clone(),
                expected: expected(rel)?,
                edits: edits
                    .iter()
                    .map(|(start, end, replacement)| soopy::TextEdit {
                        range: soopy::ActionSpan {
                            source: source.clone(),
                            start: *start as u64,
                            end: *end as u64,
                        },
                        replacement: replacement.clone().into_bytes(),
                        producer: soopy::ActionProducer::unordered(PRODUCER),
                    })
                    .collect(),
            }
        }
    })
}

fn print_previews(previews: &[soopy::FilePreview]) {
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

/// A temp root carrying only the files the plan touches. The rehearsal commits
/// into it, so a dry run walks the same soopy path a real run does.
struct Mirror {
    root: PathBuf,
}

impl Mirror {
    fn build(plan: &Plan) -> Result<Self, String> {
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
        for stage in &plan.stages {
            for act in stage {
                let Some(rel) = act.source_rel() else {
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
                std::fs::copy(plan.root.join(rel), &target)
                    .map_err(|error| format!("mirror {rel}: {error}"))?;
            }
        }
        Ok(Self {
            root: root
                .canonicalize()
                .map_err(|error| format!("canonicalize mirror: {error}"))?,
        })
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for Mirror {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
