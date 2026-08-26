//! `extract move <old> <new>`: rehome a file and repair every specifier that
//! named it, through soopy's staged mutation boundary.
//! @comment-ok: module header, the seam list every bin arm opens with

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
    bind_action, corpus_lang, directory_path, directory_source, drain_edits, extract_pool,
    is_ts_family, replace_action, respell, source_rel, ts_corpus, ts_specifiers, BoundEdit,
    CorpusLang, ExtractLang, FactSet, TsResolver, TsSpecifier,
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
    about = "move a file and repair every specifier that named it"
)]
struct MoveCli {
    /// The file to rehome.
    old: PathBuf,
    /// Where it lands.
    new: PathBuf,
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
    for spec in &plan.moves {
        println!("plan {} -> {}", spec.old_rel, spec.new_rel);
    }

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

/// One move: the canonical source, its unborn destination, and the arm that
/// reads the corpus for it.
struct MoveSpec {
    old: PathBuf,
    new: PathBuf,
    old_rel: String,
    new_rel: String,
    lang: CorpusLang,
}

/// Soopy accepts ONE operation per source file (`_7d_mutation_plan.rs`
/// `insert_non_replace`), so edits, Moves and the shim Create are separate stages.
struct Plan {
    root: PathBuf,
    moves: Vec<MoveSpec>,
    stages: Vec<Vec<soopy::SourceAction>>,
}

impl Plan {
    fn build(cli: &MoveCli) -> Result<Self, String> {
        let requested = requested_moves(cli)?;
        let root = plan_root(cli, &requested[0].0)?;
        let moves = validated_moves(&root, requested)?;
        if cli.shim && moves[0].lang != CorpusLang::Prolog {
            return Err(format!(
                "--shim writes a prolog reexport; {} is not prolog",
                moves[0].old_rel
            ));
        }
        let moved: BTreeMap<PathBuf, PathBuf> = moves
            .iter()
            .map(|spec| (spec.old.clone(), spec.new.clone()))
            .collect();

        // Every action is planned against the real root; `bind_action` re-aims it
        // at the root that actually stages it.
        let identity = soopy::SourceRoot::open_directory(&root)
            .map_err(|error| format!("open root {}: {error}", root.display()))?
            .directory()
            .identity
            .clone();

        let mut edit_stage: Vec<soopy::SourceAction> = Vec::new();
        let mut modules: BTreeMap<String, String> = BTreeMap::new();
        if moves.iter().any(|spec| spec.lang == CorpusLang::Prolog) {
            let (actions, names) = prolog_edits(&root, &moved, &identity, cli.shim)?;
            edit_stage.extend(actions);
            modules = names;
        }
        if moves.iter().any(|spec| is_ts(spec.lang)) {
            edit_stage.extend(ts_edits(&root, &moved, &identity)?);
        }
        // One arm reads `.pl`, the other the TS family, so no rel is written
        // twice; the sort states the stage order for a mixed list.
        edit_stage.sort_by(|left, right| source_rel(left).cmp(&source_rel(right)));

        let mut stages = Vec::new();
        if !edit_stage.is_empty() {
            stages.push(edit_stage);
        }
        let mut move_stage = Vec::with_capacity(moves.len());
        for spec in &moves {
            move_stage.push(soopy::SourceAction::Move {
                source: directory_source(&identity, &spec.old_rel),
                expected: content_id(&root.join(&spec.old_rel))?,
                destination: directory_path(&spec.new_rel),
            });
        }
        stages.push(move_stage);
        if cli.shim {
            let spec = &moves[0];
            let module = modules
                .get(&spec.old_rel)
                .cloned()
                .unwrap_or_else(|| stem(&spec.old));
            let old_dir = spec.old.parent().unwrap_or(&root).to_path_buf();
            let target = spec_text(&old_dir, &spec.new, "''");
            let body = format!(":- module({module}_shim, []).\n:- reexport({target}).\n");
            stages.push(vec![soopy::SourceAction::Create {
                path: directory_path(&spec.old_rel),
                bytes: body.into_bytes(),
            }]);
        }
        Ok(Plan {
            root,
            moves,
            stages,
        })
    }
}

/// The `(old, new)` pairs the invocation asks for. One row today; the batch
/// door lands on top of this seat.
fn requested_moves(cli: &MoveCli) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    Ok(vec![(cli.old.clone(), cli.new.clone())])
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
    root: &Path,
    requested: Vec<(PathBuf, PathBuf)>,
) -> Result<Vec<MoveSpec>, String> {
    let mut moves: Vec<MoveSpec> = Vec::with_capacity(requested.len());
    let mut olds: BTreeSet<PathBuf> = BTreeSet::new();
    let mut news: BTreeSet<PathBuf> = BTreeSet::new();
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
        let lang = corpus_lang(&old_rel)
            .ok_or_else(|| format!("extract move reads prolog and the TS family: {old_rel}"))?;
        let destination = corpus_lang(&new_rel)
            .ok_or_else(|| format!("move destination has no known extension: {new_rel}"))?;
        if is_ts(lang) != is_ts(destination) {
            return Err(format!("{old_rel} -> {new_rel} crosses languages"));
        }
        if !olds.insert(old.clone()) {
            return Err(format!("{old_rel} is moved twice"));
        }
        if !news.insert(new.clone()) {
            return Err(format!("{new_rel} is the destination of two moves"));
        }
        moves.push(MoveSpec {
            old,
            new,
            old_rel,
            new_rel,
            lang,
        });
    }
    Ok(moves)
}

fn is_ts(lang: CorpusLang) -> bool {
    matches!(lang, CorpusLang::Ts | CorpusLang::Tsx)
}

// ── the prolog arm ──────────────────────────────────────────────────────────

/// Every prolog importer's Replace, plus the module name each moved prolog file
/// declares (the shim's header reads it off the same parse).
fn prolog_edits(
    root: &Path,
    moved: &BTreeMap<PathBuf, PathBuf>,
    identity: &soopy::DirectoryId,
    shim: bool,
) -> Result<(Vec<soopy::SourceAction>, BTreeMap<String, String>), String> {
    let corpus = prolog_files(root);
    let rule = specifier_rule()?;
    let stems: BTreeSet<String> = moved
        .keys()
        .filter(|path| corpus_lang(&path.to_string_lossy()) == Some(CorpusLang::Prolog))
        .map(|path| stem(path))
        .filter(|stem| !stem.is_empty())
        .collect();

    // Read and parse fan out; the merge below stays sequential over `corpus`
    // in path order, so the action order and the previews do not move.
    let scanned: Vec<Scanned> = extract_pool().install(|| {
        corpus
            .par_iter()
            .map(|file| {
                let Ok(bytes) = std::fs::read(file) else {
                    return Scanned::Unreadable;
                };
                // A moved file is parsed unconditionally: its module name is read off it.
                if !moved.contains_key(file) && !carries_specifier(&bytes, &stems) {
                    return Scanned::NoDirective;
                }
                let content = soopy::ContentId::blake3(&bytes);
                let Ok(text) = String::from_utf8(bytes) else {
                    return Scanned::Unreadable;
                };
                Scanned::Rows(specifiers(&rule, text, content))
            })
            .collect()
    });

    let mut parsed = 0usize;
    let mut skipped = 0usize;
    // rel -> raw spec text -> what it becomes. One rewrite per raw: a spec
    // written twice in one file resolves to one replacement.
    let mut rewrites: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut candidates: Vec<CandidateRow> = Vec::new();
    // The parse the drain reads back, so no file is parsed twice in one run,
    // paired with the hash of the bytes that parse came from.
    let mut parses: BTreeMap<String, (&Parsed, soopy::ContentId)> = BTreeMap::new();
    let mut modules: BTreeMap<String, String> = BTreeMap::new();

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
        let dir = file.parent().unwrap_or(root).to_path_buf();
        let rel = within_root(root, file)?;
        let destination = moved.get(file);
        if destination.is_some() {
            if let Some(module) = rows.module.clone() {
                modules.insert(rel.clone(), module);
            }
        }
        for raw in &rows.paths {
            let Some(target) = resolve(&dir, raw) else {
                continue;
            };
            // Inside a moved file every relative spec is re-aimed from the
            // destination dir; elsewhere only the ones naming a moved file.
            let (from_dir, aimed) = match destination {
                Some(new) => {
                    if target == *file {
                        continue;
                    }
                    let from = new.parent().unwrap_or(root);
                    (from, moved.get(&target).unwrap_or(&target).clone())
                }
                None => match moved.get(&target) {
                    Some(new) => (dir.as_path(), new.clone()),
                    None => continue,
                },
            };
            let replacement = spec_text(from_dir, &aimed, raw);
            if &replacement == raw {
                continue;
            }
            candidates.push(CandidateRow {
                path: rel.clone(),
                raw: raw.clone(),
                target: target.display().to_string(),
            });
            parses.insert(rel.clone(), (&rows.parse, rows.content.clone()));
            rewrites
                .entry(rel.clone())
                .or_default()
                .insert(raw.clone(), replacement);
        }
    }
    tracing::debug!(parsed, skipped, corpus = corpus.len(), "move prescan");
    if shim {
        let moved_rels: BTreeSet<String> = moved
            .keys()
            .filter_map(|path| within_root(root, path).ok())
            .collect();
        rewrites.retain(|rel, _| moved_rels.contains(rel));
        candidates.retain(|row| moved_rels.contains(&row.path));
    }

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
                let (parse, content) = parses.get(rel)?;
                let source = directory_source(identity, rel);
                let by_raw = rewrites.get(rel).cloned().unwrap_or_default();
                drain_file(rel, parse, content.clone(), &rule, facts, &by_raw, source)
            })
            .collect()
    });

    let actions: Vec<soopy::SourceAction> = drained.into_iter().flatten().collect();
    tracing::debug!(
        files = named.len(),
        staged = actions.len(),
        "move drain done"
    );
    Ok((actions, modules))
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

/// `resolve` never invents a filename, so a spec naming a moved file carries
/// that file's stem verbatim and a moved file is admitted by its own stem.
fn carries_specifier(bytes: &[u8], stems: &BTreeSet<String>) -> bool {
    if !stems
        .iter()
        .any(|stem| memchr::memmem::find(bytes, stem.as_bytes()).is_some())
    {
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
    /// The bytes on disk, not the parse's text: a tree-sitter root node spans
    /// its first token to its last, so trailing bytes are outside `root().text()`.
    content: soopy::ContentId,
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
fn specifiers(
    rule: &RuleConfig<ExtractLang>,
    text: String,
    content: soopy::ContentId,
) -> SpecRows {
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
        content,
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
#[allow(clippy::too_many_arguments)]
fn drain_file(
    rel: &str,
    parse: &Parsed,
    expected: soopy::ContentId,
    rule: &RuleConfig<ExtractLang>,
    facts: &Arc<sprefa_extract::FactSet>,
    by_raw: &BTreeMap<String, String>,
    source: soopy::ActionSource,
) -> Option<soopy::SourceAction> {
    let producer = soopy::ActionProducer::unordered(PRODUCER);
    let root = parse.root();
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

// ── the TypeScript arm ──────────────────────────────────────────────────────

/// Every TS importer's Replace, off the oxc parse's module-literal spans
/// (`lang/ts.rs`) and `oxc_resolver` (`lang/ts_resolve.rs`). No rule file.
fn ts_edits(
    root: &Path,
    moved: &BTreeMap<PathBuf, PathBuf>,
    identity: &soopy::DirectoryId,
) -> Result<Vec<soopy::SourceAction>, String> {
    let resolver = TsResolver::new(root)?;
    let names = moved_names(moved);
    let corpus = ts_corpus(root);
    // An indexed rayon collect keeps corpus order, and corpus order is rel order.
    let planned: Vec<Option<soopy::SourceAction>> = extract_pool().install(|| {
        corpus
            .par_iter()
            .map(|file| ts_file_edits(root, file, &resolver, moved, &names, identity))
            .collect()
    });
    let actions: Vec<soopy::SourceAction> = planned.into_iter().flatten().collect();
    tracing::debug!(
        corpus = corpus.len(),
        staged = actions.len(),
        "move ts drain done"
    );
    Ok(actions)
}

/// One TS file's Replace, or None when nothing it writes names a moved file.
fn ts_file_edits(
    root: &Path,
    file: &Path,
    resolver: &TsResolver,
    moved: &BTreeMap<PathBuf, PathBuf>,
    names: &BTreeSet<String>,
    identity: &soopy::DirectoryId,
) -> Option<soopy::SourceAction> {
    let text = std::fs::read_to_string(file).ok()?;
    let destination = moved.get(file);
    let rows = ts_specifiers(&file.to_string_lossy(), &text).ok()?;
    let dir = file.parent()?;
    let from_dir = match destination {
        Some(new) => new.parent()?,
        None => dir,
    };
    let rel = within_root(root, file).ok()?;
    let source = directory_source(identity, &rel);
    let producer = soopy::ActionProducer::unordered(PRODUCER);
    let mut edits: Vec<soopy::TextEdit> = Vec::new();
    for row in &rows {
        let Some(replacement) = ts_replacement(
            resolver,
            root,
            file,
            from_dir,
            destination.is_some(),
            moved,
            names,
            row,
        ) else {
            continue;
        };
        let start = row.module_span.start as usize;
        let end = start + row.module_span.len as usize;
        if text.get(start..end) == Some(replacement.as_str()) {
            continue;
        }
        edits.push(
            BoundEdit {
                source: source.clone(),
                producer: producer.clone(),
                edit: ast_grep_core::source::Edit {
                    position: start,
                    deleted_length: row.module_span.len as usize,
                    inserted_text: replacement.into_bytes(),
                },
            }
            .into(),
        );
    }
    tracing::debug!(rel, edits = edits.len(), "move ts drain");
    if edits.is_empty() {
        return None;
    }
    let expected = soopy::ContentId::blake3(text.as_bytes());
    Some(replace_action(source, expected, edits))
}

/// The quoted replacement for one specifier row, or None when the row names no
/// moved file and needs no re-aim.
#[allow(clippy::too_many_arguments)]
fn ts_replacement(
    resolver: &TsResolver,
    root: &Path,
    file: &Path,
    from_dir: &Path,
    is_moved: bool,
    moved: &BTreeMap<PathBuf, PathBuf>,
    names: &BTreeSet<String>,
    row: &TsSpecifier,
) -> Option<String> {
    let relative_spec = row.module.starts_with('.');
    // A relative spec spells its target's own name; a bare or alias spec spells a
    // package or a tsconfig path, and only the resolver says what it reaches.
    if !is_moved && relative_spec && !spec_may_name(&row.module, names) {
        return None;
    }
    let target = resolver.resolve(file, &row.module)?;
    if target == *file {
        return None;
    }
    let aimed = match moved.get(&target) {
        Some(new) => new.clone(),
        // A file that stays put is re-aimed only for a relative spec in a moving
        // importer: a tsconfig path and a package name anchor to the root.
        None if is_moved && relative_spec && target.starts_with(root) => target.clone(),
        None => return None,
    };
    if !relative_spec {
        let alias = alias_respell(
            resolver,
            file,
            root,
            &row.module,
            &target,
            &aimed,
            row.quote,
        );
        if alias.is_some() {
            return alias;
        }
    }
    Some(respell(&relative_from(from_dir, &aimed), &row.module, row.quote))
}

/// The file names a batch can be reached by: every moved file's stem, plus the
/// directory name of a moved `index`, which is what a directory-form spec spells.
fn moved_names(moved: &BTreeMap<PathBuf, PathBuf>) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for old in moved.keys() {
        if !is_ts_family(&old.to_string_lossy()) {
            continue;
        }
        let own = stem(old);
        if own == "index" {
            if let Some(parent) = old.parent() {
                names.insert(stem(parent));
            }
        }
        names.insert(own);
    }
    names
}

/// Whether a relative spec's last segment can name one of the moved files. A
/// spec with no readable last segment is never gated out.
fn spec_may_name(module: &str, names: &BTreeSet<String>) -> bool {
    let last = module.rsplit('/').next().unwrap_or(module);
    let stem = last.split('.').next().unwrap_or(last);
    stem.is_empty() || names.contains(stem)
}

/// An alias keeps its alias when the prefix it resolved through still covers the
/// destination, re-probed against a file already there. Else: a relative path.
#[allow(clippy::too_many_arguments)]
fn alias_respell(
    resolver: &TsResolver,
    from: &Path,
    root: &Path,
    original: &str,
    old_target: &Path,
    new_target: &Path,
    quote: char,
) -> Option<String> {
    let old_rel = within_root(root, old_target).ok()?;
    let (prefix, mapped) = alias_prefix(original, &old_rel)?;
    let directory = root.join(&mapped);
    if !new_target.starts_with(&directory) {
        return None;
    }
    let witness = alias_witness(&directory, new_target, old_target)?;
    let witness_rel = relative_from(&directory, &witness);
    let stripped = witness_rel
        .rsplit_once('.')
        .map(|(head, _)| head)
        .unwrap_or(&witness_rel);
    let probe = resolver.resolve(from, &format!("{prefix}/{stripped}"));
    if probe.as_deref() != Some(witness.as_path()) {
        return None;
    }
    // `respell` writes a `./`-led relative path with the original's extension
    // style; the alias prefix replaces that lead.
    let spelled = respell(&relative_from(&directory, new_target), original, quote);
    let tail = spelled.trim_matches(quote).strip_prefix("./")?.to_string();
    Some(format!("{quote}{prefix}/{tail}{quote}"))
}

/// The alias prefix and the directory it maps to, read off one resolution: a
/// `paths` entry splices text, so the spec's tail is the resolved path's tail.
fn alias_prefix(original: &str, target_rel: &str) -> Option<(String, String)> {
    let spec: Vec<&str> = original.split('/').filter(|part| !part.is_empty()).collect();
    let mut path: Vec<&str> = target_rel.split('/').filter(|part| !part.is_empty()).collect();
    let spec_last = segment_stem(spec.last()?);
    if segment_stem(path.last()?) == "index" && spec_last != "index" {
        path.pop();
    }
    let mut shared = 0;
    while shared + 1 < spec.len() && shared < path.len() {
        let left = spec[spec.len() - 1 - shared];
        let right = path[path.len() - 1 - shared];
        let same = if shared == 0 {
            segment_stem(left) == segment_stem(right)
        } else {
            left == right
        };
        if !same {
            break;
        }
        shared += 1;
    }
    if shared == 0 {
        return None;
    }
    Some((
        spec[..spec.len() - shared].join("/"),
        path[..path.len() - shared].join("/"),
    ))
}

fn segment_stem(segment: &str) -> &str {
    segment.split('.').next().unwrap_or(segment)
}

/// The deepest existing file under both the mapped directory and the
/// destination's ancestry. The moved file is the last resort: it proves least.
fn alias_witness(directory: &Path, new_target: &Path, old_target: &Path) -> Option<PathBuf> {
    let mut probe = new_target.parent()?;
    loop {
        if probe.starts_with(directory) && probe.is_dir() {
            let mut found: Option<PathBuf> = None;
            let mut entries: Vec<PathBuf> = std::fs::read_dir(probe)
                .ok()?
                .flatten()
                .map(|entry| entry.path())
                .collect();
            entries.sort();
            for entry in entries {
                if !entry.is_file() || !is_ts_family(&entry.to_string_lossy()) {
                    continue;
                }
                if entry == old_target {
                    found = found.or(Some(entry));
                    continue;
                }
                return Some(entry);
            }
            if let Some(entry) = found {
                return Some(entry);
            }
        }
        if probe == directory {
            return None;
        }
        probe = probe.parent()?;
    }
}

// ── paths, roots and the soopy boundary ─────────────────────────────────────

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
