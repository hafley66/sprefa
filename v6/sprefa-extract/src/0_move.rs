//! `extract move <old.pl> <new.pl>`: rehome one prolog file and repair every
//! specifier that named it, through soopy's staged mutation boundary.
//! @comment-ok: module header, the seam list every bin arm opens with
//!
//! ast-grep finds the specifiers (`rules/move_specifier.yml`), a `FactMatcher`
//! over the run's `move_candidate` rel says which of them name the moved file,
//! and arc B's drain folds the resulting edits into one soopy Replace per file.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use ast_grep_config::{from_yaml_string, GlobalRules, RuleConfig};
use ast_grep_core::meta_var::Underlying;
use ast_grep_core::ops::{Any, Op};
use ast_grep_core::replacer::Replacer;
use ast_grep_core::source::Doc;
use ast_grep_core::{AstGrep, NodeMatch, Pattern};
use clap::Parser;
use rayon::prelude::*;
use rusqlite::Connection;
use sprefa_extract::{
    bind_action, directory_path, directory_source, drain_edits, extract_pool, replace_action,
    source_rel, BoundEdit, ExtractLang, FactSet,
};

const PRODUCER: &str = "extract-move";

/// The rule is data, next to the code, and rides in the binary so a move needs
/// no file beside it.
const MOVE_SPECIFIER_RULE: &str = include_str!("../rules/move_specifier.yml");

/// The one rel this run asks the store about, and the column holding the spec
/// text as written. Same grain as the live `import_graph_candidate`.
const CANDIDATE_REL: &str = "move_candidate";
const CANDIDATE_COLUMN: &str = "raw";

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
    /// Apply the plan to the real tree instead of dry running it.
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
            let (id, previews) =
                stage_and_commit(&plan.root, &state, stage, soopy::Durability::Durable)?;
            print_previews(&previews);
            println!("stage {id} committed");
        }
    } else {
        let mirror = Mirror::build(&plan)?;
        for stage in &plan.stages {
            let (id, previews) =
                stage_and_commit(mirror.root(), &state, stage, soopy::Durability::DryRun)?;
            print_previews(&previews);
            println!("stage {id} dry run, tree untouched");
        }
    }
    Ok(())
}

/// Soopy accepts ONE operation per source file (`_7d_mutation_plan.rs`
/// `insert_non_replace`), so edits, the Move, and the shim Create are separate stages.
struct Plan {
    root: PathBuf,
    old_rel: String,
    new_rel: String,
    stages: Vec<Vec<soopy::SourceAction>>,
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
            return Err(format!(
                "move destination already exists: {}",
                new.display()
            ));
        }
        let old_rel = within_root(&root, &old)?;
        let new_rel = within_root(&root, &new)?;
        let new_dir = new.parent().unwrap_or(&root).to_path_buf();
        let old_dir = old.parent().unwrap_or(&root).to_path_buf();

        let corpus = prolog_files(&root);
        let rule = specifier_rule()?;
        let old_stem = stem(&old);
        let mut module_name: Option<String> = None;

        // Read and parse fan out; the merge below stays sequential over `corpus`
        // in path order, so the action order and the previews do not move.
        let scanned: Vec<Scanned> = extract_pool().install(|| {
            corpus
                .par_iter()
                .map(|file| {
                    let Ok(bytes) = std::fs::read(file) else {
                        return Scanned::Unreadable;
                    };
                    // `old` is parsed unconditionally: its module name is read off it.
                    if *file != old && !carries_specifier(&bytes, &old_stem) {
                        return Scanned::NoDirective;
                    }
                    let Ok(text) = String::from_utf8(bytes) else {
                        return Scanned::Unreadable;
                    };
                    Scanned::Rows(specifiers(&rule, text))
                })
                .collect()
        });

        let mut parsed = 0usize;
        let mut skipped = 0usize;
        // rel -> raw spec text -> what it becomes. One rewrite per raw: a spec
        // written twice in one file resolves to one replacement.
        let mut rewrites: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let mut candidates: Vec<CandidateRow> = Vec::new();
        // The parse the drain reads back, so no file is parsed twice in one run.
        let mut parses: BTreeMap<String, &Parsed> = BTreeMap::new();

        for (file, scan) in corpus.iter().zip(&scanned) {
            let rows = match scan {
                Scanned::Rows(rows) => {
                    parsed += 1;
                    rows
                }
                Scanned::NoDirective => {
                    skipped += 1;
                    continue;
                }
                Scanned::Unreadable => continue,
            };
            let dir = file.parent().unwrap_or(&root).to_path_buf();
            let is_old = *file == old;
            if is_old {
                module_name = rows.module.clone();
            }
            for raw in &rows.paths {
                let Some(target) = resolve(&dir, raw) else {
                    continue;
                };
                // Inside the moved file every relative spec is re-aimed from the
                // destination dir; elsewhere only the ones naming the moved file.
                let (from_dir, aimed) = if is_old {
                    if target == old {
                        continue;
                    }
                    (new_dir.as_path(), target.clone())
                } else if target == old {
                    (dir.as_path(), new.clone())
                } else {
                    continue;
                };
                let replacement = spec_text(from_dir, &aimed, raw);
                if &replacement == raw {
                    continue;
                }
                let rel = within_root(&root, file)?;
                candidates.push(CandidateRow {
                    path: rel.clone(),
                    raw: raw.clone(),
                    target: target.display().to_string(),
                });
                parses.insert(rel.clone(), &rows.parse);
                rewrites
                    .entry(rel)
                    .or_default()
                    .insert(raw.clone(), replacement);
            }
        }
        tracing::debug!(parsed, skipped, corpus = corpus.len(), "move prescan");
        if cli.shim {
            rewrites.retain(|rel, _| rel == &old_rel);
            candidates.retain(|row| row.path == old_rel);
        }

        // Every action is planned against the real root; `bind_action` re-aims it
        // at the root that actually stages it, and re-reads `expected` there.
        let identity = soopy::SourceRoot::open_directory(&root)
            .map_err(|error| format!("open root {}: {error}", root.display()))?
            .directory()
            .identity
            .clone();

        // ONE read of the run's candidate rel, grouped by the file that wrote
        // each spec: the store, not the scan, says which nodes a file rewrites.
        let store = candidate_store(&candidates)?;
        let named = FactSet::load_by(&store, CANDIDATE_REL, "path", CANDIDATE_COLUMN)
            .map_err(|error| error.to_string())?;

        // The fact-gated scan fans out; `named` is a BTreeMap and an indexed
        // rayon collect keeps its order, so the staged action order is rel order.
        let files: Vec<(&String, &Arc<FactSet>)> = named.iter().collect();
        let drained: Vec<Option<soopy::SourceAction>> = extract_pool().install(|| {
            files
                .into_par_iter()
                .map(|(rel, facts)| {
                    let parse = parses.get(rel)?;
                    let source = directory_source(&identity, rel);
                    let by_raw = rewrites.get(rel).cloned().unwrap_or_default();
                    drain_file(rel, parse, &rule, facts, &by_raw, source)
                })
                .collect()
        });

        let edit_stage: Vec<soopy::SourceAction> = drained.into_iter().flatten().collect();
        tracing::debug!(
            files = named.len(),
            staged = edit_stage.len(),
            "move drain done"
        );

        let mut stages = Vec::new();
        if !edit_stage.is_empty() {
            stages.push(edit_stage);
        }
        stages.push(vec![soopy::SourceAction::Move {
            source: directory_source(&identity, &old_rel),
            expected: content_id(&root.join(&old_rel))?,
            destination: directory_path(&new_rel),
        }]);
        if cli.shim {
            let module = module_name.unwrap_or_else(|| stem(&old));
            let target = spec_text(&old_dir, &new, "''");
            let body = format!(":- module({module}_shim, []).\n:- reexport({target}).\n");
            stages.push(vec![soopy::SourceAction::Create {
                path: directory_path(&old_rel),
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

/// One file's prescan outcome, kept aligned to the corpus so the merge keeps
/// path order.
enum Scanned {
    Rows(SpecRows),
    NoDirective,
    Unreadable,
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

/// `resolve` never invents a filename, so a spec naming the moved file carries
/// its stem verbatim and the moved file is admitted by its own stem.
fn carries_specifier(bytes: &[u8], stem: &str) -> bool {
    if stem.is_empty() || memchr::memmem::find(bytes, stem.as_bytes()).is_none() {
        return false;
    }
    SPEC_NEEDLES
        .iter()
        .any(|needle| memchr::memmem::find(bytes, needle.as_bytes()).is_some())
}

/// One prolog file's frozen parse, read by the prescan and again by the drain.
type Parsed = AstGrep<ast_grep_core::tree_sitter::StrDoc<ExtractLang>>;

struct SpecRows {
    paths: Vec<String>,
    module: Option<String>,
    parse: Parsed,
}

/// One row of the rel the fact matcher reads: which file wrote which spec, and
/// which file that spec names.
struct CandidateRow {
    path: String,
    raw: String,
    target: String,
}

/// `language:` is not a field the rule file carries: the grammar is the
/// caller's, so it is supplied here.
fn specifier_rule() -> Result<RuleConfig<ExtractLang>, String> {
    let yaml = format!("language: prolog\n{MOVE_SPECIFIER_RULE}");
    from_yaml_string(&yaml, &GlobalRules::default())
        .map_err(|error| format!("rules/move_specifier.yml: {error}"))?
        .into_iter()
        .next()
        .ok_or_else(|| "rules/move_specifier.yml holds no rule".to_string())
}

/// Every spec the rule finds, in source order, plus the file's own module name.
/// A spec is kept as written; resolution and re-aiming are the caller's.
fn specifiers(rule: &RuleConfig<ExtractLang>, text: String) -> SpecRows {
    let parse = AstGrep::new(text, ExtractLang::Prolog);
    let mut paths: Vec<String> = parse
        .root()
        .find_all(&rule.matcher)
        .map(|matched| matched.text().to_string())
        .collect();
    paths.dedup();
    let module = module_name(&parse);
    SpecRows {
        paths,
        module,
        parse,
    }
}

/// Unquoted the way the extractor reads it (`lang/prolog/_0_source.rs:505`,
/// `atom_text`), off the same parse the specifiers came from.
fn module_name(root: &Parsed) -> Option<String> {
    let pattern = Pattern::contextual(
        "module($NAME, $EXPORTS)",
        "compound_term",
        ExtractLang::Prolog,
    )
    .ok()?;
    let matched = root.root().find(&pattern)?;
    let name = matched.get_env().get_match("NAME")?.text().to_string();
    let (_, bare) = unquote(&name);
    Some(bare.to_string())
}

/// The store's shape: `__str` UNIQUE on the natural key, the rel surrogate-keyed
/// against it. One prepared statement per table, one transaction.
fn candidate_store(rows: &[CandidateRow]) -> Result<Connection, String> {
    let mut store = Connection::open_in_memory().map_err(|error| error.to_string())?;
    store
        .execute_batch(&format!(
            "CREATE TABLE \"__str\" (\"__id\" INTEGER PRIMARY KEY, \"content\" TEXT NOT NULL UNIQUE);
             CREATE TABLE \"{CANDIDATE_REL}\" (\"__id\" INTEGER PRIMARY KEY,
                \"path\" INTEGER NOT NULL, \"{CANDIDATE_COLUMN}\" INTEGER NOT NULL,
                \"target\" INTEGER NOT NULL,
                UNIQUE (\"path\", \"{CANDIDATE_COLUMN}\", \"target\"));"
        ))
        .map_err(|error| error.to_string())?;
    let transaction = store.transaction().map_err(|error| error.to_string())?;
    {
        let mut intern = transaction
            .prepare("INSERT OR IGNORE INTO \"__str\" (\"content\") VALUES (?1)")
            .map_err(|error| error.to_string())?;
        for row in rows {
            for value in [&row.path, &row.raw, &row.target] {
                intern.execute([value]).map_err(|error| error.to_string())?;
            }
        }
        let mut insert = transaction
            .prepare(&format!(
                "INSERT OR IGNORE INTO \"{CANDIDATE_REL}\"
                    (\"path\", \"{CANDIDATE_COLUMN}\", \"target\")
                 SELECT p.\"__id\", r.\"__id\", t.\"__id\"
                 FROM \"__str\" p, \"__str\" r, \"__str\" t
                 WHERE p.\"content\" = ?1 AND r.\"content\" = ?2 AND t.\"content\" = ?3"
            ))
            .map_err(|error| error.to_string())?;
        for row in rows {
            insert
                .execute([&row.path, &row.raw, &row.target])
                .map_err(|error| error.to_string())?;
        }
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(store)
}

/// One file's Replace off the ONE parse the prescan made. The fact matchers keep
/// the specs the store names; `expected` hashes the bytes that parse came from.
fn drain_file(
    rel: &str,
    parse: &Parsed,
    rule: &RuleConfig<ExtractLang>,
    facts: &Arc<sprefa_extract::FactSet>,
    by_raw: &BTreeMap<String, String>,
    source: soopy::ActionSource,
) -> Option<soopy::SourceAction> {
    let producer = soopy::ActionProducer::unordered(PRODUCER);
    let root = parse.root();
    let expected = soopy::ContentId::blake3(root.text().as_bytes());
    let named = Any::new(facts.values().map(|raw| facts.matcher(raw)).collect::<Vec<_>>());
    let matcher = Op::every(&rule.matcher).and(named);
    let rewrite = SpecifierRewrite {
        by_raw: by_raw.clone(),
    };

    let edits = drain_edits(&root, &matcher, &rewrite);
    tracing::debug!(rel, edits = edits.len(), named = facts.len(), "move drain");
    if edits.is_empty() {
        return None;
    }
    let text_edits = edits
        .into_iter()
        .map(|edit| {
            BoundEdit {
                source: source.clone(),
                producer: producer.clone(),
                edit,
            }
            .into()
        })
        .collect();
    Some(replace_action(source, expected, text_edits))
}

/// The replacement for one matched spec, keyed on the spec as written. ONE
/// rewrite per raw text, so a file that names the moved file twice re-aims both.
struct SpecifierRewrite {
    by_raw: BTreeMap<String, String>,
}

impl<D: Doc<Source = String>> Replacer<D> for SpecifierRewrite {
    fn generate_replacement(&self, matched: &NodeMatch<'_, D>) -> Underlying<D> {
        let raw = matched.text();
        self.by_raw
            .get(raw.as_ref())
            .map(|replacement| replacement.as_bytes().to_vec())
            .unwrap_or_else(|| raw.as_bytes().to_vec())
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

fn stage_into<S: soopy::StageStore>(
    source_root: &mut soopy::SourceRoot,
    request: &soopy::StageRequest,
    store: &mut S,
) -> Result<soopy::StagedSourceTransaction, String> {
    let sealed = soopy::stage_mutations(source_root, request, store)
        .map_err(|refusal| format!("stage refused: {refusal}"))?;
    // `save` returns the manifest; only `load` rehydrates the blobs commit writes.
    soopy::show_stage(store, sealed.id)
        .map_err(|error| format!("load stage {}: {error}", sealed.id))?
        .ok_or_else(|| format!("stage {} vanished from the store", sealed.id))
}

fn stage_and_commit(
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
        .map_err(|refusal| format!("commit refused: {refusal}"))?;
    Ok((stage.id.to_string(), stage.previews))
}

fn content_id(path: &Path) -> Result<soopy::ContentId, String> {
    let bytes =
        std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    Ok(soopy::ContentId::blake3(&bytes))
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

/// A temp root carrying only the files the plan touches. The dry run commits
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
