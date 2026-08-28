//! `extract rename <FILE>#<OLD> <NEW>` and `--list <tsv>`: rename a symbol and
//! respell every occurrence the language's scope plane binds, through soopy's
//! staged mutation boundary. The `renames()` roster answers per language;
//! nothing in this file names one.
//! @comment-ok: module header, the seam list every bin arm opens with

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clap::Parser;
use sprefa_extract::move_stage::{
    content_id, print_previews, stage_and_commit, state_root, Mirror,
};
use sprefa_extract::{
    directory_source, normalize, rename_for, renames, replace_action, RenameCx, RenameRequest,
    Respell, SymbolRef,
};

const PRODUCER: &str = "extract-rename";

#[derive(Parser)]
#[command(
    name = "extract rename",
    about = "rename a symbol and respell every occurrence bound to it"
)]
struct RenameCli {
    /// `<FILE>#<OLD>`: the declaring file and the identifier as written today.
    /// Omitted when `--list` carries the renames.
    target: Option<String>,
    /// What the identifier becomes.
    new: Option<String>,
    /// A tsv of `anchor<TAB>old<TAB>new` rows, one rename per line. Blank lines
    /// and lines opening with `#` are skipped.
    #[arg(long)]
    list: Option<PathBuf>,
    /// Corpus root. Defaults to the git root holding the first anchor.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Soopy state root. Must sit outside the corpus root.
    #[arg(long)]
    state: Option<PathBuf>,
    /// Apply the plan to the real tree instead of dry running it.
    #[arg(long)]
    commit: bool,
}

pub fn run<I>(args: I) -> Result<(), String>
where
    I: IntoIterator,
    I::Item: Into<std::ffi::OsString> + Clone,
{
    let cli = RenameCli::try_parse_from(args).map_err(|error| error.to_string())?;
    let plan = Plan::build(&cli)?;
    let state = state_root(cli.state.as_deref())?;

    println!("root {}", plan.root.display());
    for (request, refs) in plan.cx.batch().iter().zip(&plan.refs) {
        println!("plan {} {} -> {}", request.anchor, request.old, request.new);
        for (file, uses) in uses_per_file(refs) {
            println!("  {file}  {uses} uses");
        }
    }
    for receipt in &plan.receipts {
        println!("{receipt}");
    }

    match cli.commit {
        true => {
            for stage in &plan.stages {
                let (id, previews) =
                    stage_and_commit(&plan.root, &state, stage, soopy::Durability::Durable)?;
                print_previews(&previews, "");
                println!("stage {id} committed");
            }
        }
        false => {
            let mirror = Mirror::build(&plan.root, &plan.stages)?;
            for stage in &plan.stages {
                let (id, previews) =
                    stage_and_commit(mirror.root(), &state, stage, soopy::Durability::DryRun)?;
                print_previews(&previews, "");
                println!("stage {id} dry run, tree untouched");
            }
        }
    }
    Ok(())
}

/// A rename moves no file, so one stage of `Replace` actions covers the whole
/// plan; soopy takes ONE operation per source file.
struct Plan {
    root: PathBuf,
    cx: RenameCx,
    /// One occurrence list per `cx.batch()` row, in batch order.
    refs: Vec<Vec<SymbolRef>>,
    stages: Vec<Vec<soopy::SourceAction>>,
    receipts: Vec<String>,
}

impl Plan {
    fn build(cli: &RenameCli) -> Result<Self, String> {
        let requested = requested_renames(cli)?;
        let root = plan_root(cli.root.as_ref(), &requested[0].0)?;
        let batch = validated_batch(&root, requested)?;
        let cx = RenameCx::open(&root)?.with_batch(batch);

        let mut refs: Vec<Vec<SymbolRef>> = Vec::with_capacity(cx.batch().len());
        for request in cx.batch() {
            let arm = rename_for(&request.anchor).ok_or_else(|| {
                format!(
                    "no rename arm for {} (extract rename renames {})",
                    request.anchor,
                    renames()
                        .iter()
                        .map(|arm| arm.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;
            let found = arm
                .symbol_refs(&cx, request)
                .map_err(|stop| stop.to_string())?;
            verify_spans(&cx, &found)?;
            refs.push(found);
        }

        let mut receipts = Vec::new();
        let respells = respells(&cx, &refs, &mut receipts)?;
        let identity = soopy::SourceRoot::open_directory(&root)
            .map_err(|error| format!("open root {}: {error}", root.display()))?
            .directory()
            .identity
            .clone();
        let producer = soopy::ActionProducer::unordered(PRODUCER);
        let mut by_file: BTreeMap<String, Vec<soopy::TextEdit>> = BTreeMap::new();
        for respell in respells {
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
        let stages = match edit_stage.is_empty() {
            true => Vec::new(),
            false => vec![edit_stage],
        };
        Ok(Plan {
            root,
            cx,
            refs,
            stages,
            receipts,
        })
    }
}

/// Occurrence counts per file for one symbol, in path order.
fn uses_per_file(refs: &[SymbolRef]) -> Vec<(&str, usize)> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for reference in refs {
        *counts.entry(reference.file.as_str()).or_default() += 1;
    }
    counts.into_iter().collect()
}

/// Every span an arm emitted still holds the old name on disk. A move checks a
/// whole file's content id; a rename writes interior spans, so each is checked.
fn verify_spans(cx: &RenameCx, refs: &[SymbolRef]) -> Result<(), String> {
    for reference in refs {
        let text = cx
            .text(&reference.file)
            .ok_or_else(|| format!("read {}", reference.file))?;
        let start = reference.span.start as usize;
        let end = reference.span.end() as usize;
        let found = text
            .get(start..end)
            .ok_or_else(|| format!("{} byte {start}..{end} is outside the file", reference.file))?;
        if found != reference.text {
            return Err(format!(
                "{} byte {start} holds {found:?}, the plan expected {:?}",
                reference.file, reference.text
            ));
        }
    }
    Ok(())
}

/// Every respell the roster proposes, in (file, offset) order. `(file, offset)`
/// names ONE replacement; two arms or two texts on one span is a plan error.
fn respells(
    cx: &RenameCx,
    refs: &[Vec<SymbolRef>],
    receipts: &mut Vec<String>,
) -> Result<Vec<Respell>, String> {
    let mut claimed: BTreeMap<(String, u32), (&'static str, String)> = BTreeMap::new();
    let mut out: Vec<Respell> = Vec::new();
    for (request, found) in cx.batch().iter().zip(refs) {
        for reference in found {
            let Some(arm) = rename_for(&reference.file) else {
                continue;
            };
            let Some(respell) = arm.respell_symbol(cx, request, reference) else {
                continue;
            };
            let key = (respell.file.clone(), respell.span.start);
            if let Some((other, text)) = claimed.get(&key) {
                if *other == arm.name() && *text == respell.text {
                    continue;
                }
                return Err(format!(
                    "{} byte {} is claimed by both the {other} and the {} rename arms",
                    respell.file,
                    respell.span.start,
                    arm.name()
                ));
            }
            claimed.insert(key, (arm.name(), respell.text.clone()));
            if let Some(receipt) = respell.receipt.clone() {
                receipts.push(receipt);
            }
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

// ── the requested batch ─────────────────────────────────────────────────────

/// The `(anchor, old, new)` rows the invocation asks for, from the positionals
/// or from the `--list` tsv. The two forms are exclusive.
fn requested_renames(cli: &RenameCli) -> Result<Vec<(PathBuf, String, String)>, String> {
    match (&cli.list, &cli.target, &cli.new) {
        (Some(list), None, None) => read_rename_list(list),
        (Some(_), _, _) => {
            Err("--list carries the renames; drop <FILE>#<OLD> and <NEW>".to_string())
        }
        (None, Some(target), Some(new)) => {
            let (anchor, old) = target
                .rsplit_once('#')
                .ok_or_else(|| format!("a rename target is `<FILE>#<OLD>`, not {target}"))?;
            if anchor.is_empty() || old.is_empty() {
                return Err(format!("a rename target is `<FILE>#<OLD>`, not {target}"));
            }
            Ok(vec![(PathBuf::from(anchor), old.to_string(), new.clone())])
        }
        (None, _, _) => Err("extract rename takes <FILE>#<OLD> <NEW>, or --list <tsv>".to_string()),
    }
}

/// `anchor<TAB>old<TAB>new` per line. A short row is an error, never a silent
/// skip: a dropped row is a rename that never happens.
fn read_rename_list(path: &Path) -> Result<Vec<(PathBuf, String, String)>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("read rename list {}: {error}", path.display()))?;
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').map(str::trim).collect();
        let [anchor, old, new] = fields.as_slice() else {
            return Err(format!(
                "{}:{number}: a rename list row is `anchor<TAB>old<TAB>new`",
                path.display()
            ));
        };
        if anchor.is_empty() || old.is_empty() || new.is_empty() {
            return Err(format!(
                "{}:{number}: all three fields of a rename list row are required",
                path.display()
            ));
        }
        rows.push((PathBuf::from(anchor), old.to_string(), new.to_string()));
    }
    if rows.is_empty() {
        return Err(format!("{} names no renames", path.display()));
    }
    Ok(rows)
}

/// The corpus root: as asked, else the git root holding the first anchor.
fn plan_root(requested: Option<&PathBuf>, first: &Path) -> Result<PathBuf, String> {
    let root = match requested {
        Some(root) => absolute(root)?,
        None => {
            let anchor = anchor_file(first)?;
            let parent = anchor.parent().unwrap_or(&anchor).to_path_buf();
            soopy::discover(&parent)
                .map_err(|error| format!("discover root for {}: {error}", anchor.display()))?
                .root
        }
    };
    root.canonicalize()
        .map_err(|error| format!("canonicalize root {}: {error}", root.display()))
}

/// Every validation a batch needs, all of it before any arm is asked anything:
/// a missing anchor, an anchor outside the corpus, or a repeated `(anchor, old)`.
fn validated_batch(
    root: &Path,
    requested: Vec<(PathBuf, String, String)>,
) -> Result<Vec<RenameRequest>, String> {
    let mut batch: Vec<RenameRequest> = Vec::with_capacity(requested.len());
    let mut seen: BTreeMap<(String, String), ()> = BTreeMap::new();
    for (anchor, old, new) in requested {
        let anchor = match anchor.is_absolute() {
            true => anchor_file(&anchor)?,
            false => anchor_file(&root.join(&anchor))?,
        };
        let anchor = within_root(root, &anchor)?;
        if old == new {
            return Err(format!("{anchor}: {old} renames to itself"));
        }
        if seen.insert((anchor.clone(), old.clone()), ()).is_some() {
            return Err(format!("{anchor}: {old} is renamed twice"));
        }
        batch.push(RenameRequest {
            anchor,
            old,
            new,
            at: None,
        });
    }
    Ok(batch)
}

fn anchor_file(path: &Path) -> Result<PathBuf, String> {
    let path = absolute(path)?;
    if !path.is_file() {
        return Err(format!("rename anchor is not a file: {}", path.display()));
    }
    path.canonicalize()
        .map_err(|error| format!("canonicalize {}: {error}", path.display()))
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
