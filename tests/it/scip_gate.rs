//! The scip family must load for call/type programs that never NAME a scip
//! rel, the load must precede extraction within the tick, AND the resolver must
//! consult occurrence POSITION before the bare descriptor name. Regressions for
//! the bugs the otel corpus measurement surfaced (2026-07-10):
//! (1) `ScipKind::used` defaulted to "program names a scip rel", so
//! `SPREFA_SCIP_INDEX` was a silent no-op for exactly the call/type programs
//! it should improve; (2) the index loaded in the RelKind loop AFTER the
//! extract families, so even a forced load only reached the resolvers on
//! tick 2 via the digest fold; (3) the override was NAME-level, so a name
//! carried by two def symbols in one file was dropped entirely (the conflict
//! refusal) — occurrence-level resolution uses `scip_occurrence`'s exact per-
//! call line to tell same-name symbols apart, and the conflict refusal becomes
//! moot wherever a position exists.

use std::fs;
use std::path::{Path, PathBuf};

use scip::types::{Document, Index, Occurrence, SymbolRole};

use sprefa_v5::db;
use sprefa_v5::engine::Engine;
use sprefa_v5::prepare_paths;

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_scipgate_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

/// An occurrence with no packed range: feeds `scip_def`/`scip_ref` (the
/// name-level path), but NOT `scip_occurrence` (which requires a range). Used to
/// exercise the name-map fallback where no position exists.
fn occurrence(symbol: &str, roles: i32) -> Occurrence {
    let mut occ = Occurrence::new();
    occ.symbol = symbol.to_string();
    occ.symbol_roles = roles;
    occ
}

/// An occurrence WITH a packed 0-based range `[start_line, start_col, end_line,
/// end_col]`: this is what lands a `scip_occurrence` row, so the resolver can
/// pick the symbol by the call site's line.
fn occurrence_r(symbol: &str, roles: i32, range: [i32; 4]) -> Occurrence {
    let mut occ = Occurrence::new();
    occ.symbol = symbol.to_string();
    occ.symbol_roles = roles;
    occ.range = range.to_vec();
    occ
}

fn document(path: &str, occurrences: Vec<Occurrence>) -> Document {
    let mut doc = Document::new();
    doc.relative_path = path.to_string();
    doc.occurrences = occurrences;
    doc
}

fn write_index(path: &Path, docs: Vec<Document>) {
    let mut index = Index::new();
    index.documents = docs;
    scip::write_message_to_file(path, index).unwrap();
}

/// descriptor name parses to `helper`, matching the call site's text.
const SYM: &str = "scip-go gomod fixture 1.0.0 `main`/helper().";
/// A DIFFERENT symbol whose trailing descriptor name is also `helper`.
const SYM_OTHER: &str = "scip-go gomod fixture 1.0.0 `other`/helper().";

/// Two same-package defs of `helper` make the syntactic tier's ambiguity
/// bucket a tie (stays bare); only the index knows the real def file.
fn write_fixture(root: &Path) {
    fs::write(root.join("a.go"), "package main\n\nfunc helper() {}\n").unwrap();
    fs::write(root.join("b.go"), "package main\n\nfunc helper() {}\n").unwrap();
    fs::write(
        root.join("caller.go"),
        "package main\n\nfunc caller() {\n\thelper()\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("p.dl"),
        "rel seen(path: file).\n\
         seen(path) <- scan(\"WORK\", \"**/*.go\", path, rev).\n\
         rel edge(caller_sym: text, callee_sym: text).\n\
         edge(caller_sym, callee_sym) <- call_edge(caller_sym, callee_sym, kind).\n\
         rel site(caller_sym: text, callee_name: text).\n\
         site(caller_sym, callee_name) <- call_site(repo, caller_sym, callee_name, path, line).\n",
    )
    .unwrap();
}

/// A caller with TWO `helper()` calls on different lines (0-based rows 3 and 4),
/// so the index can attribute each to a different `helper` def by position.
fn write_two_call_fixture(root: &Path) {
    fs::write(root.join("a.go"), "package main\n\nfunc helper() {}\n").unwrap();
    fs::write(root.join("b.go"), "package main\n\nfunc helper() {}\n").unwrap();
    fs::write(
        root.join("caller.go"),
        // rows: 0 package, 1 blank, 2 func, 3 helper(), 4 helper(), 5 close
        "package main\n\nfunc caller() {\n\thelper()\n\thelper()\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("p.dl"),
        "rel seen(path: file).\n\
         seen(path) <- scan(\"WORK\", \"**/*.go\", path, rev).\n\
         rel edge(caller_sym: text, callee_sym: text).\n\
         edge(caller_sym, callee_sym) <- call_edge(caller_sym, callee_sym, kind).\n\
         rel site(caller_sym: text, callee_name: text).\n\
         site(caller_sym, callee_name) <- call_site(repo, caller_sym, callee_name, path, line).\n",
    )
    .unwrap();
}

/// One tick; returns (edge rows, site rows).
fn graph_after_one_tick(root: &Path) -> (Vec<Vec<String>>, Vec<Vec<String>>) {
    let conn = db::open(Some(root.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, root.to_path_buf());
    let (prog, diags, _) = prepare_paths(&[root.join("p.dl")]).unwrap();
    let errs = diags
        .iter()
        .filter(|x| x.severity == sprefa_v5::ast::Severity::Error)
        .count();
    assert_eq!(errs, 0, "program should typecheck: {diags:?}");
    eng.tick(&prog, true).unwrap();
    (eng.rel_rows("edge", 2), eng.rel_rows("site", 2))
}

/// Control: with no index the two-def tie stays bare, proving the fixture is
/// genuinely undecidable for the syntactic tier (the positive twin below is
/// not passing by accident of name resolution).
#[test]
fn ambiguous_call_stays_bare_without_index() {
    let d = sandbox("bare");
    write_fixture(&d);
    let (edges, sites) = graph_after_one_tick(&d);
    assert!(
        sites
            .iter()
            .any(|r| r[0].contains("caller.go") && r[1] == "helper"),
        "the call site itself must exist: {sites:?}"
    );
    assert!(
        !edges.iter().any(|r| r[0].contains("caller.go")),
        "two same-dir defs must stay unresolved (no edge) syntactically: {edges:?}"
    );
}

/// The real assertion: a root index.scip resolves the call on the FIRST tick
/// of a program that names no scip rel. The occurrences carry NO range, so this
/// also exercises the name-level fallback (no `scip_occurrence` rows to pick
/// from — the resolver falls back to the (repo, file, name) map).
#[test]
fn index_resolves_call_first_tick_without_naming_scip_rels() {
    let d = sandbox("first");
    write_fixture(&d);
    write_index(
        &d.join("index.scip"),
        vec![
            document("b.go", vec![occurrence(SYM, SymbolRole::Definition as i32)]),
            document("caller.go", vec![occurrence(SYM, 0)]),
        ],
    );
    let (edges, _) = graph_after_one_tick(&d);
    let callee = &edges
        .iter()
        .find(|r| r[0].contains("caller.go"))
        .unwrap_or_else(|| panic!("no edge from caller.go: {edges:?}"))[1];
    assert!(
        callee.contains("b.go::"),
        "first tick must resolve via the index to b.go, got {callee}"
    );
}

/// Occurrence-level resolution: caller.go calls `helper` on two different lines,
/// and the index records that line 3 (0-based) is `SYM` (def b.go) while line 4
/// is `SYM_OTHER` (def a.go). The NAME-level map drops `helper` outright — one
/// file referencing two symbols of that name is the conflict refusal (9fd029b) —
/// so WITHOUT position both sites are bare (the ambiguous_call control proves
/// the syntactic tie). WITH position each call resolves to its OWN def file.
#[test]
fn same_name_calls_resolve_by_position() {
    let d = sandbox("bypos");
    write_two_call_fixture(&d);
    write_index(
        &d.join("index.scip"),
        vec![
            document("b.go", vec![occurrence(SYM, SymbolRole::Definition as i32)]),
            document(
                "a.go",
                vec![occurrence(SYM_OTHER, SymbolRole::Definition as i32)],
            ),
            document(
                "caller.go",
                vec![
                    // first `helper()` at 0-based row 3 -> SYM (def b.go)
                    occurrence_r(SYM, 0, [3, 1, 3, 7]),
                    // second `helper()` at 0-based row 4 -> SYM_OTHER (def a.go)
                    occurrence_r(SYM_OTHER, 0, [4, 1, 4, 7]),
                ],
            ),
        ],
    );
    let (edges, _) = graph_after_one_tick(&d);
    let callees: Vec<&String> = edges
        .iter()
        .filter(|r| r[0].contains("caller.go"))
        .map(|r| &r[1])
        .collect();
    assert!(
        callees.iter().any(|c| c.contains("b.go::")),
        "the row-3 call must resolve to b.go by position: {edges:?}"
    );
    assert!(
        callees.iter().any(|c| c.contains("a.go::")),
        "the row-4 call must resolve to a.go by position: {edges:?}"
    );
}

/// The refusal that survives position: two DIFFERENT symbols that share the
/// descriptor name occur on the SAME line as one call. Line granularity cannot
/// tell them apart, so the resolver refuses (fails toward exclusion) rather than
/// guess — and does NOT fall through to a coincidental single-def name pick. The
/// name-level map likewise drops the conflicted name, so the site stays bare.
#[test]
fn same_line_same_name_symbols_stay_bare() {
    let d = sandbox("conflict");
    write_fixture(&d); // single `helper()` at 0-based row 3
    write_index(
        &d.join("index.scip"),
        vec![
            document("b.go", vec![occurrence(SYM, SymbolRole::Definition as i32)]),
            document(
                "a.go",
                vec![occurrence(SYM_OTHER, SymbolRole::Definition as i32)],
            ),
            document(
                "caller.go",
                vec![
                    // BOTH symbols occur on the one call's line (row 3): ambiguous
                    occurrence_r(SYM, 0, [3, 1, 3, 7]),
                    occurrence_r(SYM_OTHER, 0, [3, 1, 3, 7]),
                ],
            ),
        ],
    );
    let (edges, sites) = graph_after_one_tick(&d);
    assert!(
        sites
            .iter()
            .any(|r| r[0].contains("caller.go") && r[1] == "helper"),
        "the call site itself must exist: {sites:?}"
    );
    assert!(
        !edges.iter().any(|r| r[0].contains("caller.go")),
        "two same-name symbols on the call's line must stay unresolved: {edges:?}"
    );
}

/// Fallback: a caller whose file has NO `scip_occurrence` rows (its reference
/// occurrence carries no range) still resolves through the name-level map exactly
/// as before the occurrence tier existed. Here only b.go defines `helper`, so
/// the name map is unambiguous and the call resolves to b.go.
#[test]
fn name_map_resolves_when_file_has_no_occurrences() {
    let d = sandbox("fallback");
    // Only b.go defines helper; caller.go calls it. No a.go decoy this time, so
    // even the syntactic tier could resolve — the point is that the RANGE-less
    // occurrences produce zero scip_occurrence rows, so the occurrence tier is a
    // Fallthrough and the name map carries.
    fs::write(d.join("b.go"), "package main\n\nfunc helper() {}\n").unwrap();
    fs::write(
        d.join("caller.go"),
        "package main\n\nfunc caller() {\n\thelper()\n}\n",
    )
    .unwrap();
    fs::write(
        d.join("p.dl"),
        "rel seen(path: file).\n\
         seen(path) <- scan(\"WORK\", \"**/*.go\", path, rev).\n\
         rel edge(caller_sym: text, callee_sym: text).\n\
         edge(caller_sym, callee_sym) <- call_edge(caller_sym, callee_sym, kind).\n\
         rel site(caller_sym: text, callee_name: text).\n\
         site(caller_sym, callee_name) <- call_site(repo, caller_sym, callee_name, path, line).\n",
    )
    .unwrap();
    write_index(
        &d.join("index.scip"),
        vec![
            document("b.go", vec![occurrence(SYM, SymbolRole::Definition as i32)]),
            document("caller.go", vec![occurrence(SYM, 0)]),
        ],
    );
    let (edges, _) = graph_after_one_tick(&d);
    let callee = &edges
        .iter()
        .find(|r| r[0].contains("caller.go"))
        .unwrap_or_else(|| panic!("no edge from caller.go (name-map fallback): {edges:?}"))[1];
    assert!(
        callee.contains("b.go::"),
        "name-map fallback must resolve to b.go, got {callee}"
    );
}

/// A re-index must reach the db even though the index file is OUTSIDE the
/// corpus (real repos gitignore it, so it never appears in a tick's changed
/// set). `ScipKind::dirty` stats the self index directly and compares against
/// the digest `refresh` stored — without that, the incremental tick below
/// keeps serving the v1 index forever (the two-day-stale `.dl/.state` index
/// this test retires).
#[test]
fn reindex_reloads_when_index_invisible_to_corpus() {
    let d = sandbox("reindex");
    write_fixture(&d);
    write_index(
        &d.join("index.scip"),
        vec![
            document("b.go", vec![occurrence(SYM, SymbolRole::Definition as i32)]),
            document("caller.go", vec![occurrence(SYM, 0)]),
        ],
    );
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let (prog, diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    assert_eq!(
        diags
            .iter()
            .filter(|x| x.severity == sprefa_v5::ast::Severity::Error)
            .count(),
        0,
        "program should typecheck: {diags:?}"
    );
    eng.tick(&prog, true).unwrap();
    let edges = eng.rel_rows("edge", 2);
    let callee = &edges
        .iter()
        .find(|r| r[0].contains("caller.go"))
        .unwrap_or_else(|| panic!("v1 index must resolve: {edges:?}"))[1];
    assert!(
        callee.contains("b.go::"),
        "v1 resolves to b.go, got {callee}"
    );

    // Re-index: v2 says helper's def is a.go. The index is NOT in the corpus
    // (p.dl scans only *.go), so the incremental tick's changed set is blind
    // to it — only the stat-digest gate can notice.
    write_index(
        &d.join("index.scip"),
        vec![
            document(
                "a.go",
                vec![occurrence(SYM_OTHER, SymbolRole::Definition as i32)],
            ),
            document("caller.go", vec![occurrence(SYM_OTHER, 0)]),
        ],
    );
    // An ordinary source edit drives the tick; the appended comment leaves the
    // call lines untouched.
    let caller = d.join("caller.go");
    let mut body = fs::read_to_string(&caller).unwrap();
    body.push_str("// touched\n");
    fs::write(&caller, body).unwrap();
    eng.tick_paths(&prog, std::slice::from_ref(&caller), true)
        .unwrap();

    let edges = eng.rel_rows("edge", 2);
    let callee = &edges
        .iter()
        .find(|r| r[0].contains("caller.go"))
        .unwrap_or_else(|| panic!("edge must survive the re-index: {edges:?}"))[1];
    assert!(
        callee.contains("a.go::"),
        "incremental tick must reload the re-written index (def moved to a.go), got {callee}"
    );
}

/// `index_path` picks the NEWEST existing candidate, not a fixed location
/// order: a fixed order let a stale `.dl/.state/index.scip` shadow a fresh
/// root `index.scip` for two days.
#[test]
fn index_path_newest_wins() {
    let d = sandbox("newestwins");
    let state = sprefa_v5::state_dir(&d);
    fs::create_dir_all(&state).unwrap();
    let doc = vec![document(
        "b.go",
        vec![occurrence(SYM, SymbolRole::Definition as i32)],
    )];
    write_index(&state.join("index.scip"), doc.clone());
    // APFS mtime is fine-grained, but hold a full second so coarse filesystems
    // order the two writes too.
    std::thread::sleep(std::time::Duration::from_millis(1050));
    write_index(&d.join("index.scip"), doc.clone());
    assert_eq!(
        sprefa_v5::scip_import::index_path(&d),
        Some(d.join("index.scip")),
        "fresher root index must win over the state-dir index"
    );
    std::thread::sleep(std::time::Duration::from_millis(1050));
    write_index(&state.join("index.scip"), doc);
    assert_eq!(
        sprefa_v5::scip_import::index_path(&d),
        Some(state.join("index.scip")),
        "fresher state-dir index must win over the root index"
    );
}
