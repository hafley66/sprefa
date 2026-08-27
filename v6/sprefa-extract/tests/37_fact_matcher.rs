//! `FactMatcher`: a stored (rel, column) value set as an ast-grep matcher.
//! Membership, composition with a pattern, and the once-per-run db read.
//!
//! @comment-ok: sabotage receipt, repo law keeps these in TEST headers.
//! SABOTAGE: `FactMatcher::new` rewritten to `present: true` (the store never
//! consulted, text equality alone) measured 3 failed / 3 passed, and
//! `narrows_a_pattern_to_the_stored_names` was one of the GREEN ones: every
//! value it composes IS in the store, so narrowing a pattern judges nothing
//! about absence. `absent_value_matches_nothing` is what catches it.
//! SABOTAGE: `FactSet::load` rewritten to `SELECT DISTINCT t."<column>"` (the
//! raw surrogate ids, no `__str` join) measured 5 failed / 1 passed: the
//! dictionary join is what every membership assertion rests on.
//! FAIL-FIRST: `one_statement_per_run_however_many_nodes` with the preload moved
//! inside the node walk measured 31 statements against its `assert_eq!(.., 1)`,
//! so it fails on a per-node query and only on that.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use ast_grep_core::{AstGrep, Pattern};
use rusqlite::trace::{TraceEvent, TraceEventCodes};
use rusqlite::Connection;
use sprefa_extract::{ExtractLang, FactError, FactSet};

const REL: &str = "callee";
const COLUMN: &str = "name";
const SRC: &str = "fn main() { alpha(); beta(); gamma(); }\n";

/// The store's shape: `__str` UNIQUE on the natural key, the rel keyed on
/// INTEGER surrogates referencing it (`.claude/skills/sql-relational-design`).
fn seeded(values: &[&str]) -> Connection {
    let store = Connection::open_in_memory().expect("in-memory store");
    store
        .execute_batch(
            "CREATE TABLE \"__str\" (\"__id\" INTEGER PRIMARY KEY, \"content\" TEXT NOT NULL UNIQUE);
             CREATE TABLE \"callee\" (\"__id\" INTEGER PRIMARY KEY, \"name\" INTEGER NOT NULL,
                UNIQUE (\"name\"));",
        )
        .expect("schema");
    for value in values {
        store
            .execute(
                "INSERT OR IGNORE INTO \"__str\" (\"content\") VALUES (?1)",
                [value],
            )
            .expect("intern");
        store
            .execute(
                "INSERT OR IGNORE INTO \"callee\" (\"name\")
                 SELECT \"__id\" FROM \"__str\" WHERE \"content\" = ?1",
                [value],
            )
            .expect("row");
    }
    store
}

fn rust() -> ExtractLang {
    ExtractLang::from_path("main.rs").expect("rust grammar")
}

#[test]
fn present_value_matches_the_node_and_absent_one_does_not() {
    let facts = Arc::new(FactSet::load(&seeded(&["beta", "gamma"]), REL, COLUMN).expect("preload"));
    assert_eq!(facts.rel(), REL);
    assert_eq!(facts.column(), COLUMN);
    assert_eq!(facts.len(), 2, "the dictionary join returned the text");
    assert_eq!(facts.values().collect::<Vec<_>>(), vec!["beta", "gamma"]);

    let root = AstGrep::new(SRC, rust());
    let hits: Vec<_> = root
        .root()
        .find_all(&facts.matcher("beta"))
        .map(|node| node.range())
        .collect();
    assert_eq!(hits, vec![21..25], "the identifier `beta`: {SRC:?}");

    assert_eq!(
        root.root().find_all(&facts.matcher("alpha")).count(),
        0,
        "`alpha` is in the source and not in the store"
    );
}

#[test]
fn absent_value_matches_nothing() {
    let facts = Arc::new(FactSet::load(&seeded(&["beta"]), REL, COLUMN).expect("preload"));
    let matcher = facts.matcher("alpha");
    assert!(!matcher.present(), "the store does not carry `alpha`");
    assert_eq!(matcher.value(), "alpha");

    let root = AstGrep::new(SRC, rust());
    assert_eq!(root.root().find_all(&matcher).count(), 0);
}

#[test]
fn narrows_a_pattern_to_the_stored_names() {
    let pattern = Pattern::try_new("$NAME()", rust()).expect("call pattern");
    let root = AstGrep::new(SRC, rust());
    let before = root.root().find_all(&pattern).count();
    assert_eq!(before, 3, "alpha, beta and gamma: {SRC:?}");

    let facts = Arc::new(
        FactSet::load(&seeded(&["beta()", "gamma()", "delta()"]), REL, COLUMN).expect("preload"),
    );
    let narrowed: Vec<_> = root
        .root()
        .find_all(
            &ast_grep_core::ops::Op::every(&pattern).and(ast_grep_core::ops::Any::new([
                facts.matcher("beta()"),
                facts.matcher("gamma()"),
                facts.matcher("delta()"),
            ])),
        )
        .map(|node| node.text().to_string())
        .collect();
    assert_eq!(narrowed, vec!["beta()", "gamma()"]);
    assert!(
        narrowed.len() < before,
        "{before} matches narrowed to {narrowed:?}"
    );
}

static TRACED: AtomicUsize = AtomicUsize::new(0);

fn count_statement(event: TraceEvent<'_>) {
    if matches!(event, TraceEvent::Stmt(..)) {
        TRACED.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn one_statement_per_run_however_many_nodes() {
    let store = seeded(&["beta", "gamma"]);
    TRACED.store(0, Ordering::SeqCst);
    store.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(count_statement));

    let facts = Arc::new(FactSet::load(&store, REL, COLUMN).expect("preload"));
    assert_eq!(
        TRACED.load(Ordering::SeqCst),
        1,
        "the preload is ONE statement"
    );

    let root = AstGrep::new(SRC, rust());
    let nodes = root.root().dfs().count();
    assert!(nodes > 20, "the walk visits {nodes} nodes");
    let matched: usize = ["alpha", "beta", "gamma"]
        .iter()
        .map(|name| root.root().find_all(&facts.matcher(*name)).count())
        .sum();
    assert_eq!(matched, 2);

    store.trace_v2(TraceEventCodes::empty(), None);
    assert_eq!(
        TRACED.load(Ordering::SeqCst),
        1,
        "{nodes} nodes and 3 matchers still cost the one preload"
    );
}

#[test]
fn a_name_that_is_not_an_identifier_is_refused() {
    let store = seeded(&["beta"]);
    assert_eq!(
        FactSet::load(&store, "callee\"; DROP TABLE \"__str", COLUMN),
        Err(FactError::Name("callee\"; DROP TABLE \"__str".into())),
    );
    assert_eq!(
        FactSet::load(&store, REL, ""),
        Err(FactError::Name(String::new())),
    );
}

#[test]
fn the_live_store_opens_read_only_and_preloads_once() {
    let path = match sprefa_extract::dl6_db_path() {
        Ok(path) if path.is_file() => path,
        _ => return,
    };
    let store = sprefa_extract::open_readonly(&path).expect("the live store opens read-only");
    let refused = store
        .execute(
            "CREATE TABLE \"__arc_c_probe\" (\"__id\" INTEGER PRIMARY KEY)",
            [],
        )
        .expect_err("a read-only connection cannot write the one server's db");
    assert!(
        refused.to_string().contains("readonly"),
        "{refused} names the read-only refusal"
    );

    let facts = FactSet::load(&store, "import_graph_candidate", "raw").expect("preload");
    assert_eq!(facts.rel(), "import_graph_candidate");
    assert!(
        facts.values().all(|value| !value.is_empty()),
        "every spec text came back through the __str join"
    );
}

/// The move's own rule file and grammar: the fact set is what turns "every load
/// directive in this file" into "the ones naming the moved file".
const MOVE_RULE: &str = include_str!("../rules/move_specifier.yml");
const IMPORTER_PL: &str = ":- module(a, []).\n:- use_module('lib/b').\n:- use_module('lib/c', [c/1]).\n:- include('parts/d.pl').\n";

#[test]
fn the_move_rule_finds_every_spec_and_the_facts_keep_one() {
    let rule: ast_grep_config::RuleConfig<ExtractLang> = ast_grep_config::from_yaml_string(
        &format!("language: prolog\n{MOVE_RULE}"),
        &ast_grep_config::GlobalRules::default(),
    )
    .expect("the committed rule decodes")
    .into_iter()
    .next()
    .expect("one rule");

    let root = AstGrep::new(IMPORTER_PL, ExtractLang::Prolog);
    let every: Vec<_> = root
        .root()
        .find_all(&rule.matcher)
        .map(|node| node.text().to_string())
        .collect();
    assert_eq!(
        every,
        vec!["'lib/b'", "'lib/c'", "'parts/d.pl'"],
        "the rule reaches the one- and two-argument forms and `include`"
    );

    let facts = Arc::new(FactSet::load(&seeded(&["'lib/b'"]), REL, COLUMN).expect("preload"));
    let kept: Vec<_> = root
        .root()
        .find_all(
            &ast_grep_core::ops::Op::every(&rule.matcher).and(ast_grep_core::ops::Any::new(
                facts
                    .values()
                    .map(|raw| facts.matcher(raw))
                    .collect::<Vec<_>>(),
            )),
        )
        .map(|node| node.text().to_string())
        .collect();
    assert_eq!(kept, vec!["'lib/b'"], "3 specs, 1 candidate");
}
