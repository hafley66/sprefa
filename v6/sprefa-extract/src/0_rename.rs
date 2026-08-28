//! `extract rename <FILE>#<OLD> <NEW>` and `--list <tsv>`: rename a symbol and
//! respell every occurrence the language's scope plane binds, through soopy's
//! staged mutation boundary. The `renames()` roster answers per language;
//! nothing in this file names one.
//! @comment-ok: module header, the seam list every bin arm opens with

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use clap::Parser;
use sprefa_extract::move_stage::{
    content_id, print_previews, stage_and_commit, state_root, Mirror,
};
use sprefa_extract::{
    directory_source, normalize, rename_for, renames, replace_action, RenameCx, RenameRequest,
    RenameStop, Respell, SymbolRef,
};

#[path = "1_rename_verify.rs"]
mod rename_verify;

const PRODUCER: &str = "extract-rename";

/// A failed rename run: the message and the process exit code. One code per
/// stop, all distinct from 2 for every plan error:
/// 2 plan error (usage, no arm, verify, claim conflict) · 3 `Ambiguous`, pass
/// `--at` · 4 `NotFound` · 5 `Inexact` · 6 `Dynamic`.
pub struct RenameError {
    pub message: String,
    pub exit: i32,
}

impl fmt::Display for RenameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

fn plan_error(message: String) -> RenameError {
    RenameError { message, exit: 2 }
}

fn stop_error(stop: RenameStop) -> RenameError {
    let exit = match &stop {
        RenameStop::Ambiguous { .. } => 3,
        RenameStop::NotFound { .. } => 4,
        RenameStop::Inexact { .. } => 5,
        RenameStop::Dynamic(..) => 6,
    };
    RenameError {
        message: stop.to_string(),
        exit,
    }
}

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
    /// Byte offset inside the declaration, when the anchor declares `<OLD>`
    /// more than once. One rename only; never combined with `--list`.
    #[arg(long)]
    at: Option<u32>,
    /// Apply the plan to the real tree instead of dry running it.
    #[arg(long)]
    commit: bool,
    /// Report the old-name spellings this rename leaves behind in plain text.
    #[arg(long = "text-refs")]
    text_refs: bool,
    /// Cross-check the plan against a prebuilt SCIP index. Reports only: the
    /// count never changes the plan, the stages, or the exit code.
    #[arg(long = "verify-scip", value_name = "INDEX")]
    verify_scip: Option<PathBuf>,
}

pub fn run<I>(args: I) -> Result<(), RenameError>
where
    I: IntoIterator,
    I::Item: Into<std::ffi::OsString> + Clone,
{
    let cli = RenameCli::try_parse_from(args).map_err(|error| plan_error(error.to_string()))?;
    let plan = Plan::build(&cli)?;
    let state = state_root(cli.state.as_deref()).map_err(plan_error)?;

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
    if let Some(index) = cli.verify_scip.as_deref() {
        let disagreements =
            rename_verify::verify_plan(&plan.cx, &plan.refs, index).map_err(plan_error)?;
        rename_verify::report(&disagreements);
    }

    match cli.commit {
        true => {
            for stage in &plan.stages {
                let (id, previews) =
                    stage_and_commit(&plan.root, &state, stage, soopy::Durability::Durable)
                        .map_err(plan_error)?;
                print_previews(&previews, "");
                println!("stage {id} committed");
            }
        }
        false => {
            let mirror = Mirror::build(&plan.root, &plan.stages).map_err(plan_error)?;
            for stage in &plan.stages {
                let (id, previews) =
                    stage_and_commit(mirror.root(), &state, stage, soopy::Durability::DryRun)
                        .map_err(plan_error)?;
                print_previews(&previews, "");
                println!("stage {id} dry run, tree untouched");
            }
        }
    }
    if cli.text_refs {
        for request in plan.cx.batch() {
            crate::move_text::report_rename(&plan.cx, request, &plan.rewritten);
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
    /// Every (file, line) a staged edit rewrites; the text-refs scan leaves
    /// those lines alone.
    rewritten: BTreeSet<(String, usize)>,
    receipts: Vec<String>,
}

impl Plan {
    fn build(cli: &RenameCli) -> Result<Self, RenameError> {
        let requested = requested_renames(cli)?;
        let root = plan_root(cli.root.as_ref(), &requested[0].0)?;
        let batch = validated_batch(&root, requested, cli.at)?;
        let cx = RenameCx::open(&root).map_err(plan_error)?.with_batch(batch);

        let mut refs: Vec<Vec<SymbolRef>> = Vec::with_capacity(cx.batch().len());
        for request in cx.batch() {
            let arm = rename_for(&request.anchor).ok_or_else(|| {
                plan_error(format!(
                    "no rename arm for {} (extract rename renames {})",
                    request.anchor,
                    renames()
                        .iter()
                        .map(|arm| arm.name())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })?;
            let found = arm.symbol_refs(&cx, request).map_err(stop_error)?;
            verify_spans(&cx, &found)?;
            refs.push(found);
        }

        let mut receipts = Vec::new();
        let respells = respells(&cx, &refs, &mut receipts)?;
        let rewritten = rewritten_lines(&cx, &respells)?;
        let identity = soopy::SourceRoot::open_directory(&root)
            .map_err(|error| plan_error(format!("open root {}: {error}", root.display())))?
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
            edit_stage.push(replace_action(
                source,
                content_id(&root, &rel).map_err(plan_error)?,
                edits,
            ));
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
            rewritten,
            receipts,
        })
    }
}

/// Every (file, line) one respell rewrites. A respell never carries a newline,
/// so the line numbers hold across the staged write and the post-commit scan.
fn rewritten_lines(
    cx: &RenameCx,
    respells: &[Respell],
) -> Result<BTreeSet<(String, usize)>, RenameError> {
    let mut lines = BTreeSet::new();
    for respell in respells {
        let text = cx
            .text(&respell.file)
            .ok_or_else(|| plan_error(format!("read {}", respell.file)))?;
        let prefix = text.get(..respell.span.start as usize).ok_or_else(|| {
            plan_error(format!(
                "{} byte {} is outside the file",
                respell.file, respell.span.start
            ))
        })?;
        let line = 1 + prefix.bytes().filter(|byte| *byte == b'\n').count();
        lines.insert((respell.file.clone(), line));
    }
    Ok(lines)
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
fn verify_spans(cx: &RenameCx, refs: &[SymbolRef]) -> Result<(), RenameError> {
    for reference in refs {
        let text = cx
            .text(&reference.file)
            .ok_or_else(|| plan_error(format!("read {}", reference.file)))?;
        let start = reference.span.start as usize;
        let end = reference.span.end() as usize;
        let found = text.get(start..end).ok_or_else(|| {
            plan_error(format!(
                "{} byte {start}..{end} is outside the file",
                reference.file
            ))
        })?;
        if found != reference.text {
            return Err(plan_error(format!(
                "{} byte {start} holds {found:?}, the plan expected {:?}",
                reference.file, reference.text
            )));
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
) -> Result<Vec<Respell>, RenameError> {
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
                return Err(plan_error(format!(
                    "{} byte {} is claimed by both the {other} and the {} rename arms",
                    respell.file,
                    respell.span.start,
                    arm.name()
                )));
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
fn requested_renames(cli: &RenameCli) -> Result<Vec<(PathBuf, String, String)>, RenameError> {
    match (&cli.list, &cli.target, &cli.new) {
        (Some(list), None, None) => {
            if cli.at.is_some() {
                return Err(plan_error(
                    "--at disambiguates one rename; drop it when --list carries the renames"
                        .to_string(),
                ));
            }
            read_rename_list(list)
        }
        (Some(_), _, _) => Err(plan_error(
            "--list carries the renames; drop <FILE>#<OLD> and <NEW>".to_string(),
        )),
        (None, Some(target), Some(new)) => {
            let (anchor, old) = target.rsplit_once('#').ok_or_else(|| {
                plan_error(format!("a rename target is `<FILE>#<OLD>`, not {target}"))
            })?;
            if anchor.is_empty() || old.is_empty() {
                return Err(plan_error(format!(
                    "a rename target is `<FILE>#<OLD>`, not {target}"
                )));
            }
            Ok(vec![(PathBuf::from(anchor), old.to_string(), new.clone())])
        }
        (None, _, _) => Err(plan_error(
            "extract rename takes <FILE>#<OLD> <NEW>, or --list <tsv>".to_string(),
        )),
    }
}

/// `anchor<TAB>old<TAB>new` per line. A short row is an error, never a silent
/// skip: a dropped row is a rename that never happens.
fn read_rename_list(path: &Path) -> Result<Vec<(PathBuf, String, String)>, RenameError> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| plan_error(format!("read rename list {}: {error}", path.display())))?;
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').map(str::trim).collect();
        let [anchor, old, new] = fields.as_slice() else {
            return Err(plan_error(format!(
                "{}:{number}: a rename list row is `anchor<TAB>old<TAB>new`",
                path.display()
            )));
        };
        if anchor.is_empty() || old.is_empty() || new.is_empty() {
            return Err(plan_error(format!(
                "{}:{number}: all three fields of a rename list row are required",
                path.display()
            )));
        }
        rows.push((PathBuf::from(anchor), old.to_string(), new.to_string()));
    }
    if rows.is_empty() {
        return Err(plan_error(format!("{} names no renames", path.display())));
    }
    Ok(rows)
}

/// The corpus root: as asked, else the git root holding the first anchor.
fn plan_root(requested: Option<&PathBuf>, first: &Path) -> Result<PathBuf, RenameError> {
    let root = match requested {
        Some(root) => absolute(root)?,
        None => {
            let anchor = anchor_file(first)?;
            let parent = anchor.parent().unwrap_or(&anchor).to_path_buf();
            soopy::discover(&parent)
                .map_err(|error| {
                    plan_error(format!("discover root for {}: {error}", anchor.display()))
                })?
                .root
        }
    };
    root.canonicalize()
        .map_err(|error| plan_error(format!("canonicalize root {}: {error}", root.display())))
}

/// Every validation a batch needs, all of it before any arm is asked anything:
/// a missing anchor, an anchor outside the corpus, or a repeated `(anchor, old)`.
/// `at` rides on the single positional rename; a list carries no offsets.
fn validated_batch(
    root: &Path,
    requested: Vec<(PathBuf, String, String)>,
    at: Option<u32>,
) -> Result<Vec<RenameRequest>, RenameError> {
    let mut batch: Vec<RenameRequest> = Vec::with_capacity(requested.len());
    let mut seen: BTreeMap<(String, String), ()> = BTreeMap::new();
    for (anchor, old, new) in requested {
        let anchor = match anchor.is_absolute() {
            true => anchor_file(&anchor)?,
            false => anchor_file(&root.join(&anchor))?,
        };
        let anchor = within_root(root, &anchor)?;
        if old == new {
            return Err(plan_error(format!("{anchor}: {old} renames to itself")));
        }
        if seen.insert((anchor.clone(), old.clone()), ()).is_some() {
            return Err(plan_error(format!("{anchor}: {old} is renamed twice")));
        }
        batch.push(RenameRequest {
            anchor,
            old,
            new,
            at,
        });
    }
    Ok(batch)
}

fn anchor_file(path: &Path) -> Result<PathBuf, RenameError> {
    let path = absolute(path)?;
    if !path.is_file() {
        return Err(plan_error(format!(
            "rename anchor is not a file: {}",
            path.display()
        )));
    }
    path.canonicalize()
        .map_err(|error| plan_error(format!("canonicalize {}: {error}", path.display())))
}

fn absolute(path: &Path) -> Result<PathBuf, RenameError> {
    if path.is_absolute() {
        return Ok(normalize(path));
    }
    let cwd = std::env::current_dir()
        .map_err(|error| plan_error(format!("current directory: {error}")))?;
    Ok(normalize(&cwd.join(path)))
}

fn within_root(root: &Path, path: &Path) -> Result<String, RenameError> {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .map_err(|_| {
            plan_error(format!(
                "{} is outside root {}",
                path.display(),
                root.display()
            ))
        })
}
