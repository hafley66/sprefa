//! `impl Rehome for PrologSource`: the load-directive specifiers `extract move`
//! re-aims, off the committed `rules/move_specifier.yml` scan, gated by the
//! `move_candidate` rel the fact matcher reads back.
//! @comment-ok: module header, the seam list every lang file opens with

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ast_grep_config::{from_yaml_string, GlobalRules, RuleConfig};
use ast_grep_core::meta_var::Underlying;
use ast_grep_core::ops::{Any, Op};
use ast_grep_core::replacer::Replacer;
use ast_grep_core::source::Doc;
use ast_grep_core::{AstGrep, NodeMatch, Pattern};
use rayon::prelude::*;
use rusqlite::Connection;

use super::PrologSource;
use crate::lang::extract_lang::ExtractLang;
use crate::lang::fact::FactSet;
use crate::move_cx::{dirname, join_rel, relative_between, MoveCx};
use crate::project::extract_pool;
use crate::types::{ImportRef, ImportRefKind, Rehome, Respell, Span};

/// The rule is data, next to the code, and rides in the binary so a move needs
/// no file beside it.
const MOVE_SPECIFIER_RULE: &str = include_str!("../../../rules/move_specifier.yml");

/// The one rel this run asks the store about, and the column holding the spec
/// text as written. Same grain as the live `import_graph_candidate`.
const CANDIDATE_REL: &str = "move_candidate";
const CANDIDATE_COLUMN: &str = "raw";

/// The directive names that can carry a file spec (`_0_source.rs:379-383`).
/// Bare words, not `include(`: `'include'(...)` still has to match.
const SPEC_NEEDLES: [&str; 5] = [
    "use_module",
    "ensure_loaded",
    "consult",
    "include",
    "reexport",
];

impl Rehome for PrologSource {
    fn import_refs(&self, cx: &MoveCx) -> Vec<ImportRef> {
        let Ok(rule) = specifier_rule() else {
            return Vec::new();
        };
        let corpus = cx.files_of(self);
        let stems: BTreeSet<String> = cx
            .moved()
            .keys()
            .filter(|rel| crate::move_cx::owned_by(rel, self))
            .map(|rel| crate::move_cx::stem(rel))
            .filter(|stem| !stem.is_empty())
            .collect();

        // Read and parse fan out; the merge below stays sequential over `corpus`
        // in path order, so the action order and the previews do not move.
        let scanned: Vec<Scanned> = extract_pool().install(|| {
            corpus
                .par_iter()
                .map(|rel| {
                    let Some(bytes) = cx.read(rel) else {
                        return Scanned::Unreadable;
                    };
                    // A moved file is parsed unconditionally: its module name is read off it.
                    if cx.destination(rel).is_none() && !carries_specifier(&bytes, &stems) {
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
        let mut candidates: Vec<CandidateRow> = Vec::new();
        // The parse the drain reads back, so no file is parsed twice in one run.
        let mut parses: BTreeMap<&str, &Parsed> = BTreeMap::new();
        for (rel, scan) in corpus.iter().zip(&scanned) {
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
            for raw in &rows.paths {
                let Some(target) = resolve(cx, dirname(rel), raw) else {
                    continue;
                };
                if !re_aimed(cx, rel, &target, raw) {
                    continue;
                }
                candidates.push(CandidateRow {
                    path: rel.to_string(),
                    raw: raw.clone(),
                    target: target.clone(),
                });
                parses.insert(rel, &rows.parse);
            }
        }
        tracing::debug!(parsed, skipped, corpus = corpus.len(), "move prescan");
        if cx.shim() {
            candidates.retain(|row| cx.destination(&row.path).is_some());
        }

        // ONE read of the run's candidate rel, grouped by the file that wrote
        // each spec: the store, not the scan, says which nodes a file rewrites.
        let Ok(store) = candidate_store(&candidates) else {
            return Vec::new();
        };
        let Ok(named) = FactSet::load_by(&store, CANDIDATE_REL, "path", CANDIDATE_COLUMN) else {
            return Vec::new();
        };
        let targets: BTreeMap<(String, String), String> = candidates
            .iter()
            .map(|row| ((row.path.clone(), row.raw.clone()), row.target.clone()))
            .collect();

        // The fact-gated scan fans out; `named` is a BTreeMap and an indexed
        // rayon collect keeps its order, so the ref order is rel order.
        let files: Vec<(&String, &Arc<FactSet>)> = named.iter().collect();
        let per_file: Vec<Vec<ImportRef>> = extract_pool().install(|| {
            files
                .into_par_iter()
                .map(|(rel, facts)| {
                    let Some(parse) = parses.get(rel.as_str()) else {
                        return Vec::new();
                    };
                    drain_refs(rel, parse, &rule, facts, &targets)
                })
                .collect()
        });
        let refs: Vec<ImportRef> = per_file.into_iter().flatten().collect();
        let touched: BTreeSet<&str> = refs.iter().map(|row| row.importer.as_str()).collect();
        tracing::debug!(
            files = named.len(),
            staged = touched.len(),
            "move drain done"
        );
        refs
    }

    fn respell(&self, cx: &MoveCx, reference: &ImportRef) -> Option<Respell> {
        let from_dir = dirname(cx.after(&reference.importer));
        let aimed = cx.after(&reference.target);
        let text = spec_text(from_dir, aimed, &reference.text);
        (text != reference.text).then(|| Respell {
            file: reference.importer.clone(),
            span: reference.literal,
            text,
            receipt: None,
        })
    }

    /// A module wearing the old path that reexports the new one, so every
    /// importer this run did not rewrite still loads.
    fn shim(&self, cx: &MoveCx, old: &str, new: &str) -> Option<String> {
        let text = cx.text(old)?;
        let parse = AstGrep::new(text, ExtractLang::Prolog);
        let module = module_name(&parse).unwrap_or_else(|| crate::move_cx::stem(old));
        let target = spec_text(dirname(old), new, "''");
        Some(format!(
            ":- module({module}_shim, []).\n:- reexport({target}).\n"
        ))
    }
}

/// Whether this batch re-aims the spec `raw` in `rel` naming `target`. A moved
/// file re-aims every spec; elsewhere only the ones naming a moved file.
fn re_aimed(cx: &MoveCx, rel: &str, target: &str, raw: &str) -> bool {
    let moving = cx.destination(rel).is_some();
    if moving {
        if target == rel {
            return false;
        }
    } else if cx.destination(target).is_none() {
        return false;
    }
    let from_dir = dirname(cx.after(rel));
    spec_text(from_dir, cx.after(target), raw) != raw
}

/// One file's prescan outcome, kept aligned to the corpus so the merge keeps
/// path order.
enum Scanned {
    Rows(SpecRows),
    NoDirective,
    Unreadable,
}

/// One prolog file's frozen parse, read by the prescan and again by the drain.
type Parsed = AstGrep<ast_grep_core::tree_sitter::StrDoc<ExtractLang>>;

struct SpecRows {
    paths: Vec<String>,
    parse: Parsed,
}

/// One row of the rel the fact matcher reads: which file wrote which spec, and
/// which file that spec names.
struct CandidateRow {
    path: String,
    raw: String,
    target: String,
}

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

/// Every spec the rule finds, in source order. A spec is kept as written;
/// resolution and re-aiming are the caller's.
fn specifiers(rule: &RuleConfig<ExtractLang>, text: String) -> SpecRows {
    let parse = AstGrep::new(text, ExtractLang::Prolog);
    let mut paths: Vec<String> = parse
        .root()
        .find_all(&rule.matcher)
        .map(|matched| matched.text().to_string())
        .collect();
    paths.dedup();
    SpecRows { paths, parse }
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

/// One file's references off the ONE parse the prescan made. The fact matchers
/// keep the specs the store names; the drain picks the spans, `respell` the text.
fn drain_refs(
    rel: &str,
    parse: &Parsed,
    rule: &RuleConfig<ExtractLang>,
    facts: &Arc<FactSet>,
    targets: &BTreeMap<(String, String), String>,
) -> Vec<ImportRef> {
    let root = parse.root();
    let named = Any::new(
        facts
            .values()
            .map(|raw| facts.matcher(raw))
            .collect::<Vec<_>>(),
    );
    let matcher = Op::every(&rule.matcher).and(named);
    let edits = crate::drain::drain_edits(&root, &matcher, &Verbatim);
    tracing::debug!(rel, edits = edits.len(), named = facts.len(), "move drain");
    edits
        .into_iter()
        .filter_map(|edit| {
            let start = edit.position;
            let end = start + edit.deleted_length;
            let text = parse
                .root()
                .get_doc()
                .get_source()
                .get(start..end)?
                .to_string();
            let target = targets.get(&(rel.to_string(), text.clone()))?.clone();
            Some(ImportRef {
                importer: rel.to_string(),
                literal: Span {
                    start: start as u32,
                    len: edit.deleted_length as u32,
                },
                text,
                target,
                kind: ImportRefKind::Import,
            })
        })
        .collect()
}

/// The drain picks spans, never text: the replacement is `Rehome::respell`'s.
struct Verbatim;

impl<D: Doc<Source = String>> Replacer<D> for Verbatim {
    fn generate_replacement(&self, matched: &NodeMatch<'_, D>) -> Underlying<D> {
        matched.text().as_bytes().to_vec()
    }
}

/// A prolog file spec resolves against the loading file's directory and takes
/// `.pl` when bare; `library(...)` and every other alias term names no file here.
fn resolve(cx: &MoveCx, dir: &str, raw: &str) -> Option<String> {
    let (_, bare) = unquote(raw);
    if bare.is_empty() || bare.contains('(') || bare.starts_with('/') {
        return None;
    }
    let joined = join_rel(dir, bare);
    if cx.contains(&joined) {
        return Some(joined);
    }
    let with_extension = format!("{joined}.pl");
    cx.contains(&with_extension).then_some(with_extension)
}

/// The spec text `original` becomes once it aims at `target` from `from_dir`,
/// keeping the original's quoting and its `.pl`-or-bare spelling.
fn spec_text(from_dir: &str, target: &str, original: &str) -> String {
    let (quote, bare) = unquote(original);
    let relative = relative_between(from_dir, target);
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
