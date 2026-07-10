//! `dl q <verb>` — concept verbs over the code graph.
//!
//! A verb is an embedded `.dl` program plus a parameter-injection seam: the
//! runner synthesizes a `q_target("<arg>")` fact and merges it in front of the
//! verb body (the `--move` synthesized-program precedent — the policy lives in a
//! program, not in Rust; wasm-generality law). The merged program is evaluated
//! ephemerally — a fresh in-process engine ([`run_inproc`]) for the no-daemon
//! path, or the daemon's `q` RPC (which evals against a scratch engine, never
//! the served one) — and the body's single `?` query is the answer.
//!
//! Anchor resolution rides [`crate::anchor::resolve_name`]: the runner reports
//! how the raw name resolved (candidate count + families) as an advisory note,
//! so a caller sees ambiguity / zero-resolution instead of guessing from empty
//! rows.
//!
//! Discovery: `verb_catalog(verb, args, doc)` is a catalogued builtin meta rel
//! (src/rels/catalog.rs) projected from [`verb_specs`], so `? verb_catalog(...)`
//! and a bare `dl q` list the same set. No literal-name read anywhere — the rel
//! is a `RelDecl` like `rel_catalog`, so the magic-rel audit stays green.

use std::path::PathBuf;

use anyhow::Result;

use crate::anchor::{Anchor, QueryOut};
use crate::engine::{Engine, QueryResult};

/// One concept verb: its name, the arg sketch shown in the catalog, a one-line
/// doc, and the embedded `.dl` body (which reads `q_target` + the warm builtin
/// tables and ends in exactly one `?` query — that query IS the answer shape).
pub struct VerbSpec {
    pub name: &'static str,
    pub args: &'static str,
    pub doc: &'static str,
    pub body: &'static str,
}

/// The scan block that forces the code-graph families to extract on a cold
/// engine (mirrors `anchor::probe_program`'s corpus). Harmless when merged onto
/// a served program that already scans: the extra `scan` rules just re-populate
/// `_file` idempotently.
const SCAN_BLOCK: &str = r#"
rel _q_scan(path: file).
_q_scan(path) <- scan("WORK", "**/*.rs", path, rev).
_q_scan(path) <- scan("WORK", "**/*.{ts,tsx,js,jsx,mjs,cjs}", path, rev).
_q_scan(path) <- scan("WORK", "**/*.{kt,kts}", path, rev).
_q_scan(path) <- scan("WORK", "**/*.py", path, rev).
_q_scan(path) <- scan("WORK", "**/*.go", path, rev).
_q_scan(path) <- scan("WORK", "**/*.java", path, rev).
"#;

/// `who-calls <name>`: callers of the function named `<name>`. `call_name`
/// resolves the bare target name to its def sym, `call_edge` walks callers, and
/// a second `call_name`/`call_def` join names + locates each caller.
const WHO_CALLS_BODY: &str = r#"
rel q_who_calls(caller: text, kind: text, file: file, line: int, callee: text).
q_who_calls(caller_name, caller_kind, caller_file, caller_line, target_name) <-
    q_target(target_name),
    call_name(callee_sym, target_name),
    call_edge(caller_sym, callee_sym, _),
    call_def(_, caller_sym, caller_kind, caller_file, caller_line, _),
    call_name(caller_sym, caller_name).
? q_who_calls(caller, kind, file, line, callee).
"#;

/// `where-defined <name>`: definition sites of `<name>` across the type, call,
/// and scip families (union). scip carries no line, so its rows report line 0.
const WHERE_DEFINED_BODY: &str = r#"
rel q_where_defined(sym: text, kind: text, file: file, line: int, name: text).
q_where_defined(entity_sym, entity_kind, entity_file, entity_line, target_name) <-
    q_target(target_name),
    type_entity(_, entity_sym, target_name, entity_kind, _, entity_file, entity_line).
q_where_defined(def_sym, def_kind, def_file, def_line, target_name) <-
    q_target(target_name),
    call_name(def_sym, target_name),
    call_def(_, def_sym, def_kind, def_file, def_line, _).
q_where_defined(scip_sym, "scip", scip_file, 0, target_name) <-
    q_target(target_name),
    scip_name(scip_sym, target_name),
    scip_def(scip_sym, scip_file, _).
? q_where_defined(sym, kind, file, line, name).
"#;

/// The verb table — the single source both the `dl q` runner and the
/// `verb_catalog` meta rel read.
pub fn verb_specs() -> &'static [VerbSpec] {
    &[
        VerbSpec {
            name: "who-calls",
            args: "<name>",
            doc: "callers of the function named <name>: (caller name, kind, file, line)",
            body: WHO_CALLS_BODY,
        },
        VerbSpec {
            name: "where-defined",
            args: "<name>",
            doc: "definition sites of <name> across the type / call / scip families",
            body: WHERE_DEFINED_BODY,
        },
    ]
}

/// Look up a verb by name.
pub fn find(name: &str) -> Option<&'static VerbSpec> {
    verb_specs().iter().find(|verb| verb.name == name)
}

/// Comma-joined verb names, for an unknown-verb error / bare-`dl q` listing.
pub fn verb_list() -> String {
    verb_specs()
        .iter()
        .map(|verb| verb.name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Escape a raw arg into a dl string literal (double-quoted; `"` and `\` escaped).
fn dl_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// The full embedded program source for a verb: family-forcing scan block +
/// the injected `q_target` fact + the verb body (which carries its own `?`).
pub fn verb_source(spec: &VerbSpec, arg: &str) -> String {
    format!(
        "{SCAN_BLOCK}\nrel q_target(name: text).\nq_target({}).\n{}",
        dl_string(arg),
        spec.body
    )
}

/// Parse + typecheck the verb source into a program (queries included).
pub(crate) fn verb_program(spec: &VerbSpec, arg: &str) -> Result<crate::ast::Program> {
    let toks = crate::lex::lex(&verb_source(spec, arg))?;
    let mut prog = crate::parse::parse(toks)?;
    // Warnings (path/text coercions on the scan block) are fine; an error would
    // surface at tick time. The body is fixed, so this never fails in practice.
    let _ = crate::typecheck::check_and_normalize(&mut prog, "<verb>");
    Ok(prog)
}

/// Stringify one captured query cell (Text as-is, everything else via JSON).
fn json_cell(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text,
        other => other.to_string(),
    }
}

/// Flatten the FIRST captured query result (the verb body has exactly one `?`)
/// into `(columns, rows)`.
pub(crate) fn shape(results: Vec<QueryResult>) -> (Vec<String>, Vec<Vec<String>>) {
    match results.into_iter().next() {
        Some(result) => {
            let rows = result
                .rows
                .into_iter()
                .map(|row| row.into_iter().map(json_cell).collect())
                .collect();
            (result.columns, rows)
        }
        None => (Vec::new(), Vec::new()),
    }
}

/// In-memory paging over a bounded row set.
pub(crate) fn page(rows: Vec<Vec<String>>, limit: Option<usize>, offset: Option<usize>) -> Vec<Vec<String>> {
    let off = offset.unwrap_or(0);
    let mut out: Vec<Vec<String>> = rows.into_iter().skip(off).collect();
    if let Some(lim) = limit {
        out.truncate(lim);
    }
    out
}

/// The advisory resolution note (resolution rides `anchor::resolve_name`). A
/// name that resolves to nothing / to many candidates is surfaced explicitly so
/// a small model doesn't misread empty rows as "no such symbol vs. no callers".
pub(crate) fn resolve_note(eng: &Engine, arg: &str) -> String {
    match crate::anchor::classify(arg) {
        Anchor::Name(pattern) => {
            let hits = crate::anchor::resolve_name(eng, &pattern).unwrap_or_default();
            if hits.is_empty() {
                format!("resolved: no symbol named {arg:?} (checked type / call / scip / module / dataflow)")
            } else {
                let mut families: Vec<&str> = hits.iter().map(|hit| hit.family).collect();
                families.sort_unstable();
                families.dedup();
                format!("resolved: {} candidate(s) across {}", hits.len(), families.join(", "))
            }
        }
        _ => format!("resolved: {arg:?} is not a bare symbol name; who-calls / where-defined expect a name"),
    }
}

/// Cold in-process verb run: a fresh engine ticks the embedded program, the
/// body's `?` is captured, paged, and returned with the resolution note. Used by
/// the `dl q` no-daemon fallback and the MCP `Local` pump.
pub fn run_inproc(
    root: PathBuf,
    db: Option<&str>,
    verb: &str,
    arg: &str,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<QueryOut> {
    let Some(spec) = find(verb) else {
        anyhow::bail!("unknown verb {verb:?}; available verbs: {}", verb_list());
    };
    let prog = verb_program(spec, arg)?;
    let conn = crate::db::open(db)?;
    let mut eng = Engine::new(conn, root);
    eng.set_repos(crate::daemon::load_repos_eager());
    eng.tick(&prog, true)?;
    let results = eng.run_queries_capture(&prog)?;
    let (columns, rows) = shape(results);
    let total = rows.len();
    let rows = page(rows, limit, offset);
    let notes = vec![resolve_note(&eng, arg)];
    Ok(QueryOut { columns, rows, total, notes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_verb_body_parses() {
        // The embedded bodies are fixed source; each must lex + parse cleanly
        // with a target injected (the runner never ships a broken program).
        for spec in verb_specs() {
            verb_program(spec, "Example").unwrap_or_else(|e| panic!("{}: {e}", spec.name));
        }
    }

    #[test]
    fn dl_string_escapes() {
        assert_eq!(dl_string("plain"), "\"plain\"");
        assert_eq!(dl_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(dl_string("a\\b"), "\"a\\\\b\"");
    }

    #[test]
    fn find_and_list() {
        assert!(find("who-calls").is_some());
        assert!(find("where-defined").is_some());
        assert!(find("nope").is_none());
        assert!(verb_list().contains("who-calls"));
    }
}
