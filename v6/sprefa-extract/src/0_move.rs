//! `extract move <old> <new>` and `--list <tsv>`: rehome files and repair every
//! reference that named them, through soopy's staged mutation boundary. The
//! `rehomes()` roster answers per language; nothing in this file names one.
//! @comment-ok: module header, the seam list every bin arm opens with

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use clap::Parser;
use sprefa_extract::move_stage::{
    content_id, print_previews, stage_and_commit, state_root, Mirror,
};
use sprefa_extract::{
    directory_path, directory_source, dirname, normalize, rehome_for, rehomes, replace_action,
    MoveCx, Respell,
};

const PRODUCER: &str = "extract-move";

#[derive(Parser)]
#[command(
    name = "extract move",
    about = "move a file and repair every specifier that named it"
)]
struct MoveCli {
    /// The file to rehome. Omitted when `--list` carries the moves.
    old: Option<PathBuf>,
    /// Where it lands.
    new: Option<PathBuf>,
    /// A tsv of `old<TAB>new` rows, one move per line. Blank lines and lines
    /// opening with `#` are skipped; relative paths read against the cwd.
    #[arg(long)]
    list: Option<PathBuf>,
    /// Corpus root. Defaults to the git root containing the first `old`.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Soopy state root. Must sit outside the corpus root.
    #[arg(long)]
    state: Option<PathBuf>,
    /// Apply the plan to the real tree instead of dry running it.
    #[arg(long)]
    commit: bool,
    /// Leave a reexport shim behind at `old` instead of rewriting importers.
    #[arg(long)]
    shim: bool,
    /// Relocate a moved Rust module's `mod` declaration into its new parent
    /// and respell `use` paths, instead of adding `#[path]`.
    #[arg(long = "relocate-mod")]
    relocate_mod: bool,
    /// Report the old-path spellings this move leaves behind in plain text.
    #[arg(long = "text-refs")]
    text_refs: bool,
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
    for (old, new) in &plan.moves {
        println!("plan {old} -> {new}");
    }
    for receipt in &plan.receipts {
        println!("{receipt}");
    }

    // Read off the pre-move file set: once the Moves commit, an emptied
    // directory is indistinguishable from one that was already empty.
    let emptied = emptied_directories(&plan.cx);

    if cli.commit {
        for stage in &plan.stages {
            let (id, previews) =
                stage_and_commit(&plan.root, &state, stage, soopy::Durability::Durable)?;
            print_previews(&previews);
            println!("stage {id} committed");
        }
        for directory in &emptied {
            std::fs::remove_dir(plan.cx.abs(directory))
                .map_err(|error| format!("remove empty directory {directory}: {error}"))?;
            println!("rmdir {directory}");
        }
    } else {
        let mirror = Mirror::build(&plan.root, &plan.stages)?;
        for stage in &plan.stages {
            let (id, previews) =
                stage_and_commit(mirror.root(), &state, stage, soopy::Durability::DryRun)?;
            print_previews(&previews);
            println!("stage {id} dry run, tree untouched");
        }
        for directory in &emptied {
            println!("rmdir {directory} dry run, tree untouched");
        }
    }
    if cli.text_refs {
        crate::move_text::report(&plan.cx);
    }
    Ok(())
}

/// Soopy accepts ONE operation per source file (`_7d_mutation_plan.rs`
/// `insert_non_replace`), so edits, Moves and the shim Create are separate stages.
struct Plan {
    root: PathBuf,
    cx: MoveCx,
    moves: Vec<(String, String)>,
    stages: Vec<Vec<soopy::SourceAction>>,
    receipts: Vec<String>,
}

impl Plan {
    fn build(cli: &MoveCli) -> Result<Self, String> {
        let requested = requested_moves(cli)?;
        let root = plan_root(cli, &requested[0].0)?;
        let cx = MoveCx::open(&root)?;
        let moves = validated_moves(&cx, &root, requested)?;
        if cli.shim && moves.len() > 1 {
            return Err("--shim rehomes one file; drop --list".to_string());
        }
        let cx = cx
            .with_batch(moves.iter().cloned().collect(), cli.shim)
            .with_relocate_mod(cli.relocate_mod);

        let shim_body = match cli.shim {
            true => {
                let (old, new) = &moves[0];
                let arm = rehome_for(old).ok_or_else(|| format!("no rehome arm for {old}"))?;
                Some(
                    arm.shim(&cx, old, new)
                        .ok_or_else(|| format!("--shim: {} has no shim form", arm.name()))?,
                )
            }
            false => None,
        };

        // Every action is planned against the real root; `bind_action` re-aims it
        // at the root that actually stages it.
        let identity = soopy::SourceRoot::open_directory(&root)
            .map_err(|error| format!("open root {}: {error}", root.display()))?
            .directory()
            .identity
            .clone();

        let respells = respells(&cx)?;
        let mut receipts = Vec::new();
        let mut by_file: BTreeMap<String, Vec<soopy::TextEdit>> = BTreeMap::new();
        let producer = soopy::ActionProducer::unordered(PRODUCER);
        for respell in respells {
            if let Some(receipt) = respell.receipt {
                receipts.push(receipt);
            }
            let source = directory_source(&identity, &respell.file);
            let start = respell.span.start as u64;
            by_file
                .entry(respell.file)
                .or_default()
                .push(soopy::TextEdit {
                    range: soopy::ActionSpan {
                        source,
                        start,
                        end: start + respell.span.len as u64,
                    },
                    replacement: respell.text.into_bytes(),
                    producer: producer.clone(),
                });
        }
        let mut edit_stage: Vec<soopy::SourceAction> = Vec::new();
        for (rel, edits) in by_file {
            let source = directory_source(&identity, &rel);
            edit_stage.push(replace_action(source, content_id(&root, &rel)?, edits));
        }

        let mut stages = Vec::new();
        if !edit_stage.is_empty() {
            stages.push(edit_stage);
        }
        let mut move_stage = Vec::with_capacity(moves.len());
        for (old, new) in &moves {
            move_stage.push(soopy::SourceAction::Move {
                source: directory_source(&identity, old),
                expected: content_id(&root, old)?,
                destination: directory_path(new),
            });
        }
        stages.push(move_stage);
        if let Some(body) = shim_body {
            stages.push(vec![soopy::SourceAction::Create {
                path: directory_path(&moves[0].0),
                bytes: body.into_bytes(),
            }]);
        }
        Ok(Plan {
            root,
            cx,
            moves,
            stages,
            receipts,
        })
    }
}

/// Every respell the roster proposes, in (file, offset) order. `(file, span)`
/// names ONE replacement; two arms or two texts on one span is a plan error.
fn respells(cx: &MoveCx) -> Result<Vec<Respell>, String> {
    let mut claimed: BTreeMap<(String, u32), (&'static str, String)> = BTreeMap::new();
    let mut out: Vec<Respell> = Vec::new();
    for arm in rehomes() {
        let mut refs = arm.import_refs(cx);
        refs.extend(arm.manifest_refs(cx));
        for reference in &refs {
            let Some(respell) = arm.respell(cx, reference) else {
                continue;
            };
            let key = (respell.file.clone(), respell.span.start);
            if let Some((other, text)) = claimed.get(&key) {
                if *other == arm.name() && *text == respell.text {
                    continue;
                }
                return Err(format!(
                    "{} byte {} is claimed by both the {other} and the {} rehome arms",
                    respell.file,
                    respell.span.start,
                    arm.name()
                ));
            }
            claimed.insert(key, (arm.name(), respell.text.clone()));
            out.push(respell);
        }
    }
    out.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.span.start.cmp(&right.span.start))
    });
    Ok(out)
}

// ── the emptied-directory sweep ─────────────────────────────────────────────

/// Every directory this run's moves empty, deepest first. soopy's `SourceAction`
/// has no directory arm (`soopy/src/_7b_source_actions.rs:182-201`).
fn emptied_directories(cx: &MoveCx) -> Vec<String> {
    // Ancestors of a moved file, in path order, so a parent is tried before the
    // children it already covers.
    let mut candidates: BTreeSet<String> = BTreeSet::new();
    for old in cx.moved().keys() {
        let mut probe = dirname(old);
        while !probe.is_empty() {
            candidates.insert(probe.to_string());
            probe = dirname(probe);
        }
    }
    let mut removals = Vec::new();
    let mut covered: Vec<String> = Vec::new();
    for directory in &candidates {
        if covered.iter().any(|done| under(directory, done)) {
            continue;
        }
        if !empties(cx, directory) {
            continue;
        }
        covered.push(directory.clone());
        collect_tree(cx, directory, &mut removals);
    }
    removals
}

/// Whether `directory` holds nothing after the moves land. Already empty answers
/// false: this run emptied nothing there, and an empty directory is not tracked.
fn empties(cx: &MoveCx, directory: &str) -> bool {
    if cx.moved().values().any(|new| under(new, directory)) {
        return false;
    }
    let mut held = 0usize;
    for rel in cx.files() {
        if !under(rel, directory) {
            continue;
        }
        held += 1;
        if cx.destination(rel).is_none() {
            return false;
        }
    }
    held > 0
}

/// `directory` and every directory under it holding a file, children before
/// parents and ascending within a level.
fn collect_tree(cx: &MoveCx, directory: &str, out: &mut Vec<String>) {
    let prefix = format!("{directory}/");
    let children: BTreeSet<&str> = cx
        .files()
        .iter()
        .filter_map(|rel| rel.strip_prefix(&prefix))
        .filter_map(|tail| tail.split_once('/'))
        .map(|(head, _)| head)
        .collect();
    for child in children {
        collect_tree(cx, &format!("{directory}/{child}"), out);
    }
    out.push(directory.to_string());
}

/// Whether the root-relative `path` sits under the directory `directory`.
fn under(path: &str, directory: &str) -> bool {
    directory.is_empty() || path.starts_with(&format!("{directory}/"))
}

// ── the requested batch ─────────────────────────────────────────────────────

/// The `(old, new)` pairs the invocation asks for, from the positionals or from
/// the `--list` tsv. The two forms are exclusive.
fn requested_moves(cli: &MoveCli) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    match (&cli.list, &cli.old, &cli.new) {
        (Some(list), None, None) => read_move_list(list),
        (Some(_), _, _) => Err("--list carries the moves; drop <old> and <new>".to_string()),
        (None, Some(old), Some(new)) => Ok(vec![(old.clone(), new.clone())]),
        (None, _, _) => Err("extract move takes <old> <new>, or --list <tsv>".to_string()),
    }
}

/// `old<TAB>new` per line. A row with no tab is an error, never a silent skip:
/// a dropped row is a move that never happens.
fn read_move_list(path: &Path) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("read move list {}: {error}", path.display()))?;
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (old, new) = line.split_once('\t').ok_or_else(|| {
            format!(
                "{}:{number}: a move list row is `old<TAB>new`",
                path.display()
            )
        })?;
        let (old, new) = (old.trim(), new.trim());
        if old.is_empty() || new.is_empty() {
            return Err(format!(
                "{}:{number}: both sides of a move list row are required",
                path.display()
            ));
        }
        rows.push((PathBuf::from(old), PathBuf::from(new)));
    }
    if rows.is_empty() {
        return Err(format!("{} names no moves", path.display()));
    }
    Ok(rows)
}

/// The corpus root: as asked, else the git root holding the first move's source.
fn plan_root(cli: &MoveCli, first: &Path) -> Result<PathBuf, String> {
    let root = match cli.root.as_deref() {
        Some(root) => absolute(root)?,
        None => {
            let old = absolute(first)?;
            if !old.is_file() {
                return Err(format!("move source is not a file: {}", old.display()));
            }
            let old = old
                .canonicalize()
                .map_err(|error| format!("canonicalize {}: {error}", old.display()))?;
            let parent = old.parent().unwrap_or(&old).to_path_buf();
            soopy::discover(&parent)
                .map_err(|error| format!("discover root for {}: {error}", old.display()))?
                .root
        }
    };
    root.canonicalize()
        .map_err(|error| format!("canonicalize root {}: {error}", root.display()))
}

/// Every validation a batch needs, all of it before any stage is built: a
/// missing source, a live destination, or a destination written twice ends the run.
fn validated_moves(
    cx: &MoveCx,
    root: &Path,
    requested: Vec<(PathBuf, PathBuf)>,
) -> Result<Vec<(String, String)>, String> {
    let mut moves: Vec<(String, String)> = Vec::with_capacity(requested.len());
    let mut olds: BTreeSet<String> = BTreeSet::new();
    let mut news: BTreeSet<String> = BTreeSet::new();
    for (old, new) in requested {
        let old = absolute(&old)?;
        if !old.is_file() {
            return Err(format!("move source is not a file: {}", old.display()));
        }
        let old = old
            .canonicalize()
            .map_err(|error| format!("canonicalize {}: {error}", old.display()))?;
        let new = canonical_unborn(&absolute(&new)?);
        if new.exists() {
            return Err(format!(
                "move destination already exists: {}",
                new.display()
            ));
        }
        let old_rel = within_root(root, &old)?;
        let new_rel = within_root(root, &new)?;
        if !cx.contains(&old_rel) {
            return Err(format!("move source is outside the corpus: {old_rel}"));
        }
        let arm = rehome_for(&old_rel).ok_or_else(|| {
            format!(
                "extract move rehomes {}: {old_rel}",
                rehomes()
                    .iter()
                    .map(|arm| arm.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
        let destination = rehome_for(&new_rel)
            .ok_or_else(|| format!("move destination has no known extension: {new_rel}"))?;
        if arm.name() != destination.name() {
            return Err(format!("{old_rel} -> {new_rel} crosses languages"));
        }
        if !olds.insert(old_rel.clone()) {
            return Err(format!("{old_rel} is moved twice"));
        }
        if !news.insert(new_rel.clone()) {
            return Err(format!("{new_rel} is the destination of two moves"));
        }
        moves.push((old_rel, new_rel));
    }
    Ok(moves)
}

// ── paths and roots ─────────────────────────────────────────────────────────

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
